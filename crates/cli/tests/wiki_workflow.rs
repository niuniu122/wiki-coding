use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};

use minimax_cli::{
    JsonlWriter, MainModelWikiDriver, ProjectVaultBinding, VaultKnowledgePort, WikiDriverError,
    WikiFaultPoint,
};
use minimax_core::{
    DurabilitySignals, KnowledgeValidationContext, WikiEvidenceChunk, WikiGenerationError,
    WikiGenerationFuture, WikiGenerationOutput, WikiGenerationPort,
};
use minimax_protocol::{
    ContentHash, EvidenceId, KnowledgeJobId, KnowledgeOperation, KnowledgePage,
    KnowledgePageStatus, KnowledgePatch, KnowledgeReceiptOutcome, ModelBinding, ModelId, PageId,
    ProjectId, ProviderId, ProviderProtocolKind, SchemaVersion, SourceCitation, TopicId, Usage,
};
use minimax_vault::{FinalizedSessionEvidence, knowledge_job_for_session};

#[derive(Clone, Default)]
struct ScriptedWikiPort {
    inner: Arc<ScriptedInner>,
}

#[derive(Default)]
struct ScriptedInner {
    results: Mutex<VecDeque<Result<WikiGenerationOutput, WikiGenerationError>>>,
    bindings: Mutex<Vec<ModelBinding>>,
    repairs: Mutex<Vec<bool>>,
}

impl ScriptedWikiPort {
    fn new(results: Vec<Result<WikiGenerationOutput, WikiGenerationError>>) -> Self {
        Self {
            inner: Arc::new(ScriptedInner {
                results: Mutex::new(results.into()),
                ..ScriptedInner::default()
            }),
        }
    }

    fn calls(&self) -> usize {
        self.inner.bindings.lock().expect("bindings").len()
    }

    fn bindings(&self) -> Vec<ModelBinding> {
        self.inner.bindings.lock().expect("bindings").clone()
    }

    fn repairs(&self) -> Vec<bool> {
        self.inner.repairs.lock().expect("repairs").clone()
    }
}

impl WikiGenerationPort for ScriptedWikiPort {
    fn generate<'a>(
        &'a self,
        request: &'a minimax_core::WikiGenerationRequest,
    ) -> WikiGenerationFuture<'a> {
        self.inner
            .bindings
            .lock()
            .expect("bindings")
            .push(request.job.model_binding.clone());
        self.inner
            .repairs
            .lock()
            .expect("repairs")
            .push(request.schema_repair_only);
        let result = self
            .inner
            .results
            .lock()
            .expect("results")
            .pop_front()
            .unwrap_or(Err(WikiGenerationError::Failed));
        Box::pin(async move { result })
    }
}

fn hash(byte: char) -> ContentHash {
    ContentHash::new(byte.to_string().repeat(64)).expect("hash")
}

fn binding(model: &str) -> ModelBinding {
    ModelBinding {
        provider_id: ProviderId::new("provider:test").expect("provider"),
        model_id: ModelId::new(model).expect("model"),
        protocol: ProviderProtocolKind::Responses,
    }
}

fn evidence() -> FinalizedSessionEvidence {
    FinalizedSessionEvidence {
        schema_version: SchemaVersion,
        evidence_id: EvidenceId::new("session:one:aaaaaaaaaaaaaaaa").expect("evidence"),
        session_id: minimax_protocol::SessionId::new("session-one").expect("session"),
        binding: binding("model-pinned"),
        created_at_unix_ms: 1,
        updated_at_unix_ms: 2,
        finalized_at_unix_ms: 3,
        turn_count: 1,
        event_count: 3,
        events_hash: hash('a'),
    }
}

fn usage() -> Usage {
    Usage {
        input_tokens: Some(12),
        output_tokens: Some(6),
        total_tokens: Some(18),
    }
}

fn patch(source: EvidenceId, source_hash: ContentHash) -> KnowledgePatch {
    let job_id = knowledge_job_for_session(&evidence()).expect("job").job_id;
    KnowledgePatch {
        schema_version: SchemaVersion,
        job_id,
        operations: vec![KnowledgeOperation::Create {
            page: KnowledgePage {
                schema_version: SchemaVersion,
                page_id: PageId::new("architecture-current").expect("page"),
                topic_id: TopicId::new("architecture").expect("topic"),
                relative_path: "wiki/decisions/current-architecture.md".to_owned(),
                title: "Current architecture".to_owned(),
                status: KnowledgePageStatus::Current,
                superseded_by: None,
                sources: vec![SourceCitation {
                    source_id: source,
                    source_hash,
                }],
                body: "Raw evidence remains immutable and the Wiki is compiled current truth."
                    .to_owned(),
            },
        }],
    }
}

fn output(patch: &KnowledgePatch) -> Result<WikiGenerationOutput, WikiGenerationError> {
    Ok(WikiGenerationOutput {
        raw_json: serde_json::to_string(patch).expect("patch JSON"),
        usage: usage(),
    })
}

fn binding_fixture() -> (tempfile::TempDir, tempfile::TempDir, ProjectVaultBinding) {
    let project = tempfile::tempdir().expect("project");
    let vault = tempfile::tempdir().expect("vault");
    let binding = ProjectVaultBinding {
        project_root: project.path().to_path_buf(),
        vault_root: vault.path().to_path_buf(),
        project_id: ProjectId::new("project").expect("project ID"),
        created_at_unix_ms: 1,
    };
    binding.open().expect("bootstrap");
    (project, vault, binding)
}

fn validation() -> KnowledgeValidationContext {
    KnowledgeValidationContext {
        job_id: KnowledgeJobId::new("placeholder").expect("job"),
        evidence: BTreeMap::from([(evidence().evidence_id, hash('a'))]),
        pages: Vec::new(),
    }
}

fn chunks() -> Vec<WikiEvidenceChunk> {
    vec![WikiEvidenceChunk {
        source_id: evidence().evidence_id,
        source_hash: hash('a'),
        label: "finalized_session".to_owned(),
        content: "The user chose raw evidence plus one compiled Wiki view.".to_owned(),
    }]
}

#[tokio::test]
async fn no_value_session_writes_no_op_and_invokes_zero_provider_operations() {
    let (_project, _root, vault_binding) = binding_fixture();
    let generation = ScriptedWikiPort::default();
    let knowledge = VaultKnowledgePort::new(vault_binding.clone());
    let driver = MainModelWikiDriver::new(vault_binding, generation.clone(), knowledge);
    let report = driver
        .run(
            &evidence(),
            DurabilitySignals::default(),
            chunks(),
            Vec::new(),
            validation(),
        )
        .await
        .expect("no-op");
    assert_eq!(report.receipt.outcome, KnowledgeReceiptOutcome::NoOp);
    assert_eq!(report.receipt.usage, None);
    assert_eq!(generation.calls(), 0);
}

#[tokio::test]
async fn pinned_main_model_creates_current_page_with_separate_usage_and_idempotence() {
    let (_project, root, vault_binding) = binding_fixture();
    let valid = patch(evidence().evidence_id, hash('a'));
    let generation = ScriptedWikiPort::new(vec![output(&valid)]);
    let knowledge = VaultKnowledgePort::new(vault_binding.clone());
    let driver = MainModelWikiDriver::new(vault_binding, generation.clone(), knowledge);
    let signals = DurabilitySignals {
        architecture_changes: 1,
        ..DurabilitySignals::default()
    };
    let report = driver
        .run(&evidence(), signals, chunks(), Vec::new(), validation())
        .await
        .expect("synthesized");
    assert_eq!(report.receipt.outcome, KnowledgeReceiptOutcome::Synthesized);
    assert_eq!(report.receipt.model_binding, binding("model-pinned"));
    assert_eq!(report.receipt.usage, Some(usage()));
    assert_eq!(generation.bindings(), vec![binding("model-pinned")]);
    assert!(
        root.path()
            .join("wiki/decisions/current-architecture.md")
            .is_file()
    );
    let repeated = driver
        .run(&evidence(), signals, chunks(), Vec::new(), validation())
        .await
        .expect("repeat");
    assert_eq!(repeated.receipt, report.receipt);
    assert_eq!(generation.calls(), 1);

    let mut writer = JsonlWriter::new(Vec::new());
    writer
        .write_wiki_event(&report.events[0])
        .expect("event JSONL");
    writer
        .write_wiki_receipt(&report.receipt)
        .expect("receipt JSONL");
    let rendered = String::from_utf8(writer.into_inner()).expect("UTF-8");
    assert!(rendered.contains("model-pinned"));
    assert!(rendered.contains("synthesized"));
}

#[tokio::test]
async fn generation_and_commit_crashes_resume_without_a_second_model_call_or_log_entry() {
    for fault in [
        WikiFaultPoint::AfterGenerationRecord,
        WikiFaultPoint::AfterCommit,
    ] {
        let (_project, root, vault_binding) = binding_fixture();
        let valid = patch(evidence().evidence_id, hash('a'));
        let generation = ScriptedWikiPort::new(vec![output(&valid)]);
        let knowledge = VaultKnowledgePort::new(vault_binding.clone());
        let driver = MainModelWikiDriver::new(vault_binding, generation.clone(), knowledge);
        let signals = DurabilitySignals {
            decisions: 1,
            ..DurabilitySignals::default()
        };
        assert_eq!(
            driver
                .run_with_fault(
                    &evidence(),
                    signals,
                    chunks(),
                    Vec::new(),
                    validation(),
                    fault,
                )
                .await,
            Err(WikiDriverError::FaultInjected)
        );
        let report = driver
            .run(&evidence(), signals, chunks(), Vec::new(), validation())
            .await
            .expect("recover");
        assert_eq!(report.receipt.outcome, KnowledgeReceiptOutcome::Synthesized);
        assert_eq!(generation.calls(), 1);
        let log = std::fs::read_to_string(root.path().join("log.md")).expect("log");
        assert_eq!(log.matches("wiki job").count(), 1);
    }
}

#[tokio::test]
async fn unavailable_original_model_remains_pending_until_explicit_rebind() {
    let (_project, _root, vault_binding) = binding_fixture();
    let valid = patch(evidence().evidence_id, hash('a'));
    let generation =
        ScriptedWikiPort::new(vec![Err(WikiGenerationError::Unavailable), output(&valid)]);
    let knowledge = VaultKnowledgePort::new(vault_binding.clone());
    let driver = MainModelWikiDriver::new(vault_binding, generation.clone(), knowledge);
    let signals = DurabilitySignals {
        decisions: 1,
        ..DurabilitySignals::default()
    };
    let pending = driver
        .run(&evidence(), signals, chunks(), Vec::new(), validation())
        .await
        .expect("pending");
    assert_eq!(pending.receipt.outcome, KnowledgeReceiptOutcome::Pending);
    assert_eq!(pending.receipt.model_binding, binding("model-pinned"));
    let rebound = driver
        .run_rebound(
            &evidence(),
            signals,
            chunks(),
            Vec::new(),
            validation(),
            binding("model-explicit"),
        )
        .await
        .expect("rebound");
    assert_eq!(
        rebound.receipt.outcome,
        KnowledgeReceiptOutcome::Synthesized
    );
    assert_eq!(rebound.receipt.model_binding, binding("model-explicit"));
    assert_eq!(
        generation.bindings(),
        vec![binding("model-pinned"), binding("model-explicit")]
    );
}

#[tokio::test]
async fn schema_repair_is_bounded_and_invalid_provenance_replaces_zero_targets() {
    let (_project, root, vault_binding) = binding_fixture();
    let valid = patch(evidence().evidence_id, hash('a'));
    let generation = ScriptedWikiPort::new(vec![
        Ok(WikiGenerationOutput {
            raw_json: "{not-schema}".to_owned(),
            usage: usage(),
        }),
        output(&valid),
    ]);
    let knowledge = VaultKnowledgePort::new(vault_binding.clone());
    let driver = MainModelWikiDriver::new(vault_binding, generation.clone(), knowledge);
    let signals = DurabilitySignals {
        decisions: 1,
        ..DurabilitySignals::default()
    };
    let repaired = driver
        .run(&evidence(), signals, chunks(), Vec::new(), validation())
        .await
        .expect("repair");
    assert_eq!(
        repaired.receipt.outcome,
        KnowledgeReceiptOutcome::Synthesized
    );
    assert_eq!(generation.repairs(), vec![false, true]);

    let (_project, root_invalid, vault_binding) = binding_fixture();
    let fabricated = patch(EvidenceId::new("fabricated").expect("source"), hash('a'));
    let generation = ScriptedWikiPort::new(vec![output(&fabricated)]);
    let knowledge = VaultKnowledgePort::new(vault_binding.clone());
    let driver = MainModelWikiDriver::new(vault_binding, generation, knowledge);
    let failed = driver
        .run(&evidence(), signals, chunks(), Vec::new(), validation())
        .await
        .expect("failed receipt");
    assert_eq!(failed.receipt.outcome, KnowledgeReceiptOutcome::Failed);
    assert!(
        !root_invalid
            .path()
            .join("wiki/decisions/current-architecture.md")
            .exists()
    );
    assert!(
        root.path()
            .join("wiki/decisions/current-architecture.md")
            .exists()
    );
}
