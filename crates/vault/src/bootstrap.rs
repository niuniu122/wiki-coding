use std::fmt;
use std::path::{Path, PathBuf};

use minimax_protocol::{ProjectId, SchemaVersion, VaultManifest, VaultOwnership};

use crate::path::{atomic_create_or_same, content_hash};
use crate::runtime::RuntimeStoreError;
use crate::runtime::lease::WorkspaceLease;

const MANIFEST_PATH: &str = ".minimax/manifest.json";
const AGENT_GUIDANCE: &[u8] = b"# MiniMax Project Vault\n\n`inbox/` is yours to edit. `raw/`, `wiki/`, `log.md`, and `.minimax/` are managed by MiniMax Codex. Keep this Vault private: its files are readable on this computer.\n";
const EMPTY_INDEX: &[u8] = b"# Project Wiki\n\n";
const EMPTY_LOG: &[u8] = b"# MiniMax Knowledge Log\n\n";

const FIXED_DIRECTORIES: [&str; 19] = [
    "inbox",
    "raw/sessions",
    "raw/imports",
    "raw/assets",
    "wiki/sessions",
    "wiki/projects",
    "wiki/decisions",
    "wiki/concepts",
    "wiki/providers",
    "wiki/lessons",
    ".minimax/locks",
    ".minimax/pending",
    ".minimax/transactions",
    ".minimax/recovery",
    ".minimax/indexes",
    ".minimax/trash",
    ".minimax/receipts",
    ".minimax/finalized",
    ".minimax/imports",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VaultError {
    Busy,
    Io,
    InvalidPath,
    ProjectMismatch,
    UnsupportedSchema,
    Conflict,
    RecoveryRequired,
    Finalized,
    SensitiveContent,
    SessionNotTerminal,
    SessionNotFound,
    InvalidPage,
    EmptyTransaction,
    FaultInjected,
    InvalidConfirmation,
    NotExpired,
    Expired,
}

impl fmt::Display for VaultError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Busy => "the project Vault is already open for writing",
            Self::Io => "the project Vault could not complete a local file operation",
            Self::InvalidPath => "the project or Vault path is invalid",
            Self::ProjectMismatch => "the Vault is bound to a different project",
            Self::UnsupportedSchema => "the Vault uses an unsupported schema",
            Self::Conflict => "an Agent-owned Vault file changed unexpectedly",
            Self::RecoveryRequired => "the Vault contains corruption that requires explicit repair",
            Self::Finalized => "the session evidence is already finalized with different bytes",
            Self::SensitiveContent => {
                "sensitive or private reasoning content cannot enter the Vault"
            }
            Self::SessionNotTerminal => "the session is not terminal and cannot be finalized",
            Self::SessionNotFound => "the session does not exist in the runtime journal",
            Self::InvalidPage => "the Wiki page or frontmatter is invalid",
            Self::EmptyTransaction => "the Wiki transaction has no changed targets",
            Self::FaultInjected => {
                "the test interrupted the Wiki transaction at a durable boundary"
            }
            Self::InvalidConfirmation => "the exact plan-bound confirmation token is required",
            Self::NotExpired => "the trash plan has not expired and cannot be purged",
            Self::Expired => "the trash plan has expired and can no longer be undone",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for VaultError {}

impl From<RuntimeStoreError> for VaultError {
    fn from(value: RuntimeStoreError) -> Self {
        match value {
            RuntimeStoreError::Busy => Self::Busy,
            RuntimeStoreError::Finalized => Self::Finalized,
            RuntimeStoreError::Io => Self::Io,
            _ => Self::RecoveryRequired,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum VaultWarning {
    PlaintextLocalFiles,
    VaultInsideProject,
    VaultInsideGitWorkTree,
}

pub struct ProjectVault {
    root: PathBuf,
    manifest: VaultManifest,
    warnings: Vec<VaultWarning>,
    _lease: Option<WorkspaceLease>,
}

impl ProjectVault {
    pub fn bootstrap(
        project_root: impl AsRef<Path>,
        vault_root: impl AsRef<Path>,
        project_id: ProjectId,
        created_at_unix_ms: u64,
    ) -> Result<Self, VaultError> {
        let project_root = project_root
            .as_ref()
            .canonicalize()
            .map_err(|_| VaultError::InvalidPath)?;
        let vault_root = absolute_path(vault_root.as_ref())?;
        std::fs::create_dir_all(vault_root.join(".minimax/locks")).map_err(|_| VaultError::Io)?;
        let lease = WorkspaceLease::acquire_path(&vault_root.join(".minimax/locks/writer.lock"))?;
        let project_fingerprint = content_hash(normalized_path(&project_root).as_bytes());
        let expected = VaultManifest {
            schema_version: SchemaVersion,
            project_id,
            project_fingerprint,
            created_at_unix_ms,
        };
        let manifest_path = vault_root.join(MANIFEST_PATH);
        let manifest = if manifest_path.exists() {
            let bytes = std::fs::read(&manifest_path).map_err(|_| VaultError::Io)?;
            let existing: VaultManifest =
                serde_json::from_slice(&bytes).map_err(|_| VaultError::UnsupportedSchema)?;
            if existing.project_id != expected.project_id
                || existing.project_fingerprint != expected.project_fingerprint
            {
                return Err(VaultError::ProjectMismatch);
            }
            existing
        } else {
            expected
        };

        for relative in FIXED_DIRECTORIES {
            std::fs::create_dir_all(vault_root.join(relative)).map_err(|_| VaultError::Io)?;
        }
        let mut manifest_bytes =
            serde_json::to_vec_pretty(&manifest).map_err(|_| VaultError::Io)?;
        manifest_bytes.push(b'\n');
        atomic_create_or_same(&manifest_path, &manifest_bytes)?;
        create_file_if_missing(&vault_root.join("AGENTS.md"), AGENT_GUIDANCE)?;
        create_file_if_missing(&vault_root.join("wiki/index.md"), EMPTY_INDEX)?;
        create_file_if_missing(&vault_root.join("log.md"), EMPTY_LOG)?;

        let mut warnings = vec![VaultWarning::PlaintextLocalFiles];
        let vault_location = vault_root
            .canonicalize()
            .unwrap_or_else(|_| vault_root.clone());
        if vault_location.starts_with(&project_root) {
            warnings.push(VaultWarning::VaultInsideProject);
        }
        if has_git_ancestor(&vault_location) {
            warnings.push(VaultWarning::VaultInsideGitWorkTree);
        }
        warnings.sort_unstable();
        warnings.dedup();
        Ok(Self {
            root: vault_root,
            manifest,
            warnings,
            _lease: Some(lease),
        })
    }

    /// Opens an existing Vault without creating files or acquiring mutation authority.
    pub fn open_read_only(
        project_root: impl AsRef<Path>,
        vault_root: impl AsRef<Path>,
        project_id: ProjectId,
    ) -> Result<Self, VaultError> {
        let project_root = project_root
            .as_ref()
            .canonicalize()
            .map_err(|_| VaultError::InvalidPath)?;
        let vault_root = vault_root
            .as_ref()
            .canonicalize()
            .map_err(|_| VaultError::InvalidPath)?;
        let bytes = std::fs::read(vault_root.join(MANIFEST_PATH)).map_err(|_| VaultError::Io)?;
        let manifest: VaultManifest =
            serde_json::from_slice(&bytes).map_err(|_| VaultError::UnsupportedSchema)?;
        let project_fingerprint = content_hash(normalized_path(&project_root).as_bytes());
        if manifest.project_id != project_id || manifest.project_fingerprint != project_fingerprint
        {
            return Err(VaultError::ProjectMismatch);
        }
        let mut warnings = vec![VaultWarning::PlaintextLocalFiles];
        if vault_root.starts_with(&project_root) {
            warnings.push(VaultWarning::VaultInsideProject);
        }
        if has_git_ancestor(&vault_root) {
            warnings.push(VaultWarning::VaultInsideGitWorkTree);
        }
        warnings.sort_unstable();
        warnings.dedup();
        Ok(Self {
            root: vault_root,
            manifest,
            warnings,
            _lease: None,
        })
    }

    #[must_use]
    pub fn recommended_sibling(project_root: impl AsRef<Path>) -> PathBuf {
        let project_root = project_root.as_ref();
        let name = project_root
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("project");
        project_root
            .parent()
            .unwrap_or(project_root)
            .join(format!("{name}.vault"))
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    #[must_use]
    pub const fn manifest(&self) -> &VaultManifest {
        &self.manifest
    }

    #[must_use]
    pub fn warnings(&self) -> &[VaultWarning] {
        &self.warnings
    }
}

#[must_use]
pub fn classify_vault_path(relative_path: &str) -> Option<VaultOwnership> {
    if relative_path == "AGENTS.md"
        || relative_path == "inbox"
        || relative_path.starts_with("inbox/")
    {
        Some(VaultOwnership::Human)
    } else if relative_path == ".minimax" || relative_path.starts_with(".minimax/") {
        Some(VaultOwnership::Internal)
    } else if relative_path == "raw"
        || relative_path.starts_with("raw/")
        || relative_path == "wiki"
        || relative_path.starts_with("wiki/")
        || relative_path == "log.md"
    {
        Some(VaultOwnership::Agent)
    } else {
        None
    }
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

fn normalized_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/").to_lowercase()
}

fn create_file_if_missing(path: &Path, bytes: &[u8]) -> Result<(), VaultError> {
    if path.exists() {
        Ok(())
    } else {
        match crate::path::create_new_file(path, bytes) {
            Ok(()) => Ok(()),
            Err(VaultError::Io) if path.exists() => Ok(()),
            Err(error) => Err(error),
        }
    }
}

fn has_git_ancestor(path: &Path) -> bool {
    path.ancestors()
        .any(|ancestor| ancestor.join(".git").exists())
}
