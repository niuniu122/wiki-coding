use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use minimax_protocol::ContentHash;
use sha2::{Digest, Sha256};

use crate::VaultError;

pub(crate) fn content_hash(bytes: &[u8]) -> ContentHash {
    ContentHash::new(sha256_hex(bytes)).expect("SHA-256 is always valid")
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub(crate) fn atomic_create_or_same(path: &Path, bytes: &[u8]) -> Result<(), VaultError> {
    if path.exists() {
        return if std::fs::read(path).map_err(|_| VaultError::Io)? == bytes {
            Ok(())
        } else {
            Err(VaultError::Conflict)
        };
    }
    let parent = path.parent().ok_or(VaultError::InvalidPath)?;
    std::fs::create_dir_all(parent).map_err(|_| VaultError::Io)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|_| VaultError::Io)?;
    temporary.write_all(bytes).map_err(|_| VaultError::Io)?;
    temporary.flush().map_err(|_| VaultError::Io)?;
    temporary.as_file().sync_all().map_err(|_| VaultError::Io)?;
    match temporary.persist_noclobber(path) {
        Ok(file) => file.sync_all().map_err(|_| VaultError::Io),
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
            if std::fs::read(path).map_err(|_| VaultError::Io)? == bytes {
                Ok(())
            } else {
                Err(VaultError::Conflict)
            }
        }
        Err(_) => Err(VaultError::Io),
    }
}

pub(crate) fn create_new_file(path: &Path, bytes: &[u8]) -> Result<(), VaultError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| VaultError::Io)?;
    file.write_all(bytes).map_err(|_| VaultError::Io)?;
    file.flush().map_err(|_| VaultError::Io)?;
    file.sync_all().map_err(|_| VaultError::Io)
}
