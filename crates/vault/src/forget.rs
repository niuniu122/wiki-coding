use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use minimax_protocol::{
    ContentHash, EvidenceId, ForgetId, ForgetPlan, ForgetReceipt, InboxImportReceipt,
    KnowledgeOperation, KnowledgePage, KnowledgePatch, SchemaVersion, TransactionId,
};

use crate::bootstrap::{ProjectVault, VaultError};
use crate::lint::{relative_path, sorted_files_recursive};
use crate::page::{parse_wiki_page, render_wiki_page};
use crate::path::{atomic_create_or_same, atomic_replace, content_hash, sha256_hex};
use crate::raw::FinalizedSessionEvidence;
use crate::transaction::{
    PreparedWikiTransaction, WikiChange, recover_wiki_transaction, wiki_transaction_exists,
};

struct EvidenceLocation {
    primary_path: PathBuf,
    cleanup_paths: Vec<PathBuf>,
}

pub fn plan_forget(
    vault: &ProjectVault,
    evidence_id: EvidenceId,
    expected_hash: ContentHash,
    created_at_unix_ms: u64,
) -> Result<ForgetPlan, VaultError> {
    locate_evidence(vault, &evidence_id, &expected_hash)?;
    let mut affected_page_paths = Vec::new();
    for path in sorted_files_recursive(&vault.root().join("wiki"))? {
        let relative = relative_path(vault.root(), &path);
        if relative == "wiki/index.md" || !relative.ends_with(".md") {
            continue;
        }
        let page = parse_wiki_page(&relative, &std::fs::read(path).map_err(|_| VaultError::Io)?)?;
        if page
            .sources
            .iter()
            .any(|source| source.source_id == evidence_id)
        {
            affected_page_paths.push(relative);
        }
    }
    affected_page_paths.sort();
    let canonical = serde_json::to_vec(&(
        &evidence_id,
        &expected_hash,
        &affected_page_paths,
        created_at_unix_ms,
    ))
    .map_err(|_| VaultError::RecoveryRequired)?;
    let plan_hash = content_hash(&canonical);
    let forget_id = ForgetId::new(format!("forget:{}", &plan_hash.as_str()[..24]))
        .map_err(|_| VaultError::RecoveryRequired)?;
    Ok(ForgetPlan {
        schema_version: SchemaVersion,
        forget_id,
        evidence_id,
        expected_hash,
        affected_page_paths,
        plan_hash,
        created_at_unix_ms,
    })
}

#[must_use]
pub fn forget_confirmation(plan: &ForgetPlan) -> String {
    format!(
        "FORGET {} {}",
        plan.forget_id.as_str(),
        &plan.plan_hash.as_str()[..16]
    )
}

pub fn apply_forget_plan(
    vault: &ProjectVault,
    plan: &ForgetPlan,
    patch: &KnowledgePatch,
    confirmation: &str,
    recorded_at_unix_ms: u64,
) -> Result<ForgetReceipt, VaultError> {
    if confirmation != forget_confirmation(plan) {
        return Err(VaultError::InvalidConfirmation);
    }
    let tombstone_path = forget_tombstone_path(vault.root(), &plan.forget_id);
    if tombstone_path.exists() {
        let receipt = read_receipt(&tombstone_path)?;
        if receipt.forget_id == plan.forget_id
            && receipt.evidence_hash == plan.expected_hash
            && receipt.code == "forgotten"
        {
            return Ok(receipt);
        }
    }
    if plan_forget(
        vault,
        plan.evidence_id.clone(),
        plan.expected_hash.clone(),
        plan.created_at_unix_ms,
    )? != *plan
    {
        return Err(VaultError::Conflict);
    }
    let location = locate_evidence(vault, &plan.evidence_id, &plan.expected_hash)?;
    let changes = validate_recrystallization(vault, plan, patch)?;
    let transaction_id = TransactionId::new(format!(
        "forget:{}",
        &sha256_hex(plan.plan_hash.as_str().as_bytes())[..24]
    ))
    .map_err(|_| VaultError::RecoveryRequired)?;
    if wiki_transaction_exists(vault, &transaction_id) {
        recover_wiki_transaction(vault, &transaction_id)?;
    } else {
        PreparedWikiTransaction::prepare(
            vault,
            transaction_id.clone(),
            changes,
            recorded_at_unix_ms,
        )?
        .roll_forward()?;
    }

    let tombstone_relative_path = relative_path(vault.root(), &tombstone_path);
    let evidence_key = content_hash(plan.evidence_id.as_str().as_bytes());
    let mut receipt = ForgetReceipt {
        schema_version: SchemaVersion,
        forget_id: plan.forget_id.clone(),
        evidence_key,
        evidence_hash: plan.expected_hash.clone(),
        transaction_id,
        tombstone_relative_path,
        code: "prepared".to_owned(),
        recorded_at_unix_ms,
    };
    atomic_create_or_same(&tombstone_path, &encode(&receipt)?)?;
    if content_hash(&std::fs::read(&location.primary_path).map_err(|_| VaultError::Io)?)
        != plan.expected_hash
    {
        return Err(VaultError::Conflict);
    }
    for path in &location.cleanup_paths {
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(VaultError::Io),
        }
    }
    let mut parents = location
        .cleanup_paths
        .iter()
        .filter_map(|path| path.parent().map(Path::to_path_buf))
        .collect::<BTreeSet<_>>();
    while let Some(parent) = parents.pop_first() {
        if parent.starts_with(vault.root().join("raw")) {
            let _ = std::fs::remove_dir(&parent);
        }
    }
    receipt.code = "forgotten".to_owned();
    atomic_replace(&tombstone_path, &encode(&receipt)?)?;
    Ok(receipt)
}

fn validate_recrystallization(
    vault: &ProjectVault,
    plan: &ForgetPlan,
    patch: &KnowledgePatch,
) -> Result<Vec<WikiChange>, VaultError> {
    let patch = patch
        .clone()
        .validate()
        .map_err(|_| VaultError::InvalidPage)?;
    let affected = plan
        .affected_page_paths
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut replacements = BTreeMap::<String, KnowledgePage>::new();
    for operation in patch.operations {
        match operation {
            KnowledgeOperation::Replace {
                page,
                expected_hash,
            } if affected.contains(&page.relative_path) => {
                let current = std::fs::read(vault.root().join(&page.relative_path))
                    .map_err(|_| VaultError::Conflict)?;
                if content_hash(&current) != expected_hash
                    || page
                        .sources
                        .iter()
                        .any(|source| source.source_id == plan.evidence_id)
                {
                    return Err(VaultError::Conflict);
                }
                replacements.insert(page.relative_path.clone(), page);
            }
            _ => return Err(VaultError::InvalidPage),
        }
    }
    if replacements.keys().cloned().collect::<BTreeSet<_>>() != affected {
        return Err(VaultError::Conflict);
    }
    let mut changes = Vec::new();
    for (relative, page) in replacements {
        let current = std::fs::read(vault.root().join(&relative)).map_err(|_| VaultError::Io)?;
        changes.push(WikiChange {
            relative_path: relative,
            expected_old_hash: Some(content_hash(&current)),
            bytes: render_wiki_page(&page)?,
        });
    }
    let log_path = vault.root().join("log.md");
    let mut log = std::fs::read(&log_path).map_err(|_| VaultError::Io)?;
    log.extend_from_slice(
        format!("- privacy forget {} committed\n", plan.forget_id.as_str()).as_bytes(),
    );
    changes.push(WikiChange {
        relative_path: "log.md".to_owned(),
        expected_old_hash: Some(content_hash(
            &std::fs::read(log_path).map_err(|_| VaultError::Io)?,
        )),
        bytes: log,
    });
    Ok(changes)
}

fn locate_evidence(
    vault: &ProjectVault,
    evidence_id: &EvidenceId,
    expected_hash: &ContentHash,
) -> Result<EvidenceLocation, VaultError> {
    for metadata_path in sorted_files_recursive(&vault.root().join("raw/sessions"))? {
        if metadata_path.file_name().and_then(|value| value.to_str()) != Some("session.json") {
            continue;
        }
        let evidence = serde_json::from_slice::<FinalizedSessionEvidence>(
            &std::fs::read(&metadata_path).map_err(|_| VaultError::Io)?,
        )
        .map_err(|_| VaultError::RecoveryRequired)?;
        if &evidence.evidence_id == evidence_id {
            if &evidence.events_hash != expected_hash {
                return Err(VaultError::Conflict);
            }
            let events = metadata_path
                .parent()
                .ok_or(VaultError::RecoveryRequired)?
                .join("events.jsonl");
            if content_hash(&std::fs::read(&events).map_err(|_| VaultError::Io)?) != *expected_hash
            {
                return Err(VaultError::Conflict);
            }
            return Ok(EvidenceLocation {
                primary_path: events.clone(),
                cleanup_paths: vec![events, metadata_path],
            });
        }
    }
    for receipt_path in sorted_files_recursive(&vault.root().join(".minimax/imports"))? {
        let receipt = serde_json::from_slice::<InboxImportReceipt>(
            &std::fs::read(&receipt_path).map_err(|_| VaultError::Io)?,
        )
        .map_err(|_| VaultError::RecoveryRequired)?
        .validate()
        .map_err(|_| VaultError::RecoveryRequired)?;
        if &receipt.evidence_id == evidence_id {
            if &receipt.content_hash != expected_hash {
                return Err(VaultError::Conflict);
            }
            let imported = vault.root().join(&receipt.imported_relative_path);
            if content_hash(&std::fs::read(&imported).map_err(|_| VaultError::Io)?)
                != *expected_hash
            {
                return Err(VaultError::Conflict);
            }
            return Ok(EvidenceLocation {
                primary_path: imported.clone(),
                cleanup_paths: vec![imported, receipt_path],
            });
        }
    }
    Err(VaultError::SessionNotFound)
}

fn forget_tombstone_path(root: &Path, forget_id: &ForgetId) -> PathBuf {
    root.join(".minimax/receipts").join(format!(
        "forget-{}.json",
        sha256_hex(forget_id.as_str().as_bytes())
    ))
}

fn read_receipt(path: &Path) -> Result<ForgetReceipt, VaultError> {
    serde_json::from_slice(&std::fs::read(path).map_err(|_| VaultError::RecoveryRequired)?)
        .map_err(|_| VaultError::RecoveryRequired)
}

fn encode<T: serde::Serialize>(value: &T) -> Result<Vec<u8>, VaultError> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|_| VaultError::RecoveryRequired)?;
    bytes.push(b'\n');
    Ok(bytes)
}
