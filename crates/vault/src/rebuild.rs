use std::collections::{BTreeMap, BTreeSet};

use minimax_protocol::{
    KnowledgeOperation, KnowledgePage, KnowledgePageStatus, PageId, RebuildReceipt, SchemaVersion,
    TransactionId,
};

use crate::bootstrap::{ProjectVault, VaultError};
use crate::lint::{evidence_inventory, lint_vault, raw_snapshot, sorted_files_recursive};
use crate::page::render_wiki_page;
use crate::path::{content_hash, sha256_hex};
use crate::transaction::{
    PreparedWikiTransaction, WikiChange, recover_wiki_transaction, wiki_transaction_exists,
};
use crate::workflow::synthesized_knowledge_patches;

pub fn rebuild_compiled_wiki(
    vault: &ProjectVault,
    recorded_at_unix_ms: u64,
) -> Result<RebuildReceipt, VaultError> {
    let before = raw_snapshot(vault)?;
    let raw_snapshot_hash =
        content_hash(&serde_json::to_vec(&before).map_err(|_| VaultError::RecoveryRequired)?);
    let evidence = evidence_inventory(vault, &mut Vec::new());
    let mut pages = BTreeMap::<PageId, KnowledgePage>::new();
    for patch in synthesized_knowledge_patches(vault)? {
        for operation in patch
            .validate()
            .map_err(|_| VaultError::RecoveryRequired)?
            .operations
        {
            match operation {
                KnowledgeOperation::Create { page } | KnowledgeOperation::Replace { page, .. } => {
                    pages.insert(page.page_id.clone(), page);
                }
                KnowledgeOperation::Remove { page_id, .. } => {
                    pages.remove(&page_id);
                }
            }
        }
    }
    validate_projection(&pages, &evidence)?;

    let desired_paths = pages
        .values()
        .map(|page| page.relative_path.clone())
        .collect::<BTreeSet<_>>();
    for path in sorted_files_recursive(&vault.root().join("wiki"))? {
        let relative = path
            .strip_prefix(vault.root())
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        if relative != "wiki/index.md"
            && relative.ends_with(".md")
            && !desired_paths.contains(&relative)
        {
            return Err(VaultError::Conflict);
        }
    }

    let mut targets = Vec::new();
    for page in pages.values() {
        targets.push(change_for(
            vault,
            &page.relative_path,
            render_wiki_page(page)?,
        )?);
    }
    targets.push(change_for(vault, "wiki/index.md", render_index(&pages))?);
    targets.push(change_for(vault, "log.md", render_log(&pages))?);
    targets.push(change_for(
        vault,
        ".minimax/indexes/wiki-v1.json",
        render_index_record(&pages)?,
    )?);

    let identity = serde_json::to_vec(&(
        &raw_snapshot_hash,
        targets
            .iter()
            .map(|target| (&target.relative_path, content_hash(&target.bytes)))
            .collect::<Vec<_>>(),
    ))
    .map_err(|_| VaultError::RecoveryRequired)?;
    let transaction_id = TransactionId::new(format!("rebuild:{}", &sha256_hex(&identity)[..24]))
        .map_err(|_| VaultError::RecoveryRequired)?;
    let transaction = if wiki_transaction_exists(vault, &transaction_id) {
        recover_wiki_transaction(vault, &transaction_id)?;
        Some(transaction_id)
    } else {
        match PreparedWikiTransaction::prepare(
            vault,
            transaction_id.clone(),
            targets,
            recorded_at_unix_ms,
        ) {
            Ok(transaction) => {
                transaction.roll_forward()?;
                Some(transaction_id)
            }
            Err(VaultError::EmptyTransaction) => None,
            Err(error) => return Err(error),
        }
    };

    if raw_snapshot(vault)? != before {
        return Err(VaultError::RecoveryRequired);
    }
    let report = lint_vault(vault);
    if !report.is_clean() {
        return Err(VaultError::RecoveryRequired);
    }
    Ok(RebuildReceipt {
        schema_version: SchemaVersion,
        operation_id: format!("rebuild:{}", &sha256_hex(&identity)[..24]),
        raw_snapshot_hash,
        raw_object_count: u32::try_from(before.len()).map_err(|_| VaultError::RecoveryRequired)?,
        page_count: u32::try_from(pages.len()).map_err(|_| VaultError::RecoveryRequired)?,
        code: if transaction.is_some() {
            "rebuilt".to_owned()
        } else {
            "no_changes".to_owned()
        },
        transaction_id: transaction,
        recorded_at_unix_ms,
    })
}

fn validate_projection(
    pages: &BTreeMap<PageId, KnowledgePage>,
    evidence: &BTreeMap<minimax_protocol::EvidenceId, minimax_protocol::ContentHash>,
) -> Result<(), VaultError> {
    let mut paths = BTreeSet::new();
    let mut current_topics = BTreeSet::new();
    for page in pages.values() {
        page.clone()
            .validate()
            .map_err(|_| VaultError::InvalidPage)?;
        if !paths.insert(page.relative_path.clone()) {
            return Err(VaultError::InvalidPage);
        }
        if page.status == KnowledgePageStatus::Current
            && !current_topics.insert(page.topic_id.clone())
        {
            return Err(VaultError::InvalidPage);
        }
        if page
            .sources
            .iter()
            .any(|source| evidence.get(&source.source_id) != Some(&source.source_hash))
        {
            return Err(VaultError::RecoveryRequired);
        }
    }
    Ok(())
}

fn change_for(
    vault: &ProjectVault,
    relative_path: &str,
    bytes: Vec<u8>,
) -> Result<WikiChange, VaultError> {
    let path = vault.root().join(relative_path);
    let expected_old_hash = match std::fs::read(path) {
        Ok(bytes) => Some(content_hash(&bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => return Err(VaultError::Io),
    };
    Ok(WikiChange {
        relative_path: relative_path.to_owned(),
        expected_old_hash,
        bytes,
    })
}

fn render_index(pages: &BTreeMap<PageId, KnowledgePage>) -> Vec<u8> {
    let mut output = String::from("# Project Wiki\n\n");
    for page in pages
        .values()
        .filter(|page| page.status == KnowledgePageStatus::Current)
    {
        output.push_str(&format!(
            "- [{}]({})\n",
            page.title,
            &page.relative_path[5..]
        ));
    }
    output.into_bytes()
}

fn render_log(pages: &BTreeMap<PageId, KnowledgePage>) -> Vec<u8> {
    let mut output = String::from("# MiniMax Knowledge Log\n\n");
    for page in pages.values() {
        let status = match page.status {
            KnowledgePageStatus::Current => "current",
            KnowledgePageStatus::Superseded => "superseded",
        };
        output.push_str(&format!(
            "- {} | {} | {status}\n",
            page.page_id.as_str(),
            page.relative_path
        ));
    }
    output.into_bytes()
}

fn render_index_record(pages: &BTreeMap<PageId, KnowledgePage>) -> Result<Vec<u8>, VaultError> {
    let records = pages
        .values()
        .map(|page| {
            serde_json::json!({
                "pageId": page.page_id.as_str(),
                "topicId": page.topic_id.as_str(),
                "relativePath": page.relative_path,
                "status": match page.status {
                    KnowledgePageStatus::Current => "current",
                    KnowledgePageStatus::Superseded => "superseded",
                },
            })
        })
        .collect::<Vec<_>>();
    let mut bytes =
        serde_json::to_vec_pretty(&records).map_err(|_| VaultError::RecoveryRequired)?;
    bytes.push(b'\n');
    Ok(bytes)
}
