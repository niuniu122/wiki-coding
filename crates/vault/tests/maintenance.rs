use minimax_protocol::{
    ContentHash, EvidenceId, KnowledgeOperation, KnowledgePage, KnowledgePageStatus,
    KnowledgePatch, KnowledgeReceipt, KnowledgeReceiptOutcome, ModelBinding, ModelId, PageId,
    ProjectId, ProviderId, ProviderProtocolKind, SchemaVersion, SessionId, SourceCitation, TopicId,
    TransactionId, Usage,
};
use minimax_vault::{
    FinalizedSessionEvidence, KnowledgeWorkflowStore, PreparedWikiTransaction, ProjectVault,
    StoredGeneration, TransactionFaultPoint, WikiChange, ensure_knowledge_job, hash_vault_bytes,
    lint_vault, rebuild_compiled_wiki, repair_vault,
};

fn vault() -> (tempfile::TempDir, tempfile::TempDir, ProjectVault) {
    let project = tempfile::tempdir().expect("project");
    let root = tempfile::tempdir().expect("vault");
    let vault = ProjectVault::bootstrap(
        project.path(),
        root.path(),
        ProjectId::new("maintenance-project").expect("project ID"),
        1,
    )
    .expect("bootstrap");
    (project, root, vault)
}

fn snapshot(root: &std::path::Path) -> Vec<(String, ContentHash)> {
    fn visit(
        root: &std::path::Path,
        current: &std::path::Path,
        output: &mut Vec<(String, ContentHash)>,
    ) {
        let mut entries = std::fs::read_dir(current)
            .expect("read directory")
            .collect::<Result<Vec<_>, _>>()
            .expect("entries");
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            if entry.file_type().expect("type").is_dir() {
                visit(root, &entry.path(), output);
            } else {
                let relative = entry
                    .path()
                    .strip_prefix(root)
                    .expect("relative")
                    .to_string_lossy()
                    .replace('\\', "/");
                if !relative.starts_with(".minimax/locks/") {
                    output.push((
                        relative,
                        hash_vault_bytes(&std::fs::read(entry.path()).expect("read file")),
                    ));
                }
            }
        }
    }
    let mut output = Vec::new();
    visit(root, root, &mut output);
    output
}

#[test]
fn maintenance_lint_is_read_only_stable_and_reports_owned_damage() {
    let (_project, _root, vault) = vault();
    let before = snapshot(vault.root());
    assert!(lint_vault(&vault).is_clean());
    assert_eq!(snapshot(vault.root()), before);

    std::fs::remove_file(vault.root().join("log.md")).expect("remove owned file");
    std::fs::write(
        vault.root().join("wiki/decisions/broken.md"),
        b"not frontmatter\n",
    )
    .expect("write invalid page");
    let first = lint_vault(&vault);
    let second = lint_vault(&vault);
    assert_eq!(first, second);
    assert_eq!(
        first
            .issues
            .iter()
            .map(|issue| (issue.code, issue.relative_path.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (minimax_protocol::VaultIssueCode::OwnedPathMissing, "log.md"),
            (
                minimax_protocol::VaultIssueCode::WikiPageInvalid,
                "wiki/decisions/broken.md",
            ),
        ]
    );
}

#[test]
fn maintenance_repair_only_quarantines_tail_and_rolls_prepared_transaction_forward() {
    let (_project, _root, vault) = vault();
    let transaction_id = TransactionId::new("repair-tx").expect("transaction");
    let mut transaction = PreparedWikiTransaction::prepare(
        &vault,
        transaction_id.clone(),
        vec![WikiChange {
            relative_path: "wiki/index.md".to_owned(),
            expected_old_hash: Some(hash_vault_bytes(b"# Project Wiki\n\n")),
            bytes: b"# Project Wiki\n\n- repaired\n".to_vec(),
        }],
        10,
    )
    .expect("prepare");
    transaction
        .roll_forward_with_fault(Some(TransactionFaultPoint::AfterPrepared))
        .expect_err("injected interruption");

    let pending = vault.root().join(".minimax/pending/test");
    std::fs::create_dir_all(&pending).expect("pending");
    std::fs::write(pending.join("journal.jsonl"), b"{\"sequence\":0}\npartial")
        .expect("partial journal");
    let receipt = repair_vault(&vault, 20).expect("repair");
    assert_eq!(receipt.recovered_transactions, vec![transaction_id]);
    assert_eq!(receipt.quarantined_fragments.len(), 1);
    assert!(receipt.remaining_issues.is_empty());
    assert_eq!(
        std::fs::read(vault.root().join("wiki/index.md")).expect("index"),
        b"# Project Wiki\n\n- repaired\n"
    );
    let repeated = repair_vault(&vault, 21).expect("repeat repair");
    assert!(repeated.recovered_transactions.is_empty());
    assert!(repeated.quarantined_fragments.is_empty());
}

#[test]
fn maintenance_rebuild_replays_durable_patch_and_preserves_every_raw_byte() {
    let (_project, _root, vault) = vault();
    let events = b"{\"safe\":true}\n";
    let events_hash = hash_vault_bytes(events);
    let evidence = FinalizedSessionEvidence {
        schema_version: SchemaVersion,
        evidence_id: EvidenceId::new("session:rebuild:source").expect("evidence"),
        session_id: SessionId::new("session-rebuild").expect("session"),
        binding: ModelBinding {
            provider_id: ProviderId::new("provider:scripted").expect("provider"),
            model_id: ModelId::new("model-pinned").expect("model"),
            protocol: ProviderProtocolKind::Responses,
        },
        created_at_unix_ms: 1,
        updated_at_unix_ms: 2,
        finalized_at_unix_ms: 3,
        turn_count: 1,
        event_count: 1,
        events_hash: events_hash.clone(),
    };
    let raw = vault.root().join("raw/sessions/rebuild");
    std::fs::create_dir_all(&raw).expect("raw");
    std::fs::write(raw.join("events.jsonl"), events).expect("events");
    let mut metadata = serde_json::to_vec_pretty(&evidence).expect("metadata");
    metadata.push(b'\n');
    std::fs::write(raw.join("session.json"), metadata).expect("session metadata");

    let job = ensure_knowledge_job(&vault, &evidence).expect("job");
    let page = KnowledgePage {
        schema_version: SchemaVersion,
        page_id: PageId::new("page-rebuild").expect("page"),
        topic_id: TopicId::new("topic-rebuild").expect("topic"),
        relative_path: "wiki/decisions/rebuild.md".to_owned(),
        title: "Rebuild decision".to_owned(),
        status: KnowledgePageStatus::Current,
        superseded_by: None,
        sources: vec![SourceCitation {
            source_id: evidence.evidence_id.clone(),
            source_hash: events_hash.clone(),
        }],
        body: "Raw evidence remains authoritative.".to_owned(),
    };
    let patch = KnowledgePatch {
        schema_version: SchemaVersion,
        job_id: job.job_id.clone(),
        operations: vec![KnowledgeOperation::Create { page }],
    };
    let mut store = KnowledgeWorkflowStore::open(&vault, job.clone()).expect("store");
    store
        .append_generation(StoredGeneration::Accepted {
            attempt: 1,
            model_binding: job.model_binding.clone(),
            patch,
            usage: Usage::default(),
        })
        .expect("accepted patch");
    store
        .append_receipt(KnowledgeReceipt {
            schema_version: SchemaVersion,
            job_id: job.job_id,
            source_id: job.source_id,
            source_hash: job.source_hash,
            outcome: KnowledgeReceiptOutcome::Synthesized,
            code: "synthesized".to_owned(),
            model_binding: job.model_binding,
            usage: Some(Usage::default()),
            patch_hash: Some(hash_vault_bytes(b"patch")),
            transaction_id: Some(TransactionId::new("original-wiki-tx").expect("tx")),
        })
        .expect("receipt");
    drop(store);

    let raw_before = snapshot(&vault.root().join("raw"));
    let first = rebuild_compiled_wiki(&vault, 30).expect("rebuild");
    let compiled_before = snapshot(vault.root());
    let second = rebuild_compiled_wiki(&vault, 31).expect("repeat rebuild");
    assert_eq!(first.transaction_id, second.transaction_id);
    assert_eq!(snapshot(vault.root()), compiled_before);
    assert_eq!(snapshot(&vault.root().join("raw")), raw_before);
    assert!(lint_vault(&vault).is_clean());
}
