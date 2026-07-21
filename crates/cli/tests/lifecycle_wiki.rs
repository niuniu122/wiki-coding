use std::collections::{BTreeMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use minimax_cli::{
    DriverIds, HttpProviderPort, ProviderPort, RuntimeDriver, finalize_active_session_wiki,
    resolve_project_vault, wiki_search,
};
use minimax_core::{WikiGenerationFuture, WikiGenerationOutput, WikiGenerationPort};
use minimax_protocol::{
    KnowledgeOperation, KnowledgePage, KnowledgePageStatus, KnowledgePatch, ModelBinding, ModelId,
    PageId, ProjectId, ProviderId, ProviderProtocolKind, RuntimeFailure, SchemaVersion,
    SourceCitation, StreamEvent, TerminalOutcome, TopicId, Usage,
};
use minimax_provider::{
    ConfigLayer, CredentialMode, CredentialResolver, HttpProviderClient, resolve_config,
};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
struct ProductProvider {
    runtime: Arc<Mutex<VecDeque<Vec<StreamEvent>>>>,
    wiki_calls: Arc<AtomicUsize>,
    binding: Arc<Mutex<ModelBinding>>,
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
            binding: Arc::new(Mutex::new(binding())),
        }
    }

    fn interrupted(answer: &str) -> Self {
        Self {
            runtime: Arc::new(Mutex::new(VecDeque::from([vec![
                StreamEvent::VisibleTextDelta {
                    delta: answer.to_owned(),
                },
                StreamEvent::Terminal {
                    outcome: TerminalOutcome::Interrupted,
                },
            ]]))),
            wiki_calls: Arc::new(AtomicUsize::new(0)),
            binding: Arc::new(Mutex::new(binding())),
        }
    }
}

impl ProviderPort for ProductProvider {
    fn rebind(&mut self, binding: &ModelBinding) {
        self.binding
            .lock()
            .expect("provider binding")
            .clone_from(binding);
    }

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
        if *self.binding.lock().expect("provider binding") != request.job.model_binding {
            return Box::pin(async { Err(minimax_core::WikiGenerationError::BindingMismatch) });
        }
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
async fn chat_think_blocks_are_absent_from_runtime_and_raw_vault_evidence() {
    let project = tempfile::tempdir().expect("project");
    let vault = tempfile::tempdir().expect("vault");
    let resolved = resolve_project_vault(
        project.path(),
        Some(vault.path()),
        Some("project:reasoning-boundary"),
        1,
    )
    .expect("Vault binding");
    let endpoint = chat_fixture_server(concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"prefix<th\"}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"ink>PRIVATE_REASONING\"}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"</thi\"}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"nk>suffix\"}}]}\n\n",
        "data: [DONE]\n\n"
    ))
    .await;
    let environment = BTreeMap::from([("MINIMAX_API_KEY".to_owned(), "fixture-secret".to_owned())]);
    let config =
        resolve_config(None, None, &environment, ConfigLayer::default()).expect("provider config");
    let credential = CredentialResolver::new(&environment, None)
        .resolve(&config, CredentialMode::Headless)
        .expect("credential");
    let binding = ModelBinding {
        provider_id: ProviderId::new("provider:test").expect("provider"),
        model_id: ModelId::new("model:reasoning-boundary").expect("model"),
        protocol: ProviderProtocolKind::ChatCompletions,
    };
    let provider = HttpProviderPort::new(
        HttpProviderClient::new(&endpoint, Some(std::time::Duration::from_secs(2)))
            .expect("HTTP provider"),
        credential,
        binding.clone(),
    );
    let mut driver = RuntimeDriver::open(
        project.path(),
        binding,
        provider,
        DriverIds::new("reasoning-boundary", 10),
    )
    .expect("driver");
    let report = driver
        .run_prompt("safe user prompt", 128)
        .await
        .expect("runtime turn");
    let visible = report
        .events
        .iter()
        .filter_map(|event| match &event.event {
            minimax_protocol::RuntimeEvent::VisibleTextDelta { delta } => Some(delta.as_str()),
            _ => None,
        })
        .collect::<String>();
    assert_eq!(visible, "prefixsuffix");

    let project_vault = resolved.binding.open().expect("open Vault");
    driver
        .finalize_active_session(&project_vault, 20)
        .expect("finalize raw evidence");
    let runtime =
        std::fs::read_to_string(project.path().join(".minimax/runtime/v1/sessions.jsonl"))
            .expect("runtime journal");
    let raw = read_tree_text(project_vault.root().join("raw"));
    for evidence in [&runtime, &raw] {
        assert!(evidence.contains("prefixsuffix"));
        assert!(!evidence.contains("PRIVATE_REASONING"));
        assert!(!evidence.contains("<think>"));
        assert!(!evidence.contains("</think>"));
        assert!(!evidence.contains("fixture-secret"));
    }
}

#[tokio::test]
async fn creating_a_model_session_rebinds_runtime_and_wiki_generation() {
    let project = tempfile::tempdir().expect("project");
    let vault = tempfile::tempdir().expect("vault");
    let resolved = resolve_project_vault(
        project.path(),
        Some(vault.path()),
        Some("project:model-switch"),
        1,
    )
    .expect("binding");
    let provider = ProductProvider::completed("We selected the switched model architecture.");
    let provider_binding = Arc::clone(&provider.binding);
    let mut driver = RuntimeDriver::open(
        project.path(),
        binding(),
        provider,
        DriverIds::new("model-switch", 10),
    )
    .expect("driver");

    let original_session = driver.active_session_id().expect("original session");
    let switched = switched_binding();
    let switched_session = driver
        .create_session(switched.clone())
        .expect("model session");
    driver
        .run_prompt("We decided to use the switched model architecture", 128)
        .await
        .expect("runtime turn");

    assert_eq!(
        driver
            .active_session_id()
            .and_then(|session_id| driver.session(&session_id))
            .map(|session| session.binding),
        Some(switched.clone())
    );
    assert_eq!(
        *provider_binding.lock().expect("provider binding"),
        switched
    );
    drop(driver);

    let reopened_provider = ProductProvider::completed("unused");
    let reopened_binding = Arc::clone(&reopened_provider.binding);
    let mut driver = RuntimeDriver::open(
        project.path(),
        binding(),
        reopened_provider,
        DriverIds::new("model-switch-reopen", 15),
    )
    .expect("reopened driver");
    assert_eq!(
        *reopened_binding.lock().expect("reopened provider binding"),
        switched_binding()
    );
    driver
        .resume(original_session)
        .expect("resume original model");
    assert_eq!(
        *reopened_binding.lock().expect("original provider binding"),
        binding()
    );
    driver
        .resume(switched_session)
        .expect("resume switched model");
    assert_eq!(
        *reopened_binding.lock().expect("switched provider binding"),
        switched_binding()
    );

    let report = finalize_active_session_wiki(&driver, &resolved.binding, 20)
        .await
        .expect("Wiki lifecycle")
        .expect("Wiki report");
    assert_eq!(
        report.receipt.outcome,
        minimax_protocol::KnowledgeReceiptOutcome::Synthesized
    );
    assert_eq!(report.receipt.model_binding, switched_binding());
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
    assert_eq!(wiki_calls.load(Ordering::SeqCst), 1);
    assert_matrix_responsibility(
        "test/cli-lifecycle.test.ts",
        "ts-lifecycle-finalize-before-restart",
        "terminal_runtime_session_reaches_pinned_wiki_and_current_retrieval",
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

#[tokio::test]
async fn interrupted_runtime_session_still_gets_a_durable_wiki_receipt() {
    let project = tempfile::tempdir().expect("project");
    let vault = tempfile::tempdir().expect("vault");
    let resolved = resolve_project_vault(
        project.path(),
        Some(vault.path()),
        Some("project:interrupted"),
        1,
    )
    .expect("binding");
    let provider = ProductProvider::interrupted("The Vault architecture decision was interrupted.");
    let wiki_calls = Arc::clone(&provider.wiki_calls);
    let mut driver = RuntimeDriver::open(
        project.path(),
        binding(),
        provider,
        DriverIds::new("interrupted", 10),
    )
    .expect("driver");
    let report = driver
        .run_prompt("We decided to use the Vault architecture", 128)
        .await
        .expect("interrupted runtime turn");
    assert_eq!(
        report.receipt.outcome,
        minimax_protocol::RuntimeTerminalOutcome::Interrupted
    );

    let wiki = finalize_active_session_wiki(&driver, &resolved.binding, 20)
        .await
        .expect("Wiki lifecycle")
        .expect("Wiki report");
    assert_eq!(
        wiki.receipt.outcome,
        minimax_protocol::KnowledgeReceiptOutcome::Synthesized
    );
    assert_eq!(wiki_calls.load(Ordering::SeqCst), 1);

    let vault = resolved.binding.open().expect("open Vault");
    let raw_session = std::fs::read_dir(vault.root().join("raw/sessions"))
        .expect("raw sessions")
        .next()
        .expect("one raw session")
        .expect("raw session entry")
        .path()
        .join("session.json");
    let evidence: minimax_vault::FinalizedSessionEvidence =
        serde_json::from_slice(&std::fs::read(raw_session).expect("session evidence"))
            .expect("valid session evidence");
    assert_eq!(evidence.session_id, report.receipt.session_id);
    let job = minimax_vault::knowledge_job_for_session(&evidence).expect("Wiki job");
    let store = minimax_vault::KnowledgeWorkflowStore::open(&vault, job).expect("Wiki store");
    assert_eq!(store.history().receipt(), Some(&wiki.receipt));
}

fn binding() -> ModelBinding {
    ModelBinding {
        provider_id: ProviderId::new("provider:test").expect("provider"),
        model_id: ModelId::new("model:pinned").expect("model"),
        protocol: ProviderProtocolKind::Responses,
    }
}

fn switched_binding() -> ModelBinding {
    ModelBinding {
        provider_id: ProviderId::new("provider:test").expect("provider"),
        model_id: ModelId::new("model:switched").expect("model"),
        protocol: ProviderProtocolKind::Responses,
    }
}

async fn chat_fixture_server(body: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind fixture");
    let address = listener.local_addr().expect("fixture address");
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept fixture");
        let mut request = vec![0_u8; 16 * 1024];
        let _ = socket.read(&mut request).await.expect("read request");
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        socket
            .write_all(response.as_bytes())
            .await
            .expect("write fixture response");
        let _ = socket.shutdown().await;
    });
    format!("http://{address}")
}

fn read_tree_text(root: std::path::PathBuf) -> String {
    let mut pending = vec![root];
    let mut text = String::new();
    while let Some(path) = pending.pop() {
        if path.is_dir() {
            let mut entries = std::fs::read_dir(path)
                .expect("read evidence directory")
                .collect::<Result<Vec<_>, _>>()
                .expect("evidence entries");
            entries.sort_by_key(std::fs::DirEntry::file_name);
            pending.extend(entries.into_iter().map(|entry| entry.path()));
        } else {
            text.push_str(&String::from_utf8_lossy(
                &std::fs::read(path).expect("read evidence file"),
            ));
        }
    }
    text
}

const fn usage() -> Usage {
    Usage {
        input_tokens: Some(8),
        output_tokens: Some(5),
        total_tokens: Some(13),
    }
}

fn assert_matrix_responsibility(source_path: &str, id: &str, test_name: &str) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("repository root");
    let matrix: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            root.join("fixtures/compat/verification/typescript-responsibilities.v1.json"),
        )
        .expect("coverage matrix"),
    )
    .expect("coverage matrix JSON");
    let source = matrix["sources"]
        .as_array()
        .expect("coverage sources")
        .iter()
        .find(|source| source["sourcePath"] == source_path)
        .expect("historical source");
    assert!(
        source["responsibilities"]
            .as_array()
            .expect("responsibilities")
            .iter()
            .any(|responsibility| responsibility["id"] == id
                && responsibility["evidence"]
                    .as_array()
                    .is_some_and(|evidence| evidence
                        .iter()
                        .any(|item| item["path"] == "crates/cli/tests/lifecycle_wiki.rs"
                            && item["test"] == test_name)))
    );
}
