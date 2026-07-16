use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use minimax_cli::{
    DriverIds, ProviderPort, RuntimeDriver, finalize_active_session_wiki, resolve_project_vault,
    wiki_search,
};
use minimax_core::{WikiGenerationFuture, WikiGenerationOutput, WikiGenerationPort};
use minimax_protocol::{
    KnowledgeOperation, KnowledgePage, KnowledgePageStatus, KnowledgePatch, ModelBinding, ModelId,
    PageId, ProjectId, ProviderId, ProviderProtocolKind, RuntimeFailure, SchemaVersion,
    SourceCitation, StreamEvent, TerminalOutcome, TopicId, Usage,
};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
struct ProductProvider {
    runtime: Arc<Mutex<VecDeque<Vec<StreamEvent>>>>,
    wiki_calls: Arc<AtomicUsize>,
}

impl ProductProvider {
    fn completed(answer: &str) -> Self {
        Self {
            runtime: Arc::new(Mutex::new(VecDeque::from([vec![
                StreamEvent::VisibleTextDelta {
                    delta: answer.to_owned(),
                },
                StreamEvent::Usage { usage: usage() },
                StreamEvent::Terminal {
                    outcome: TerminalOutcome::Completed,
                },
            ]]))),
            wiki_calls: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl ProviderPort for ProductProvider {
    fn stream<'a>(
        &'a mut self,
        _request: &'a minimax_protocol::TurnRequest,
        _cancellation: &'a CancellationToken,
        emit: &'a mut (dyn FnMut(StreamEvent) + Send),
    ) -> Pin<Box<dyn Future<Output = Result<(), RuntimeFailure>> + Send + 'a>> {
        let events = self.runtime.lock().expect("runtime events").pop_front();
        Box::pin(async move {
            for event in events.ok_or_else(|| {
                RuntimeFailure::new(minimax_protocol::RuntimeErrorCode::ProtocolPrematureEof)
            })? {
                emit(event);
            }
            Ok(())
        })
    }
}

impl WikiGenerationPort for ProductProvider {
    fn generate<'a>(
        &'a self,
        request: &'a minimax_core::WikiGenerationRequest,
    ) -> WikiGenerationFuture<'a> {
        self.wiki_calls.fetch_add(1, Ordering::SeqCst);
        let patch = KnowledgePatch {
            schema_version: SchemaVersion,
            job_id: request.job.job_id.clone(),
            operations: vec![KnowledgeOperation::Create {
                page: KnowledgePage {
                    schema_version: SchemaVersion,
                    page_id: PageId::new("vault-architecture").expect("page ID"),
                    topic_id: TopicId::new("architecture").expect("topic ID"),
                    relative_path: "wiki/decisions/vault-architecture.md".to_owned(),
                    title: "Vault architecture".to_owned(),
                    status: KnowledgePageStatus::Current,
                    superseded_by: None,
                    sources: vec![SourceCitation {
                        source_id: request.job.source_id.clone(),
                        source_hash: request.job.source_hash.clone(),
                    }],
                    body: "The project uses one local Obsidian-compatible Vault.".to_owned(),
                },
            }],
        };
        Box::pin(async move {
            Ok(WikiGenerationOutput {
                raw_json: serde_json::to_string(&patch).expect("patch JSON"),
                usage: usage(),
            })
        })
    }
}

#[tokio::test]
async fn terminal_runtime_session_reaches_pinned_wiki_and_current_retrieval() {
    let project = tempfile::tempdir().expect("project");
    let vault = tempfile::tempdir().expect("vault");
    let resolved = resolve_project_vault(
        project.path(),
        Some(vault.path()),
        Some("project:integration"),
        1,
    )
    .expect("binding");
    let provider = ProductProvider::completed("We implemented the selected Vault design.");
    let wiki_calls = Arc::clone(&provider.wiki_calls);
    let mut driver = RuntimeDriver::open(
        project.path(),
        binding(),
        provider,
        DriverIds::new("lifecycle", 10),
    )
    .expect("driver");
    let finalized_session = driver.active_session_id().expect("active session");
    driver
        .run_prompt("我们决定采用 Vault 架构", 128)
        .await
        .expect("runtime turn");

    let report = finalize_active_session_wiki(&driver, &resolved.binding, 20)
        .await
        .expect("Wiki lifecycle")
        .expect("Wiki report");
    assert_eq!(
        report.receipt.outcome,
        minimax_protocol::KnowledgeReceiptOutcome::Synthesized
    );
    assert_eq!(report.receipt.model_binding, binding());
    assert_eq!(report.receipt.usage, Some(usage()));
    assert_eq!(wiki_calls.load(Ordering::SeqCst), 1);

    let result = wiki_search(
        project.path(),
        vault.path(),
        ProjectId::new("project:integration").expect("project ID"),
        "Obsidian Vault",
        5,
    )
    .expect("Wiki search");
    assert_eq!(result.results.len(), 1);
    assert_eq!(result.results[0].id, "vault-architecture");
    drop(driver);

    let reopened = RuntimeDriver::open(
        project.path(),
        binding(),
        ProductProvider::completed("unused"),
        DriverIds::new("reopened", 30),
    )
    .expect("reopened driver");
    assert_ne!(
        reopened.active_session_id().expect("new active session"),
        finalized_session
    );
}

#[tokio::test]
async fn lookup_only_session_gets_no_op_receipt_without_second_model_call() {
    let project = tempfile::tempdir().expect("project");
    let vault = tempfile::tempdir().expect("vault");
    let resolved = resolve_project_vault(
        project.path(),
        Some(vault.path()),
        Some("project:lookup"),
        1,
    )
    .expect("binding");
    let provider = ProductProvider::completed("The answer is local.");
    let wiki_calls = Arc::clone(&provider.wiki_calls);
    let mut driver = RuntimeDriver::open(
        project.path(),
        binding(),
        provider,
        DriverIds::new("lookup", 10),
    )
    .expect("driver");
    driver
        .run_prompt("what is this?", 128)
        .await
        .expect("runtime turn");
    let report = finalize_active_session_wiki(&driver, &resolved.binding, 20)
        .await
        .expect("Wiki lifecycle")
        .expect("Wiki report");
    assert_eq!(
        report.receipt.outcome,
        minimax_protocol::KnowledgeReceiptOutcome::NoOp
    );
    assert_eq!(wiki_calls.load(Ordering::SeqCst), 0);
}

fn binding() -> ModelBinding {
    ModelBinding {
        provider_id: ProviderId::new("provider:test").expect("provider"),
        model_id: ModelId::new("model:pinned").expect("model"),
        protocol: ProviderProtocolKind::Responses,
    }
}

const fn usage() -> Usage {
    Usage {
        input_tokens: Some(8),
        output_tokens: Some(5),
        total_tokens: Some(13),
    }
}
