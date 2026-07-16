use minimax_protocol::{
    ContentHash, EvidenceId, KnowledgeOperation, KnowledgePage, KnowledgePageStatus,
    KnowledgePatch, KnowledgeReceipt, KnowledgeReceiptOutcome, ModelBinding, ModelId, PageId,
    ProjectId, ProviderId, ProviderProtocolKind, SchemaVersion, SessionId, SourceCitation, TopicId,
    TransactionId, Usage, WikiWorkflowEvent, WikiWorkflowState,
};
use minimax_vault::{
    FinalizedSessionEvidence, KnowledgeWorkflowStore, ProjectVault, StoredGeneration, VaultError,
    ensure_knowledge_job, find_evaluation_missing, knowledge_job_for_session,
};

fn hash(byte: char) -> ContentHash {
    ContentHash::new(byte.to_string().repeat(64)).expect("hash")
}

fn evidence() -> FinalizedSessionEvidence {
    FinalizedSessionEvidence {
        schema_version: SchemaVersion,
        evidence_id: EvidenceId::new("session:one:aaaaaaaaaaaaaaaa").expect("evidence"),
        session_id: SessionId::new("session-one").expect("session"),
        binding: ModelBinding {
            provider_id: ProviderId::new("provider:test").expect("provider"),
            model_id: ModelId::new("model-pinned").expect("model"),
            protocol: ProviderProtocolKind::Responses,
        },
        created_at_unix_ms: 1,
        updated_at_unix_ms: 2,
        finalized_at_unix_ms: 3,
        turn_count: 1,
        event_count: 3,
        events_hash: hash('a'),
    }
}

fn patch(job_id: minimax_protocol::KnowledgeJobId) -> KnowledgePatch {
    KnowledgePatch {
        schema_version: SchemaVersion,
        job_id,
        operations: vec![KnowledgeOperation::Create {
            page: KnowledgePage {
                schema_version: SchemaVersion,
                page_id: PageId::new("page-1").expect("page"),
                topic_id: TopicId::new("topic-1").expect("topic"),
                relative_path: "wiki/decisions/page-1.md".to_owned(),
                title: "Decision".to_owned(),
                status: KnowledgePageStatus::Current,
                superseded_by: None,
                sources: vec![SourceCitation {
                    source_id: evidence().evidence_id,
                    source_hash: hash('a'),
                }],
                body: "Keep raw evidence.".to_owned(),
            },
        }],
    }
}

fn vault() -> (tempfile::TempDir, tempfile::TempDir, ProjectVault) {
    let project = tempfile::tempdir().expect("project");
    let root = tempfile::tempdir().expect("vault");
    let vault = ProjectVault::bootstrap(
        project.path(),
        root.path(),
        ProjectId::new("project").expect("project ID"),
        1,
    )
    .expect("bootstrap");
    (project, root, vault)
}

#[test]
fn workflow_job_generation_and_receipt_are_strict_idempotent_and_recoverable() {
    let (_project, _root, vault) = vault();
    let job = ensure_knowledge_job(&vault, &evidence()).expect("job");
    let mut store = KnowledgeWorkflowStore::open(&vault, job.clone()).expect("store");
    let event = WikiWorkflowEvent {
        schema_version: SchemaVersion,
        job_id: job.job_id.clone(),
        state: WikiWorkflowState::Generating,
        code: "generation_started".to_owned(),
        usage: None,
    };
    store.append_event(event.clone()).expect("event");
    store.append_event(event).expect("repeat event");
    store
        .append_generation(StoredGeneration::Started {
            attempt: 1,
            model_binding: job.model_binding.clone(),
        })
        .expect("started");
    let accepted = StoredGeneration::Accepted {
        attempt: 1,
        model_binding: job.model_binding.clone(),
        patch: patch(job.job_id.clone()),
        usage: Usage {
            input_tokens: Some(10),
            output_tokens: Some(5),
            total_tokens: Some(15),
        },
    };
    store.append_generation(accepted.clone()).expect("accepted");
    store
        .append_generation(accepted.clone())
        .expect("repeat accepted");
    let receipt = KnowledgeReceipt {
        schema_version: SchemaVersion,
        job_id: job.job_id.clone(),
        source_id: job.source_id.clone(),
        source_hash: job.source_hash.clone(),
        outcome: KnowledgeReceiptOutcome::Synthesized,
        code: "synthesized".to_owned(),
        model_binding: job.model_binding.clone(),
        usage: Some(Usage {
            input_tokens: Some(10),
            output_tokens: Some(5),
            total_tokens: Some(15),
        }),
        patch_hash: Some(hash('b')),
        transaction_id: Some(TransactionId::new("tx-1").expect("transaction")),
    };
    store.append_receipt(receipt.clone()).expect("receipt");
    drop(store);

    let reopened = KnowledgeWorkflowStore::open(&vault, job.clone()).expect("reopen");
    assert_eq!(reopened.history().receipt(), Some(&receipt));
    assert_eq!(
        reopened
            .history()
            .terminal_generation(1, &job.model_binding),
        Some(&accepted)
    );
    assert!(reopened.history().generation_started(1, &job.model_binding));
}

#[test]
fn conflicting_generation_outcome_fails_without_overwriting_history() {
    let (_project, _root, vault) = vault();
    let job = ensure_knowledge_job(&vault, &evidence()).expect("job");
    let mut store = KnowledgeWorkflowStore::open(&vault, job.clone()).expect("store");
    store
        .append_generation(StoredGeneration::SchemaRejected {
            attempt: 1,
            model_binding: job.model_binding.clone(),
            code: "schema_invalid".to_owned(),
            usage: Usage::default(),
        })
        .expect("first");
    assert!(matches!(
        store.append_generation(StoredGeneration::Accepted {
            attempt: 1,
            model_binding: job.model_binding.clone(),
            patch: patch(job.job_id),
            usage: Usage::default(),
        }),
        Err(VaultError::Conflict)
    ));
}

#[test]
fn finalized_evaluation_missing_window_is_found_once_in_stable_order() {
    let (_project, _root, vault) = vault();
    let raw = vault.root().join("raw/sessions/a");
    std::fs::create_dir_all(&raw).expect("raw");
    let mut bytes = serde_json::to_vec_pretty(&evidence()).expect("evidence JSON");
    bytes.push(b'\n');
    std::fs::write(raw.join("session.json"), bytes).expect("session metadata");
    assert_eq!(
        find_evaluation_missing(&vault).expect("missing"),
        vec![evidence()]
    );
    let job = ensure_knowledge_job(&vault, &evidence()).expect("ensure");
    assert_eq!(
        job,
        knowledge_job_for_session(&evidence()).expect("stable job")
    );
    assert!(
        find_evaluation_missing(&vault)
            .expect("recovered")
            .is_empty()
    );
}
