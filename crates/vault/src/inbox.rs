use std::path::Path;

use minimax_protocol::{
    EvidenceId, InboxImportReceipt, InboxImportStatus, RawEvidenceKind, SchemaVersion,
    TransactionId, validate_vault_relative_path,
};

use crate::bootstrap::{ProjectVault, VaultError};
use crate::path::{atomic_create_or_same, atomic_replace, content_hash};
use crate::transaction::transaction_is_committed;

const MAX_INBOX_BYTES: usize = 16 * 1024 * 1024;
const MAX_TEXT_BYTES: usize = 4 * 1024 * 1024;

pub fn import_inbox_file(
    vault: &ProjectVault,
    origin_relative_path: &str,
    imported_at_unix_ms: u64,
) -> Result<InboxImportReceipt, VaultError> {
    validate_vault_relative_path(origin_relative_path).map_err(|_| VaultError::InvalidPath)?;
    if !origin_relative_path.starts_with("inbox/") {
        return Err(VaultError::InvalidPath);
    }
    let origin = vault.root().join(origin_relative_path);
    let bytes = std::fs::read(&origin).map_err(|_| VaultError::Io)?;
    if bytes.len() > MAX_INBOX_BYTES {
        return Err(VaultError::RecoveryRequired);
    }
    let digest = content_hash(&bytes);
    let is_text =
        bytes.len() <= MAX_TEXT_BYTES && !bytes.contains(&0) && std::str::from_utf8(&bytes).is_ok();
    let (kind, imported_relative_path, status, code) = if is_text {
        (
            RawEvidenceKind::Import,
            format!("raw/imports/{}.txt", digest.as_str()),
            InboxImportStatus::ImportedSourceRetained,
            "imported_source_retained",
        )
    } else {
        (
            RawEvidenceKind::Asset,
            format!("raw/assets/{}.bin", digest.as_str()),
            InboxImportStatus::EvidenceOnly,
            "unsupported_binary_evidence_only",
        )
    };
    let imported = vault.root().join(&imported_relative_path);
    atomic_create_or_same(&imported, &bytes)?;
    if content_hash(&std::fs::read(&imported).map_err(|_| VaultError::Io)?) != digest {
        return Err(VaultError::RecoveryRequired);
    }
    let current_origin = std::fs::read(&origin).map_err(|_| VaultError::Io)?;
    if content_hash(&current_origin) != digest {
        return Err(VaultError::Conflict);
    }
    let receipt = InboxImportReceipt {
        schema_version: SchemaVersion,
        evidence_id: EvidenceId::new(format!("import:{}", digest.as_str()))
            .map_err(|_| VaultError::RecoveryRequired)?,
        kind,
        content_hash: digest,
        bytes: u64::try_from(bytes.len()).map_err(|_| VaultError::RecoveryRequired)?,
        origin_relative_path: origin_relative_path.to_owned(),
        imported_relative_path,
        imported_at_unix_ms,
        status,
        code: code.to_owned(),
        transaction_id: None,
    }
    .validate()
    .map_err(|_| VaultError::RecoveryRequired)?;
    let receipt_path = import_receipt_path(vault.root(), receipt.content_hash.as_str());
    let receipt_bytes = encode_receipt(&receipt)?;
    atomic_create_or_same(&receipt_path, &receipt_bytes)?;
    Ok(receipt)
}

pub fn complete_inbox_import(
    vault: &ProjectVault,
    content_hash_value: &minimax_protocol::ContentHash,
    transaction_id: &TransactionId,
) -> Result<InboxImportReceipt, VaultError> {
    if !transaction_is_committed(vault, transaction_id)? {
        return Err(VaultError::RecoveryRequired);
    }
    let receipt_path = import_receipt_path(vault.root(), content_hash_value.as_str());
    let mut receipt = decode_receipt(&receipt_path)?;
    if receipt.content_hash != *content_hash_value {
        return Err(VaultError::RecoveryRequired);
    }
    if receipt.kind == RawEvidenceKind::Asset {
        return Ok(receipt);
    }
    if receipt.status == InboxImportStatus::CompiledSourceRemoved {
        return if receipt.transaction_id.as_ref() == Some(transaction_id) {
            Ok(receipt)
        } else {
            Err(VaultError::Conflict)
        };
    }
    let origin = vault.root().join(&receipt.origin_relative_path);
    match std::fs::read(&origin) {
        Ok(bytes) if content_hash(&bytes) == receipt.content_hash => {
            if std::fs::remove_file(&origin).is_err() {
                return Ok(receipt);
            }
        }
        Ok(_) => return Ok(receipt),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Ok(receipt),
    }
    receipt.status = InboxImportStatus::CompiledSourceRemoved;
    receipt.code = "compiled_source_removed".to_owned();
    receipt.transaction_id = Some(transaction_id.clone());
    receipt
        .clone()
        .validate()
        .map_err(|_| VaultError::RecoveryRequired)?;
    atomic_replace(&receipt_path, &encode_receipt(&receipt)?)?;
    Ok(receipt)
}

fn import_receipt_path(root: &Path, content_hash: &str) -> std::path::PathBuf {
    root.join(".minimax/imports")
        .join(format!("{content_hash}.json"))
}

fn encode_receipt(receipt: &InboxImportReceipt) -> Result<Vec<u8>, VaultError> {
    let mut bytes = serde_json::to_vec_pretty(receipt).map_err(|_| VaultError::RecoveryRequired)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn decode_receipt(path: &Path) -> Result<InboxImportReceipt, VaultError> {
    serde_json::from_slice::<InboxImportReceipt>(
        &std::fs::read(path).map_err(|_| VaultError::RecoveryRequired)?,
    )
    .map_err(|_| VaultError::RecoveryRequired)?
    .validate()
    .map_err(|_| VaultError::RecoveryRequired)
}
