use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use minimax_protocol::{
    ContentHash, SchemaVersion, TransactionId, TransactionManifest, TransactionState,
    TransactionTarget, VaultReceipt, validate_vault_relative_path,
};

use crate::bootstrap::{ProjectVault, VaultError};
use crate::path::{atomic_create_or_same, atomic_replace, content_hash, sha256_hex};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WikiChange {
    pub relative_path: String,
    pub expected_old_hash: Option<ContentHash>,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransactionFaultPoint {
    AfterPrepared,
    AfterApplyingState,
    AfterTarget(usize),
    BeforeReceipt,
}

pub struct PreparedWikiTransaction<'a> {
    vault: &'a ProjectVault,
    manifest: TransactionManifest,
    transaction_dir: PathBuf,
}

impl<'a> PreparedWikiTransaction<'a> {
    pub fn prepare(
        vault: &'a ProjectVault,
        transaction_id: TransactionId,
        mut changes: Vec<WikiChange>,
        created_at_unix_ms: u64,
    ) -> Result<Self, VaultError> {
        if changes.is_empty() {
            return Err(VaultError::EmptyTransaction);
        }
        changes.sort_by(|left, right| {
            target_rank(&left.relative_path)
                .cmp(&target_rank(&right.relative_path))
                .then_with(|| left.relative_path.cmp(&right.relative_path))
        });
        let mut seen = BTreeSet::new();
        let mut effective = Vec::new();
        for change in changes {
            validate_target_path(&change.relative_path)?;
            if !seen.insert(change.relative_path.clone()) {
                return Err(VaultError::Conflict);
            }
            let target = vault.root().join(&change.relative_path);
            let current = read_hash(&target)?;
            if current != change.expected_old_hash {
                return Err(VaultError::Conflict);
            }
            let expected_hash = content_hash(&change.bytes);
            if current.as_ref() != Some(&expected_hash) {
                effective.push((change, expected_hash));
            }
        }
        if effective.is_empty() {
            return Err(VaultError::EmptyTransaction);
        }
        let transaction_dir = transaction_directory(vault.root(), &transaction_id);
        std::fs::create_dir_all(transaction_dir.join("staged")).map_err(|_| VaultError::Io)?;
        let mut targets = Vec::new();
        for (index, (change, expected_hash)) in effective.into_iter().enumerate() {
            let staged_relative_path = format!(
                ".minimax/transactions/{}/staged/{index:04}.data",
                transaction_directory_name(&transaction_id)
            );
            atomic_create_or_same(&vault.root().join(&staged_relative_path), &change.bytes)?;
            targets.push(TransactionTarget {
                relative_path: change.relative_path,
                old_hash: change.expected_old_hash,
                expected_hash,
                staged_relative_path,
                order: u32::try_from(index).map_err(|_| VaultError::RecoveryRequired)?,
            });
        }
        let manifest = TransactionManifest {
            schema_version: SchemaVersion,
            transaction_id,
            state: TransactionState::Prepared,
            targets,
            created_at_unix_ms,
        }
        .validate()
        .map_err(|_| VaultError::RecoveryRequired)?;
        write_manifest(&transaction_dir, &manifest, false)?;
        Ok(Self {
            vault,
            manifest,
            transaction_dir,
        })
    }

    #[must_use]
    pub const fn manifest(&self) -> &TransactionManifest {
        &self.manifest
    }

    pub fn roll_forward(mut self) -> Result<VaultReceipt, VaultError> {
        self.roll_forward_with_fault(None)
    }

    pub fn roll_forward_with_fault(
        &mut self,
        fault: Option<TransactionFaultPoint>,
    ) -> Result<VaultReceipt, VaultError> {
        if fault == Some(TransactionFaultPoint::AfterPrepared) {
            return Err(VaultError::FaultInjected);
        }
        validate_prepared(self.vault.root(), &self.transaction_dir, &self.manifest)?;
        if self.manifest.state != TransactionState::Committed {
            self.manifest.state = TransactionState::Applying;
            write_manifest(&self.transaction_dir, &self.manifest, true)?;
        }
        if fault == Some(TransactionFaultPoint::AfterApplyingState) {
            return Err(VaultError::FaultInjected);
        }
        for (index, target) in self.manifest.targets.iter().enumerate() {
            let target_path = self.vault.root().join(&target.relative_path);
            if read_hash(&target_path)?.as_ref() != Some(&target.expected_hash) {
                let staged = std::fs::read(self.vault.root().join(&target.staged_relative_path))
                    .map_err(|_| VaultError::RecoveryRequired)?;
                atomic_replace(&target_path, &staged)?;
            }
            if fault == Some(TransactionFaultPoint::AfterTarget(index)) {
                return Err(VaultError::FaultInjected);
            }
        }
        self.manifest.state = TransactionState::Committed;
        write_manifest(&self.transaction_dir, &self.manifest, true)?;
        if fault == Some(TransactionFaultPoint::BeforeReceipt) {
            return Err(VaultError::FaultInjected);
        }
        write_receipt(self.vault.root(), &self.manifest)
    }
}

pub fn recover_wiki_transaction(
    vault: &ProjectVault,
    transaction_id: &TransactionId,
) -> Result<VaultReceipt, VaultError> {
    let transaction_dir = transaction_directory(vault.root(), transaction_id);
    let manifest = load_manifest(&transaction_dir)?;
    if &manifest.transaction_id != transaction_id
        || matches!(manifest.state, TransactionState::Preparing)
    {
        return Err(VaultError::RecoveryRequired);
    }
    PreparedWikiTransaction {
        vault,
        manifest,
        transaction_dir,
    }
    .roll_forward()
}

pub(crate) fn transaction_is_committed(
    vault: &ProjectVault,
    transaction_id: &TransactionId,
) -> Result<bool, VaultError> {
    let transaction_dir = transaction_directory(vault.root(), transaction_id);
    let manifest = load_manifest(&transaction_dir)?;
    if manifest.transaction_id != *transaction_id || manifest.state != TransactionState::Committed {
        return Ok(false);
    }
    let receipt_path = vault.root().join(".minimax/receipts").join(format!(
        "transaction-{}.json",
        transaction_directory_name(transaction_id)
    ));
    let receipt: VaultReceipt = serde_json::from_slice(
        &std::fs::read(receipt_path).map_err(|_| VaultError::RecoveryRequired)?,
    )
    .map_err(|_| VaultError::RecoveryRequired)?;
    Ok(receipt.operation_id == transaction_id.as_str() && receipt.code == "committed")
}

fn validate_prepared(
    vault_root: &Path,
    transaction_dir: &Path,
    manifest: &TransactionManifest,
) -> Result<(), VaultError> {
    manifest
        .clone()
        .validate()
        .map_err(|_| VaultError::RecoveryRequired)?;
    let expected_prefix = format!(
        ".minimax/transactions/{}/staged/",
        transaction_directory_name(&manifest.transaction_id)
    );
    for target in &manifest.targets {
        if !target.staged_relative_path.starts_with(&expected_prefix) {
            return Err(VaultError::RecoveryRequired);
        }
        let staged = std::fs::read(vault_root.join(&target.staged_relative_path))
            .map_err(|_| VaultError::RecoveryRequired)?;
        if content_hash(&staged) != target.expected_hash {
            return Err(VaultError::RecoveryRequired);
        }
        let current = read_hash(&vault_root.join(&target.relative_path))?;
        if current.as_ref() != target.old_hash.as_ref()
            && current.as_ref() != Some(&target.expected_hash)
        {
            return Err(VaultError::Conflict);
        }
    }
    let on_disk = load_manifest(transaction_dir)?;
    if &on_disk != manifest {
        return Err(VaultError::RecoveryRequired);
    }
    Ok(())
}

fn load_manifest(transaction_dir: &Path) -> Result<TransactionManifest, VaultError> {
    let bytes = std::fs::read(transaction_dir.join("manifest.json"))
        .map_err(|_| VaultError::RecoveryRequired)?;
    serde_json::from_slice::<TransactionManifest>(&bytes)
        .map_err(|_| VaultError::RecoveryRequired)?
        .validate()
        .map_err(|_| VaultError::RecoveryRequired)
}

fn write_manifest(
    transaction_dir: &Path,
    manifest: &TransactionManifest,
    replace: bool,
) -> Result<(), VaultError> {
    let mut bytes =
        serde_json::to_vec_pretty(manifest).map_err(|_| VaultError::RecoveryRequired)?;
    bytes.push(b'\n');
    let path = transaction_dir.join("manifest.json");
    if replace {
        atomic_replace(&path, &bytes)
    } else {
        atomic_create_or_same(&path, &bytes)
    }
}

fn write_receipt(
    vault_root: &Path,
    manifest: &TransactionManifest,
) -> Result<VaultReceipt, VaultError> {
    let receipt = VaultReceipt {
        schema_version: SchemaVersion,
        operation_id: manifest.transaction_id.as_str().to_owned(),
        code: "committed".to_owned(),
        recorded_at_unix_ms: manifest.created_at_unix_ms,
    };
    let mut bytes = serde_json::to_vec_pretty(&receipt).map_err(|_| VaultError::Io)?;
    bytes.push(b'\n');
    let receipt_path = vault_root.join(".minimax/receipts").join(format!(
        "transaction-{}.json",
        transaction_directory_name(&manifest.transaction_id)
    ));
    atomic_create_or_same(&receipt_path, &bytes)?;
    Ok(receipt)
}

fn read_hash(path: &Path) -> Result<Option<ContentHash>, VaultError> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(content_hash(&bytes))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(VaultError::Io),
    }
}

fn transaction_directory(root: &Path, id: &TransactionId) -> PathBuf {
    root.join(".minimax/transactions")
        .join(transaction_directory_name(id))
}

fn transaction_directory_name(id: &TransactionId) -> String {
    sha256_hex(id.as_str().as_bytes())
}

fn validate_target_path(path: &str) -> Result<(), VaultError> {
    validate_vault_relative_path(path).map_err(|_| VaultError::InvalidPath)?;
    if path == "wiki/index.md"
        || path == "log.md"
        || (path.starts_with("wiki/") && path.ends_with(".md"))
        || path.starts_with(".minimax/indexes/")
    {
        Ok(())
    } else {
        Err(VaultError::InvalidPath)
    }
}

fn target_rank(path: &str) -> u8 {
    if path == "wiki/index.md" {
        1
    } else if path == "log.md" {
        2
    } else if path.starts_with(".minimax/") {
        3
    } else {
        0
    }
}
