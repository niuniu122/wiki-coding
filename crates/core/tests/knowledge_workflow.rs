use std::collections::BTreeMap;

use minimax_core::{
    CurrentWikiPage, DurabilityCode, DurabilityDecision, DurabilityGate, DurabilitySignals,
    KnowledgeEffect, KnowledgeGuardError, KnowledgeInput, KnowledgePatchValidator,
    KnowledgeValidationContext, MainModelWikiWorkflow, WikiWorkflowError,
};
use minimax_protocol::{
    ContentHash, EvidenceId, KnowledgeEvaluationJob, KnowledgeJobId, KnowledgeOperation,
    KnowledgePage, KnowledgePageStatus, KnowledgePatch, ModelBinding, ModelId, PageId, ProviderId,
    ProviderProtocolKind, SchemaVersion, SourceCitation, TopicId, TransactionId, Usage,
    WikiWorkflowState,
};

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

fn job() -> KnowledgeEvaluationJob {
    KnowledgeEvaluationJob {
        schema_version: SchemaVersion,
        job_id: KnowledgeJobId::new("job-1").expect("job"),
        source_id: EvidenceId::new("source-1").expect("source"),
        source_hash: hash('a'),
        model_binding: binding("model-pinned"),
        prompt_version: 1,
        patch_schema_version: 1,
        max_evidence_bytes: 4096,
        max_output_tokens: 512,
    }
}

fn usage() -> Usage {
    Usage {
        input_tokens: Some(10),
        output_tokens: Some(5),
        total_tokens: Some(15),
    }
}

fn page(id: &str, status: KnowledgePageStatus) -> KnowledgePage {
    KnowledgePage {
        schema_version: SchemaVersion,
        page_id: PageId::new(id).expect("page"),
        topic_id: TopicId::new("topic-1").expect("topic"),
        relative_path: format!("wiki/decisions/{id}.md"),
        title: format!("Decision {id}"),
        status,
        superseded_by: None,
        sources: vec![SourceCitation {
            source_id: EvidenceId::new("source-1").expect("source"),
            source_hash: hash('a'),
        }],
        body: "Source-grounded current decision.".to_owned(),
    }
}

fn patch() -> KnowledgePatch {
    KnowledgePatch {
        schema_version: SchemaVersion,
        job_id: KnowledgeJobId::new("job-1").expect("job"),
        operations: vec![KnowledgeOperation::Create {
            page: page("new-page", KnowledgePageStatus::Current),
        }],
    }
}

#[test]
fn durability_gate_is_deterministic_and_no_op_emits_zero_generation_effects() {
    assert_eq!(
        DurabilityGate::evaluate(DurabilitySignals {
            architecture_changes: 1,
            ..DurabilitySignals::default()
        }),
        DurabilityDecision::Durable(DurabilityCode::Architecture)
    );
    assert_eq!(
        DurabilityGate::evaluate(DurabilitySignals {
            lookup_only: true,
            ..DurabilitySignals::default()
        }),
        DurabilityDecision::NoOp(DurabilityCode::LookupOnly)
    );
    let mut workflow = MainModelWikiWorkflow::new(job()).expect("workflow");
    let effects = workflow
        .apply(
            KnowledgeInput::Evaluated(DurabilityDecision::NoOp(DurabilityCode::NoDurableSignal)),
            &[],
            &[],
        )
        .expect("no-op");
    assert_eq!(workflow.state(), WikiWorkflowState::NoOp);
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, KnowledgeEffect::Generate(_)))
    );
    assert!(effects.iter().any(|effect| matches!(effect, KnowledgeEffect::PersistReceipt(receipt) if receipt.usage.is_none())));
}

#[test]
fn workflow_pins_model_repairs_schema_once_and_keeps_usage_separate() {
    let mut workflow = MainModelWikiWorkflow::new(job()).expect("workflow");
    let effects = workflow
        .apply(
            KnowledgeInput::Evaluated(DurabilityDecision::Durable(DurabilityCode::Decision)),
            &[],
            &[],
        )
        .expect("durable");
    let request = effects
        .iter()
        .find_map(|effect| match effect {
            KnowledgeEffect::Generate(request) => Some(request),
            _ => None,
        })
        .expect("generation");
    assert_eq!(request.job.model_binding, binding("model-pinned"));
    assert!(!request.schema_repair_only);
    let repair = workflow
        .apply(
            KnowledgeInput::SchemaRejected {
                code: "schema_invalid".to_owned(),
                usage: usage(),
            },
            &[],
            &[],
        )
        .expect("repair");
    assert_eq!(workflow.generation_attempts(), 2);
    assert!(repair.iter().any(
        |effect| matches!(effect, KnowledgeEffect::Generate(request) if request.schema_repair_only)
    ));
    let failed = workflow
        .apply(
            KnowledgeInput::SchemaRejected {
                code: "schema_invalid".to_owned(),
                usage: usage(),
            },
            &[],
            &[],
        )
        .expect("exhausted");
    assert_eq!(workflow.state(), WikiWorkflowState::Failed);
    assert!(failed.iter().any(|effect| matches!(effect, KnowledgeEffect::PersistReceipt(receipt) if receipt.usage == Some(Usage { input_tokens: Some(20), output_tokens: Some(10), total_tokens: Some(30) }))));
    assert_eq!(
        workflow.apply(
            KnowledgeInput::GeneratedPatch {
                patch: patch(),
                usage: usage()
            },
            &[],
            &[]
        ),
        Err(WikiWorkflowError::InvalidTransition)
    );
}

#[test]
fn unavailable_binding_stays_pending_until_explicit_rebind() {
    let mut workflow = MainModelWikiWorkflow::new(job()).expect("workflow");
    workflow
        .apply(
            KnowledgeInput::Evaluated(DurabilityDecision::Durable(DurabilityCode::Decision)),
            &[],
            &[],
        )
        .expect("durable");
    workflow
        .apply(
            KnowledgeInput::GenerationUnavailable {
                code: "model_unavailable".to_owned(),
            },
            &[],
            &[],
        )
        .expect("pending");
    assert_eq!(workflow.state(), WikiWorkflowState::Pending);
    assert_eq!(workflow.job().model_binding, binding("model-pinned"));
    let effects = workflow
        .apply(
            KnowledgeInput::Rebind {
                model_binding: binding("model-explicit"),
            },
            &[],
            &[],
        )
        .expect("explicit rebind");
    assert_eq!(workflow.job().model_binding, binding("model-explicit"));
    assert!(effects.iter().any(|effect| matches!(effect, KnowledgeEffect::Generate(request) if request.job.model_binding == binding("model-explicit"))));
}

#[test]
fn validated_patch_commits_then_synthesizes_with_one_receipt() {
    let mut workflow = MainModelWikiWorkflow::new(job()).expect("workflow");
    workflow
        .apply(
            KnowledgeInput::Evaluated(DurabilityDecision::Durable(DurabilityCode::Decision)),
            &[],
            &[],
        )
        .expect("durable");
    workflow
        .apply(
            KnowledgeInput::GeneratedPatch {
                patch: patch(),
                usage: usage(),
            },
            &[],
            &[],
        )
        .expect("generated");
    let commit = workflow
        .apply(
            KnowledgeInput::PatchValidated {
                patch_hash: hash('b'),
            },
            &[],
            &[],
        )
        .expect("validated");
    assert!(
        commit
            .iter()
            .any(|effect| matches!(effect, KnowledgeEffect::Commit { .. }))
    );
    let done = workflow
        .apply(
            KnowledgeInput::Committed {
                transaction_id: TransactionId::new("tx-1").expect("transaction"),
            },
            &[],
            &[],
        )
        .expect("committed");
    assert_eq!(workflow.state(), WikiWorkflowState::Synthesized);
    assert_eq!(
        done.iter()
            .filter(|effect| matches!(effect, KnowledgeEffect::PersistReceipt(_)))
            .count(),
        1
    );
}

#[test]
fn patch_validator_rejects_sources_secrets_stale_hashes_and_conflicting_truth() {
    let context = KnowledgeValidationContext {
        job_id: KnowledgeJobId::new("job-1").expect("job"),
        evidence: BTreeMap::from([(EvidenceId::new("source-1").expect("source"), hash('a'))]),
        pages: Vec::new(),
    };
    KnowledgePatchValidator::validate(patch(), &context).expect("valid create");

    let mut invalid_source = patch();
    let KnowledgeOperation::Create { page } = &mut invalid_source.operations[0] else {
        unreachable!();
    };
    page.sources[0].source_id = EvidenceId::new("fabricated").expect("source");
    assert_eq!(
        KnowledgePatchValidator::validate(invalid_source, &context),
        Err(KnowledgeGuardError::UnknownSource)
    );
    let mut secret = patch();
    let KnowledgeOperation::Create { page } = &mut secret.operations[0] else {
        unreachable!();
    };
    page.body = "api_key=abcdefghijklmnop".to_owned();
    assert_eq!(
        KnowledgePatchValidator::validate(secret, &context),
        Err(KnowledgeGuardError::SensitiveContent)
    );

    let existing = CurrentWikiPage {
        page_id: PageId::new("old").expect("page"),
        topic_id: TopicId::new("topic-1").expect("topic"),
        relative_path: "wiki/decisions/old.md".to_owned(),
        status: KnowledgePageStatus::Current,
        superseded_by: None,
        content_hash: hash('c'),
    };
    let context = KnowledgeValidationContext {
        pages: vec![existing],
        ..context
    };
    assert_eq!(
        KnowledgePatchValidator::validate(patch(), &context),
        Err(KnowledgeGuardError::CurrentTruthConflict)
    );
}

#[test]
fn explicit_supersession_is_the_only_valid_second_current_path() {
    let old_id = PageId::new("old").expect("page");
    let new_id = PageId::new("new").expect("page");
    let old = CurrentWikiPage {
        page_id: old_id.clone(),
        topic_id: TopicId::new("topic-1").expect("topic"),
        relative_path: "wiki/decisions/old.md".to_owned(),
        status: KnowledgePageStatus::Current,
        superseded_by: None,
        content_hash: hash('c'),
    };
    let mut old_page = page("old", KnowledgePageStatus::Superseded);
    old_page.superseded_by = Some(new_id.clone());
    let mut new_page = page("new", KnowledgePageStatus::Current);
    new_page.page_id = new_id;
    let patch = KnowledgePatch {
        schema_version: SchemaVersion,
        job_id: KnowledgeJobId::new("job-1").expect("job"),
        operations: vec![
            KnowledgeOperation::Replace {
                page: old_page,
                expected_hash: hash('c'),
            },
            KnowledgeOperation::Create { page: new_page },
        ],
    };
    let context = KnowledgeValidationContext {
        job_id: KnowledgeJobId::new("job-1").expect("job"),
        evidence: BTreeMap::from([(EvidenceId::new("source-1").expect("source"), hash('a'))]),
        pages: vec![old],
    };
    KnowledgePatchValidator::validate(patch, &context).expect("valid supersession");
}
