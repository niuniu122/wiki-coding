use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use minimax_protocol::{
    ContentHash, EvidenceId, GcCandidate, GcClass, GcId, GcPlan, GcReceipt, GcReceiptAction,
    InboxImportReceipt, KnowledgeEvaluationJob, SchemaVersion, TrashEntry, TrashManifest,
    TrashState,
};

use crate::bootstrap::{ProjectVault, VaultError};
use crate::lint::{relative_path, sorted_files_recursive};
use crate::page::parse_wiki_page;
use crate::path::{atomic_create_or_same, atomic_replace, content_hash, sha256_hex};
use crate::raw::FinalizedSessionEvidence;

const THIRTY_DAYS_MS: u64 = 30 * 24 * 60 * 60 * 1_000;
const SEVEN_DAYS_MS: u64 = 7 * 24 * 60 * 60 * 1_000;

pub fn gc_report(vault: &ProjectVault, evaluated_at_unix_ms: u64) -> Result<GcPlan, VaultError> {
    let references = referenced_evidence(vault)?;
    let pins = pinned_paths(vault)?;
    let mut candidates = Vec::new();
    let evidence_paths = raw_evidence_paths(vault)?;
    for path in sorted_files_recursive(&vault.root().join("raw"))? {
        let relative = relative_path(vault.root(), &path);
        let bytes = std::fs::read(&path).map_err(|_| VaultError::Io)?;
        let evidence_id = evidence_paths.get(&relative);
        let class =
            if pins.contains(&relative) || evidence_id.is_some_and(|id| references.contains(id)) {
                GcClass::Referenced
            } else {
                GcClass::Permanent
            };
        candidates.push(candidate(relative, &bytes, class, None)?);
    }
    for path in sorted_files_recursive(&vault.root().join(".minimax/indexes"))? {
        let relative = relative_path(vault.root(), &path);
        let bytes = std::fs::read(&path).map_err(|_| VaultError::Io)?;
        let class = if pins.contains(&relative) {
            GcClass::Referenced
        } else {
            GcClass::Rebuildable
        };
        candidates.push(candidate(relative, &bytes, class, None)?);
    }
    for namespace in [".minimax/transient", ".minimax/completed-staging"] {
        for path in sorted_files_recursive(&vault.root().join(namespace))? {
            let relative = relative_path(vault.root(), &path);
            let bytes = std::fs::read(&path).map_err(|_| VaultError::Io)?;
            let modified = std::fs::metadata(&path)
                .map_err(|_| VaultError::Io)?
                .modified()
                .map_err(|_| VaultError::Io)?
                .duration_since(UNIX_EPOCH)
                .map_err(|_| VaultError::RecoveryRequired)?
                .as_millis();
            let modified = u64::try_from(modified).map_err(|_| VaultError::RecoveryRequired)?;
            let eligible_at = modified
                .checked_add(THIRTY_DAYS_MS)
                .ok_or(VaultError::RecoveryRequired)?;
            let class = if pins.contains(&relative) {
                GcClass::Referenced
            } else {
                GcClass::Collectable
            };
            candidates.push(candidate(relative, &bytes, class, Some(eligible_at))?);
        }
    }
    candidates.sort_by(|left, right| {
        left.class
            .cmp(&right.class)
            .then_with(|| left.eligible_at_unix_ms.cmp(&right.eligible_at_unix_ms))
            .then_with(|| left.bytes.cmp(&right.bytes))
            .then_with(|| left.relative_path.cmp(&right.relative_path))
    });
    let canonical = serde_json::to_vec(&(SchemaVersion, &candidates, evaluated_at_unix_ms))
        .map_err(|_| VaultError::RecoveryRequired)?;
    let plan_hash = content_hash(&canonical);
    let gc_id = GcId::new(format!("gc:{}", &plan_hash.as_str()[..24]))
        .map_err(|_| VaultError::RecoveryRequired)?;
    Ok(GcPlan {
        schema_version: SchemaVersion,
        gc_id,
        plan_hash,
        candidates,
        created_at_unix_ms: evaluated_at_unix_ms,
    })
}

#[must_use]
pub fn gc_apply_confirmation(plan: &GcPlan) -> String {
    format!(
        "APPLY {} {}",
        plan.gc_id.as_str(),
        &plan.plan_hash.as_str()[..16]
    )
}

#[must_use]
pub fn gc_purge_confirmation(manifest: &TrashManifest) -> String {
    format!(
        "PURGE {} {}",
        manifest.gc_id.as_str(),
        &manifest.plan_hash.as_str()[..16]
    )
}

pub fn apply_gc_plan(
    vault: &ProjectVault,
    plan: &GcPlan,
    confirmation: &str,
    applied_at_unix_ms: u64,
) -> Result<GcReceipt, VaultError> {
    if confirmation != gc_apply_confirmation(plan) {
        return Err(VaultError::InvalidConfirmation);
    }
    let manifest_path = trash_manifest_path(vault.root(), &plan.gc_id);
    if manifest_path.exists() {
        let manifest = read_trash_manifest(&manifest_path)?;
        if manifest.plan_hash != plan.plan_hash || manifest.gc_id != plan.gc_id {
            return Err(VaultError::Conflict);
        }
        return finish_apply(vault, manifest, &manifest_path);
    }
    if gc_report(vault, plan.created_at_unix_ms)? != *plan {
        return Err(VaultError::Conflict);
    }
    let mut entries = Vec::new();
    for candidate in plan.candidates.iter().filter(|candidate| {
        candidate.class == GcClass::Rebuildable
            || (candidate.class == GcClass::Collectable
                && candidate
                    .eligible_at_unix_ms
                    .is_some_and(|eligible| eligible <= applied_at_unix_ms))
    }) {
        entries.push(TrashEntry {
            original_relative_path: candidate.relative_path.clone(),
            trash_relative_path: format!(
                ".minimax/trash/{}/files/{}",
                safe_gc_directory(&plan.gc_id),
                candidate.relative_path
            ),
            content_hash: candidate.content_hash.clone(),
            bytes: candidate.bytes,
        });
    }
    let manifest = TrashManifest {
        schema_version: SchemaVersion,
        gc_id: plan.gc_id.clone(),
        plan_hash: plan.plan_hash.clone(),
        state: TrashState::Prepared,
        entries,
        applied_at_unix_ms,
        expires_at_unix_ms: applied_at_unix_ms
            .checked_add(SEVEN_DAYS_MS)
            .ok_or(VaultError::RecoveryRequired)?,
    };
    atomic_create_or_same(&manifest_path, &encode(&manifest)?)?;
    finish_apply(vault, manifest, &manifest_path)
}

pub fn undo_gc_plan(
    vault: &ProjectVault,
    gc_id: &GcId,
    now_unix_ms: u64,
) -> Result<GcReceipt, VaultError> {
    let manifest_path = trash_manifest_path(vault.root(), gc_id);
    let mut manifest = read_trash_manifest(&manifest_path)?;
    if manifest.state == TrashState::Undone {
        return gc_receipt(&manifest, GcReceiptAction::Undone, now_unix_ms);
    }
    if manifest.state != TrashState::Applied {
        return Err(VaultError::RecoveryRequired);
    }
    if now_unix_ms > manifest.expires_at_unix_ms {
        return Err(VaultError::Expired);
    }
    for entry in &manifest.entries {
        let original = vault.root().join(&entry.original_relative_path);
        let trashed = vault.root().join(&entry.trash_relative_path);
        if original.exists() || read_hash(&trashed)? != entry.content_hash {
            return Err(VaultError::Conflict);
        }
    }
    for entry in &manifest.entries {
        let original = vault.root().join(&entry.original_relative_path);
        let trashed = vault.root().join(&entry.trash_relative_path);
        if let Some(parent) = original.parent() {
            std::fs::create_dir_all(parent).map_err(|_| VaultError::Io)?;
        }
        std::fs::rename(trashed, original).map_err(|_| VaultError::Io)?;
    }
    manifest.state = TrashState::Undone;
    atomic_replace(&manifest_path, &encode(&manifest)?)?;
    let receipt = gc_receipt(&manifest, GcReceiptAction::Undone, now_unix_ms)?;
    write_gc_receipt(vault, &receipt)?;
    Ok(receipt)
}

pub fn purge_gc_plan(
    vault: &ProjectVault,
    gc_id: &GcId,
    confirmation: &str,
    now_unix_ms: u64,
) -> Result<GcReceipt, VaultError> {
    let manifest_path = trash_manifest_path(vault.root(), gc_id);
    let manifest = read_trash_manifest(&manifest_path)?;
    if confirmation != gc_purge_confirmation(&manifest) {
        return Err(VaultError::InvalidConfirmation);
    }
    if manifest.state != TrashState::Applied {
        return Err(VaultError::RecoveryRequired);
    }
    if now_unix_ms <= manifest.expires_at_unix_ms {
        return Err(VaultError::NotExpired);
    }
    for entry in &manifest.entries {
        if read_hash(&vault.root().join(&entry.trash_relative_path))? != entry.content_hash {
            return Err(VaultError::Conflict);
        }
    }
    let receipt = gc_receipt(&manifest, GcReceiptAction::Purged, now_unix_ms)?;
    write_gc_receipt(vault, &receipt)?;
    let trash_root = vault
        .root()
        .join(".minimax/trash")
        .join(safe_gc_directory(gc_id));
    std::fs::remove_dir_all(trash_root).map_err(|_| VaultError::Io)?;
    Ok(receipt)
}

pub fn read_gc_trash_manifest(
    vault: &ProjectVault,
    gc_id: &GcId,
) -> Result<TrashManifest, VaultError> {
    read_trash_manifest(&trash_manifest_path(vault.root(), gc_id))
}

fn finish_apply(
    vault: &ProjectVault,
    mut manifest: TrashManifest,
    manifest_path: &Path,
) -> Result<GcReceipt, VaultError> {
    if manifest.state == TrashState::Applied {
        return gc_receipt(
            &manifest,
            GcReceiptAction::Applied,
            manifest.applied_at_unix_ms,
        );
    }
    if manifest.state != TrashState::Prepared {
        return Err(VaultError::Conflict);
    }
    for entry in &manifest.entries {
        let original = vault.root().join(&entry.original_relative_path);
        let trashed = vault.root().join(&entry.trash_relative_path);
        match (original.exists(), trashed.exists()) {
            (true, false) => {
                if read_hash(&original)? != entry.content_hash {
                    return Err(VaultError::Conflict);
                }
                if let Some(parent) = trashed.parent() {
                    std::fs::create_dir_all(parent).map_err(|_| VaultError::Io)?;
                }
                std::fs::rename(original, trashed).map_err(|_| VaultError::Io)?;
            }
            (false, true) if read_hash(&trashed)? == entry.content_hash => {}
            _ => return Err(VaultError::Conflict),
        }
    }
    manifest.state = TrashState::Applied;
    atomic_replace(manifest_path, &encode(&manifest)?)?;
    let receipt = gc_receipt(
        &manifest,
        GcReceiptAction::Applied,
        manifest.applied_at_unix_ms,
    )?;
    write_gc_receipt(vault, &receipt)?;
    Ok(receipt)
}

fn referenced_evidence(vault: &ProjectVault) -> Result<BTreeSet<EvidenceId>, VaultError> {
    let mut referenced = BTreeSet::new();
    for path in sorted_files_recursive(&vault.root().join("wiki"))? {
        let relative = relative_path(vault.root(), &path);
        if relative == "wiki/index.md" || !relative.ends_with(".md") {
            continue;
        }
        if let Ok(page) = std::fs::read(&path)
            .map_err(|_| VaultError::Io)
            .and_then(|bytes| parse_wiki_page(&relative, &bytes))
        {
            referenced.extend(page.sources.into_iter().map(|source| source.source_id));
        }
    }
    for path in sorted_files_recursive(&vault.root().join(".minimax/pending"))? {
        if path.file_name().and_then(|value| value.to_str()) != Some("job.json") {
            continue;
        }
        if let Ok(job) = serde_json::from_slice::<KnowledgeEvaluationJob>(
            &std::fs::read(path).map_err(|_| VaultError::Io)?,
        ) {
            referenced.insert(job.source_id);
        }
    }
    Ok(referenced)
}

fn raw_evidence_paths(vault: &ProjectVault) -> Result<BTreeMap<String, EvidenceId>, VaultError> {
    let mut paths = BTreeMap::new();
    for metadata in sorted_files_recursive(&vault.root().join("raw/sessions"))? {
        if metadata.file_name().and_then(|value| value.to_str()) != Some("session.json") {
            continue;
        }
        let evidence = serde_json::from_slice::<FinalizedSessionEvidence>(
            &std::fs::read(&metadata).map_err(|_| VaultError::Io)?,
        )
        .map_err(|_| VaultError::RecoveryRequired)?;
        if let Some(parent) = metadata.parent() {
            for path in sorted_files_recursive(parent)? {
                paths.insert(
                    relative_path(vault.root(), &path),
                    evidence.evidence_id.clone(),
                );
            }
        }
    }
    for receipt_path in sorted_files_recursive(&vault.root().join(".minimax/imports"))? {
        let receipt = serde_json::from_slice::<InboxImportReceipt>(
            &std::fs::read(receipt_path).map_err(|_| VaultError::Io)?,
        )
        .map_err(|_| VaultError::RecoveryRequired)?
        .validate()
        .map_err(|_| VaultError::RecoveryRequired)?;
        paths.insert(receipt.imported_relative_path, receipt.evidence_id);
    }
    Ok(paths)
}

fn pinned_paths(vault: &ProjectVault) -> Result<BTreeSet<String>, VaultError> {
    let path = vault.root().join(".minimax/pins.json");
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice::<Vec<String>>(&bytes)
            .map(BTreeSet::from_iter)
            .map_err(|_| VaultError::RecoveryRequired),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(BTreeSet::new()),
        Err(_) => Err(VaultError::Io),
    }
}

fn candidate(
    relative_path: String,
    bytes: &[u8],
    class: GcClass,
    eligible_at_unix_ms: Option<u64>,
) -> Result<GcCandidate, VaultError> {
    Ok(GcCandidate {
        relative_path,
        content_hash: content_hash(bytes),
        bytes: u64::try_from(bytes.len()).map_err(|_| VaultError::RecoveryRequired)?,
        class,
        eligible_at_unix_ms,
    })
}

fn gc_receipt(
    manifest: &TrashManifest,
    action: GcReceiptAction,
    recorded_at_unix_ms: u64,
) -> Result<GcReceipt, VaultError> {
    Ok(GcReceipt {
        schema_version: SchemaVersion,
        gc_id: manifest.gc_id.clone(),
        plan_hash: manifest.plan_hash.clone(),
        action,
        object_count: u32::try_from(manifest.entries.len())
            .map_err(|_| VaultError::RecoveryRequired)?,
        bytes: manifest.entries.iter().try_fold(0_u64, |total, entry| {
            total
                .checked_add(entry.bytes)
                .ok_or(VaultError::RecoveryRequired)
        })?,
        recorded_at_unix_ms,
    })
}

fn write_gc_receipt(vault: &ProjectVault, receipt: &GcReceipt) -> Result<(), VaultError> {
    let action = match receipt.action {
        GcReceiptAction::Applied => "applied",
        GcReceiptAction::Undone => "undone",
        GcReceiptAction::Purged => "purged",
    };
    let path = vault.root().join(".minimax/receipts").join(format!(
        "gc-{}-{action}.json",
        safe_gc_directory(&receipt.gc_id)
    ));
    atomic_create_or_same(&path, &encode(receipt)?)
}

fn read_hash(path: &Path) -> Result<ContentHash, VaultError> {
    content_hash_result(std::fs::read(path).map_err(|_| VaultError::Io)?)
}

fn content_hash_result(bytes: Vec<u8>) -> Result<ContentHash, VaultError> {
    Ok(content_hash(&bytes))
}

fn encode<T: serde::Serialize>(value: &T) -> Result<Vec<u8>, VaultError> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|_| VaultError::RecoveryRequired)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn read_trash_manifest(path: &Path) -> Result<TrashManifest, VaultError> {
    serde_json::from_slice(&std::fs::read(path).map_err(|_| VaultError::RecoveryRequired)?)
        .map_err(|_| VaultError::RecoveryRequired)
}

fn trash_manifest_path(root: &Path, gc_id: &GcId) -> PathBuf {
    root.join(".minimax/trash")
        .join(safe_gc_directory(gc_id))
        .join("manifest.json")
}

fn safe_gc_directory(gc_id: &GcId) -> String {
    sha256_hex(gc_id.as_str().as_bytes())
}
