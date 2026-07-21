use std::fs::{File, OpenOptions};
use std::path::Path;

use fs4::{FileExt, TryLockError};

use super::RuntimeStoreError;

pub(crate) struct WorkspaceLease {
    file: File,
}

impl WorkspaceLease {
    pub(crate) fn probe(path: &Path) -> Result<Option<Self>, RuntimeStoreError> {
        let file = match OpenOptions::new().read(true).write(true).open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(RuntimeStoreError::Io),
        };
        match FileExt::try_lock(&file) {
            Ok(()) => Ok(Some(Self { file })),
            Err(TryLockError::WouldBlock) => Err(RuntimeStoreError::Busy),
            Err(TryLockError::Error(_)) => Err(RuntimeStoreError::Io),
        }
    }

    pub(crate) fn acquire(runtime_dir: &Path) -> Result<Self, RuntimeStoreError> {
        Self::acquire_path(&runtime_dir.join("writer.lock"))
    }

    pub(crate) fn acquire_path(path: &Path) -> Result<Self, RuntimeStoreError> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(|_| RuntimeStoreError::Io)?;
        match FileExt::try_lock(&file) {
            Ok(()) => Ok(Self { file }),
            Err(TryLockError::WouldBlock) => Err(RuntimeStoreError::Busy),
            Err(TryLockError::Error(_)) => Err(RuntimeStoreError::Io),
        }
    }
}

impl Drop for WorkspaceLease {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}
