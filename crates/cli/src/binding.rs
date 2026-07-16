use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use minimax_protocol::{ProjectId, SchemaVersion};
use minimax_vault::{ProjectVault, VaultError, hash_vault_bytes};
use serde::{Deserialize, Serialize};

use crate::ProjectVaultBinding;

const BINDING_PATH: &str = ".minimax/vault-binding.v1.json";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedProjectVault {
    pub binding: ProjectVaultBinding,
    pub created: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredVaultBinding {
    schema_version: SchemaVersion,
    project_id: ProjectId,
    vault_root: PathBuf,
}

pub fn resolve_project_vault(
    project_root: &Path,
    explicit_vault: Option<&Path>,
    explicit_project_id: Option<&str>,
    now_unix_ms: u64,
) -> Result<ResolvedProjectVault, VaultError> {
    let project_root = project_root
        .canonicalize()
        .map_err(|_| VaultError::InvalidPath)?;
    let binding_path = project_root.join(BINDING_PATH);
    let stored = load_binding(&binding_path)?;
    let requested_id = explicit_project_id
        .map(|value| ProjectId::new(value.to_owned()).map_err(|_| VaultError::InvalidPath))
        .transpose()?;
    let requested_vault = explicit_vault.map(absolute_path).transpose()?;

    let selected = if let Some(existing) = stored.as_ref() {
        if requested_id
            .as_ref()
            .is_some_and(|value| value != &existing.project_id)
            || requested_vault.as_ref().is_some_and(|value| {
                canonical_or_absolute(value) != canonical_or_absolute(&existing.vault_root)
            })
        {
            return Err(VaultError::ProjectMismatch);
        }
        existing.clone()
    } else {
        StoredVaultBinding {
            schema_version: SchemaVersion,
            project_id: requested_id.unwrap_or_else(|| default_project_id(&project_root)),
            vault_root: requested_vault
                .unwrap_or_else(|| ProjectVault::recommended_sibling(&project_root)),
        }
    };

    let vault = ProjectVault::bootstrap(
        &project_root,
        &selected.vault_root,
        selected.project_id.clone(),
        now_unix_ms,
    )?;
    let canonical_vault = vault
        .root()
        .canonicalize()
        .map_err(|_| VaultError::InvalidPath)?;
    let binding = ProjectVaultBinding {
        project_root,
        vault_root: canonical_vault,
        project_id: vault.manifest().project_id.clone(),
        created_at_unix_ms: vault.manifest().created_at_unix_ms,
    };
    drop(vault);

    let normalized = StoredVaultBinding {
        schema_version: SchemaVersion,
        project_id: binding.project_id.clone(),
        vault_root: binding.vault_root.clone(),
    };
    let created = stored.is_none();
    persist_binding(&binding_path, &normalized)?;
    Ok(ResolvedProjectVault { binding, created })
}

fn load_binding(path: &Path) -> Result<Option<StoredVaultBinding>, VaultError> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|_| VaultError::RecoveryRequired),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(VaultError::Io),
    }
}

fn persist_binding(path: &Path, binding: &StoredVaultBinding) -> Result<(), VaultError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|_| VaultError::Io)?;
    }
    let mut bytes = serde_json::to_vec_pretty(binding).map_err(|_| VaultError::Io)?;
    bytes.push(b'\n');
    match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(mut file) => {
            file.write_all(&bytes).map_err(|_| VaultError::Io)?;
            file.flush().map_err(|_| VaultError::Io)?;
            file.sync_all().map_err(|_| VaultError::Io)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let existing = std::fs::read(path).map_err(|_| VaultError::Io)?;
            if existing == bytes {
                Ok(())
            } else {
                Err(VaultError::Conflict)
            }
        }
        Err(_) => Err(VaultError::Io),
    }
}

fn default_project_id(project_root: &Path) -> ProjectId {
    let normalized = project_root
        .to_string_lossy()
        .replace('\\', "/")
        .to_lowercase();
    let hash = hash_vault_bytes(normalized.as_bytes());
    ProjectId::new(format!("project:{}", &hash.as_str()[..24])).expect("stable project ID")
}

fn absolute_path(path: &Path) -> Result<PathBuf, VaultError> {
    if path.as_os_str().is_empty() {
        return Err(VaultError::InvalidPath);
    }
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        std::env::current_dir()
            .map_err(|_| VaultError::Io)
            .map(|current| current.join(path))
    }
}

fn canonical_or_absolute(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}
