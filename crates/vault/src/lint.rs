use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use minimax_protocol::{
    ContentHash, EvidenceId, InboxImportReceipt, KnowledgePageStatus, SchemaVersion,
    TransactionManifest, TransactionState, VaultIssue, VaultIssueCode, VaultLintReport,
    VaultRepairReceipt,
};

use crate::bootstrap::{ProjectVault, VaultError};
use crate::page::parse_wiki_page;
use crate::path::{atomic_create_or_same, content_hash, sha256_hex};
use crate::raw::FinalizedSessionEvidence;
use crate::transaction::recover_wiki_transaction;

const REQUIRED_PATHS: [&str; 23] = [
    ".minimax/manifest.json",
    "AGENTS.md",
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
    "wiki/index.md",
    "log.md",
    ".minimax/pending",
    ".minimax/transactions",
    ".minimax/recovery",
    ".minimax/indexes",
    ".minimax/trash",
    ".minimax/receipts",
    ".minimax/finalized",
    ".minimax/imports",
    ".minimax/locks",
];

pub fn lint_vault(vault: &ProjectVault) -> VaultLintReport {
    let mut issues = Vec::new();
    lint_manifest(vault, &mut issues);
    for relative in REQUIRED_PATHS {
        if !vault.root().join(relative).exists() {
            issue(
                &mut issues,
                VaultIssueCode::OwnedPathMissing,
                relative,
                None,
            );
        }
    }
    let evidence = evidence_inventory(vault, &mut issues);
    lint_wiki(vault, &evidence, &mut issues);
    lint_workflows(vault, &mut issues);
    lint_transactions(vault, &mut issues);
    issues.sort_by(|left, right| {
        left.code
            .cmp(&right.code)
            .then_with(|| left.relative_path.cmp(&right.relative_path))
            .then_with(|| left.related_id.cmp(&right.related_id))
    });
    VaultLintReport {
        schema_version: SchemaVersion,
        project_id: vault.manifest().project_id.clone(),
        issues,
    }
}

pub fn repair_vault(
    vault: &ProjectVault,
    recorded_at_unix_ms: u64,
) -> Result<VaultRepairReceipt, VaultError> {
    let mut quarantined_fragments = repair_workflow_tails(vault)?;
    quarantined_fragments.sort();
    let mut recovered_transactions = Vec::new();
    for directory in sorted_directories(&vault.root().join(".minimax/transactions"))? {
        let manifest_path = directory.join("manifest.json");
        let Ok(bytes) = std::fs::read(&manifest_path) else {
            continue;
        };
        let Ok(manifest) = serde_json::from_slice::<TransactionManifest>(&bytes) else {
            continue;
        };
        let Ok(manifest) = manifest.validate() else {
            continue;
        };
        let receipt = transaction_receipt_path(vault.root(), &manifest);
        if manifest.state != TransactionState::Committed || !receipt.exists() {
            recover_wiki_transaction(vault, &manifest.transaction_id)?;
            recovered_transactions.push(manifest.transaction_id);
        }
    }
    recovered_transactions.sort();
    let report = lint_vault(vault);
    let identity = serde_json::to_vec(&(
        &recovered_transactions,
        &quarantined_fragments,
        recorded_at_unix_ms,
    ))
    .map_err(|_| VaultError::RecoveryRequired)?;
    Ok(VaultRepairReceipt {
        schema_version: SchemaVersion,
        operation_id: format!("repair:{}", &sha256_hex(&identity)[..24]),
        recovered_transactions,
        quarantined_fragments,
        remaining_issues: report.issues,
        recorded_at_unix_ms,
    })
}

pub(crate) fn evidence_inventory(
    vault: &ProjectVault,
    issues: &mut Vec<VaultIssue>,
) -> BTreeMap<EvidenceId, ContentHash> {
    let mut evidence = BTreeMap::new();
    for directory in sorted_directories(&vault.root().join("raw/sessions")).unwrap_or_default() {
        let relative = relative_path(vault.root(), &directory.join("session.json"));
        let metadata = match std::fs::read(directory.join("session.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<FinalizedSessionEvidence>(&bytes).ok())
        {
            Some(metadata) => metadata,
            None => {
                issue(issues, VaultIssueCode::RawMetadataInvalid, &relative, None);
                continue;
            }
        };
        let events_path = directory.join("events.jsonl");
        match std::fs::read(&events_path) {
            Ok(bytes) if content_hash(&bytes) == metadata.events_hash => {
                evidence.insert(metadata.evidence_id, metadata.events_hash);
            }
            Ok(_) => issue(
                issues,
                VaultIssueCode::RawHashMismatch,
                &relative_path(vault.root(), &events_path),
                Some(metadata.evidence_id.as_str()),
            ),
            Err(_) => issue(
                issues,
                VaultIssueCode::RawContentMissing,
                &relative_path(vault.root(), &events_path),
                Some(metadata.evidence_id.as_str()),
            ),
        }
    }
    for path in sorted_files(&vault.root().join(".minimax/imports")).unwrap_or_default() {
        let relative = relative_path(vault.root(), &path);
        let receipt = match std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<InboxImportReceipt>(&bytes).ok())
            .and_then(|receipt| receipt.validate().ok())
        {
            Some(receipt) => receipt,
            None => {
                issue(issues, VaultIssueCode::RawMetadataInvalid, &relative, None);
                continue;
            }
        };
        let imported = vault.root().join(&receipt.imported_relative_path);
        match std::fs::read(&imported) {
            Ok(bytes) if content_hash(&bytes) == receipt.content_hash => {
                evidence.insert(receipt.evidence_id, receipt.content_hash);
            }
            Ok(_) => issue(
                issues,
                VaultIssueCode::RawHashMismatch,
                &receipt.imported_relative_path,
                Some(receipt.evidence_id.as_str()),
            ),
            Err(_) => issue(
                issues,
                VaultIssueCode::RawContentMissing,
                &receipt.imported_relative_path,
                Some(receipt.evidence_id.as_str()),
            ),
        }
    }
    evidence
}

fn lint_manifest(vault: &ProjectVault, issues: &mut Vec<VaultIssue>) {
    let path = vault.root().join(".minimax/manifest.json");
    let valid = std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<minimax_protocol::VaultManifest>(&bytes).ok())
        .is_some_and(|manifest| manifest == *vault.manifest());
    if !valid {
        issue(
            issues,
            VaultIssueCode::ManifestInvalid,
            ".minimax/manifest.json",
            None,
        );
    }
}

fn lint_wiki(
    vault: &ProjectVault,
    evidence: &BTreeMap<EvidenceId, ContentHash>,
    issues: &mut Vec<VaultIssue>,
) {
    let mut pages = Vec::new();
    for path in sorted_files_recursive(&vault.root().join("wiki")).unwrap_or_default() {
        let relative = relative_path(vault.root(), &path);
        if relative == "wiki/index.md" || !relative.ends_with(".md") {
            continue;
        }
        match std::fs::read(&path)
            .map_err(|_| VaultError::Io)
            .and_then(|bytes| parse_wiki_page(&relative, &bytes))
        {
            Ok(page) => pages.push(page),
            Err(_) => issue(issues, VaultIssueCode::WikiPageInvalid, &relative, None),
        }
    }
    let mut page_ids = BTreeMap::new();
    let mut current_topics = BTreeMap::new();
    for page in &pages {
        if let Some(first) = page_ids.insert(page.page_id.clone(), page.relative_path.clone()) {
            issue(
                issues,
                VaultIssueCode::WikiPageIdDuplicate,
                &page.relative_path,
                Some(&first),
            );
        }
        if page.status == KnowledgePageStatus::Current
            && let Some(first) =
                current_topics.insert(page.topic_id.clone(), page.relative_path.clone())
        {
            issue(
                issues,
                VaultIssueCode::WikiCurrentTopicDuplicate,
                &page.relative_path,
                Some(&first),
            );
        }
        for source in &page.sources {
            match evidence.get(&source.source_id) {
                None => issue(
                    issues,
                    VaultIssueCode::WikiSourceMissing,
                    &page.relative_path,
                    Some(source.source_id.as_str()),
                ),
                Some(hash) if hash != &source.source_hash => issue(
                    issues,
                    VaultIssueCode::WikiSourceHashMismatch,
                    &page.relative_path,
                    Some(source.source_id.as_str()),
                ),
                Some(_) => {}
            }
        }
    }
}

fn lint_workflows(vault: &ProjectVault, issues: &mut Vec<VaultIssue>) {
    for directory in sorted_directories(&vault.root().join(".minimax/pending")).unwrap_or_default()
    {
        let journal = directory.join("journal.jsonl");
        let Ok(bytes) = std::fs::read(&journal) else {
            continue;
        };
        let relative = relative_path(vault.root(), &journal);
        if !bytes.is_empty() && !bytes.ends_with(b"\n") {
            issue(
                issues,
                VaultIssueCode::WorkflowJournalIncomplete,
                &relative,
                None,
            );
            continue;
        }
        for (expected, line) in bytes
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .enumerate()
        {
            let valid = serde_json::from_slice::<serde_json::Value>(line)
                .ok()
                .and_then(|value| value.get("sequence").and_then(serde_json::Value::as_u64))
                == u64::try_from(expected).ok();
            if !valid {
                issue(
                    issues,
                    VaultIssueCode::WorkflowRecordInvalid,
                    &relative,
                    Some(&expected.to_string()),
                );
            }
        }
    }
}

fn lint_transactions(vault: &ProjectVault, issues: &mut Vec<VaultIssue>) {
    for directory in
        sorted_directories(&vault.root().join(".minimax/transactions")).unwrap_or_default()
    {
        let path = directory.join("manifest.json");
        let relative = relative_path(vault.root(), &path);
        let manifest = match std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<TransactionManifest>(&bytes).ok())
            .and_then(|manifest| manifest.validate().ok())
        {
            Some(manifest) => manifest,
            None => {
                issue(
                    issues,
                    VaultIssueCode::TransactionManifestInvalid,
                    &relative,
                    None,
                );
                continue;
            }
        };
        let mut conflict = false;
        for target in &manifest.targets {
            let staged_hash = std::fs::read(vault.root().join(&target.staged_relative_path))
                .ok()
                .map(|bytes| content_hash(&bytes));
            let target_path = vault.root().join(&target.relative_path);
            let current_hash = match std::fs::read(&target_path) {
                Ok(bytes) => Some(content_hash(&bytes)),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(_) => {
                    conflict = true;
                    continue;
                }
            };
            if staged_hash.as_ref() != Some(&target.expected_hash)
                || (current_hash.as_ref() != target.old_hash.as_ref()
                    && current_hash.as_ref() != Some(&target.expected_hash))
            {
                conflict = true;
            }
        }
        if conflict {
            issue(
                issues,
                VaultIssueCode::TransactionTargetConflict,
                &relative,
                Some(manifest.transaction_id.as_str()),
            );
        } else if manifest.state != TransactionState::Committed
            || !transaction_receipt_path(vault.root(), &manifest).exists()
        {
            issue(
                issues,
                VaultIssueCode::TransactionRecoveryRequired,
                &relative,
                Some(manifest.transaction_id.as_str()),
            );
        }
    }
}

fn repair_workflow_tails(vault: &ProjectVault) -> Result<Vec<String>, VaultError> {
    let mut repaired = Vec::new();
    for directory in sorted_directories(&vault.root().join(".minimax/pending"))? {
        let journal = directory.join("journal.jsonl");
        let Ok(bytes) = std::fs::read(&journal) else {
            continue;
        };
        if bytes.is_empty() || bytes.ends_with(b"\n") {
            continue;
        }
        let complete_len = bytes
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map_or(0, |position| position + 1);
        let fragment = &bytes[complete_len..];
        let recovery_relative = format!(
            ".minimax/recovery/workflow-final-fragment-{}.partial",
            content_hash(fragment).as_str()
        );
        atomic_create_or_same(&vault.root().join(&recovery_relative), fragment)?;
        let mut file = OpenOptions::new()
            .write(true)
            .open(&journal)
            .map_err(|_| VaultError::Io)?;
        file.set_len(u64::try_from(complete_len).map_err(|_| VaultError::RecoveryRequired)?)
            .and_then(|()| file.flush())
            .and_then(|()| file.sync_all())
            .map_err(|_| VaultError::Io)?;
        repaired.push(recovery_relative);
    }
    Ok(repaired)
}

fn transaction_receipt_path(root: &Path, manifest: &TransactionManifest) -> PathBuf {
    root.join(".minimax/receipts").join(format!(
        "transaction-{}.json",
        sha256_hex(manifest.transaction_id.as_str().as_bytes())
    ))
}

pub(crate) fn sorted_files_recursive(root: &Path) -> Result<Vec<PathBuf>, VaultError> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        let mut entries = std::fs::read_dir(directory)
            .map_err(|_| VaultError::Io)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| VaultError::Io)?;
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries.into_iter().rev() {
            if entry.file_type().map_err(|_| VaultError::Io)?.is_dir() {
                pending.push(entry.path());
            } else {
                files.push(entry.path());
            }
        }
    }
    files.sort();
    Ok(files)
}

fn sorted_files(root: &Path) -> Result<Vec<PathBuf>, VaultError> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut entries = std::fs::read_dir(root)
        .map_err(|_| VaultError::Io)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| VaultError::Io)?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    Ok(entries
        .into_iter()
        .filter_map(|entry| entry.file_type().ok()?.is_file().then_some(entry.path()))
        .collect())
}

fn sorted_directories(root: &Path) -> Result<Vec<PathBuf>, VaultError> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut entries = std::fs::read_dir(root)
        .map_err(|_| VaultError::Io)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| VaultError::Io)?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    Ok(entries
        .into_iter()
        .filter_map(|entry| entry.file_type().ok()?.is_dir().then_some(entry.path()))
        .collect())
}

pub(crate) fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn issue(
    issues: &mut Vec<VaultIssue>,
    code: VaultIssueCode,
    relative_path: &str,
    related_id: Option<&str>,
) {
    issues.push(VaultIssue {
        code,
        relative_path: relative_path.to_owned(),
        related_id: related_id.map(str::to_owned),
    });
}

pub(crate) fn raw_snapshot(vault: &ProjectVault) -> Result<Vec<(String, ContentHash)>, VaultError> {
    let mut snapshot = Vec::new();
    for path in sorted_files_recursive(&vault.root().join("raw"))? {
        let bytes = std::fs::read(&path).map_err(|_| VaultError::Io)?;
        snapshot.push((relative_path(vault.root(), &path), content_hash(&bytes)));
    }
    Ok(snapshot)
}
