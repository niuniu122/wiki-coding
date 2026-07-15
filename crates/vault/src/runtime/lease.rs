use std::fs::{File, OpenOptions};
use std::path::Path;

use fs4::{FileExt, TryLockError};

use super::RuntimeStoreError;

pub(crate) struct WorkspaceLease {
    file: File,
}

impl WorkspaceLease {
    pub(crate) fn acquire(runtime_dir: &Path) -> Result<Self, RuntimeStoreError> {
        let path = runtime_dir.join("writer.lock");
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
