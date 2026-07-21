use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use minimax_cli::{
    DriverIds, ExitClass, HeadlessApprovalPort, ProviderPort, RuntimeDriver, exit_for_report,
};
use minimax_core::{
    CancellationPort, CompactionBudget, PermissionMode, ToolExecutionContext, ToolFuture,
    ToolLifecycleFuture, ToolPort,
};
use minimax_protocol::{
    AgentLimits, FULL_ACCESS_TOOL_NAMES, ModelBinding, ModelId, ProviderId, ProviderProtocolKind,
    RuntimeErrorCode, RuntimeFailure, RuntimeTerminalOutcome, SHELL_TOOL_NAMES, SchemaVersion,
    ShellReceipt, ShellSessionId, ShellSessionState, StreamEvent, TerminalOutcome,
    ToolCallFragment, ToolCallId, ToolInvocation, ToolResult, ToolTerminalStatus, TurnId,
    TurnStatus, Usage,
};
use minimax_tools::BuiltinToolPort;
use minimax_vault::{RuntimeStore, RuntimeStoreError};
use tokio_util::sync::CancellationToken;

enum MockRun {
    Events(Vec<StreamEvent>),
    WaitForCancellation,
}

struct MockProvider {
    runs: VecDeque<MockRun>,
}

impl MockProvider {
    fn completed(contents: &[&str]) -> MockRun {
        let mut events = contents
            .iter()
            .map(|delta| StreamEvent::VisibleTextDelta {
                delta: (*delta).to_owned(),
            })
            .collect::<Vec<_>>();
        events.push(StreamEvent::Usage {
            usage: Usage {
                input_tokens: Some(3),
                output_tokens: Some(2),
                total_tokens: Some(5),
            },
        });
        events.push(StreamEvent::Terminal {
            outcome: TerminalOutcome::Completed,
        });
        MockRun::Events(events)
    }
}

impl ProviderPort for MockProvider {
    fn rebind(&mut self, _binding: &ModelBinding) {}

    fn stream<'a>(
        &'a mut self,
        _request: &'a minimax_protocol::TurnRequest,
        cancellation: &'a CancellationToken,
        emit: &'a mut (dyn FnMut(StreamEvent) + Send),
    ) -> Pin<Box<dyn Future<Output = Result<(), RuntimeFailure>> + Send + 'a>> {
        Box::pin(async move {
            match self
                .runs
                .pop_front()
                .ok_or_else(|| RuntimeFailure::new(RuntimeErrorCode::ProtocolPrematureEof))?
            {
                MockRun::Events(events) => {
                    for event in events {
                        emit(event);
                        tokio::task::yield_now().await;
                    }
                    Ok(())
                }
                MockRun::WaitForCancellation => {
                    cancellation.cancelled().await;
                    Err(RuntimeFailure::new(RuntimeErrorCode::Interrupted))
                }
            }
        })
    }
}

#[derive(Default)]
struct ProcessShellState {
    accepting: bool,
    sessions: Vec<String>,
}

#[derive(Clone, Default)]
struct ProcessScopedShellPort {
    state: Arc<Mutex<ProcessShellState>>,
}

impl ToolPort for ProcessScopedShellPort {
    fn preflight(
        &self,
        invocation: &ToolInvocation,
        context: ToolExecutionContext,
        _cancellation: &dyn CancellationPort,
    ) -> Result<(), ToolResult> {
        if SHELL_TOOL_NAMES.contains(&invocation.call.name.as_str())
            && (context.permission_mode() != PermissionMode::FullAccess
                || !self.state.lock().expect("shell state").accepting)
        {
            return Err(shell_result(
                invocation,
                ToolTerminalStatus::Rejected,
                "shell_requires_full_access",
                None,
            ));
        }
        Ok(())
    }

    fn execute<'a>(
        &'a self,
        invocation: &'a ToolInvocation,
        _context: ToolExecutionContext,
        _cancellation: &'a dyn CancellationPort,
    ) -> ToolFuture<'a> {
        Box::pin(async move {
            match invocation.call.name.as_str() {
                "shell_command" => {
                    let mut state = self.state.lock().expect("shell state");
                    let session_id = format!("shell-process-{:04}", state.sessions.len() + 1);
                    state.sessions.push(session_id.clone());
                    shell_result(
                        invocation,
                        ToolTerminalStatus::Succeeded,
                        "shell_running",
                        Some(running_receipt(&session_id)),
                    )
                }
                "shell_session" => {
                    let arguments: serde_json::Value =
                        serde_json::from_str(&invocation.call.arguments_json)
                            .expect("session arguments");
                    let session_id = arguments["session_id"].as_str().unwrap_or_default();
                    if self
                        .state
                        .lock()
                        .expect("shell state")
                        .sessions
                        .iter()
                        .any(|known| known == session_id)
                    {
                        shell_result(
                            invocation,
                            ToolTerminalStatus::Succeeded,
                            "shell_running",
                            Some(running_receipt(session_id)),
                        )
                    } else {
                        shell_result(
                            invocation,
                            ToolTerminalStatus::Rejected,
                            "shell_session_not_found",
                            None,
                        )
                    }
                }
                _ => shell_result(invocation, ToolTerminalStatus::Succeeded, "ok", None),
            }
        })
    }

    fn transition_permission<'a>(&'a self, mode: PermissionMode) -> ToolLifecycleFuture<'a> {
        Box::pin(async move {
            let mut state = self.state.lock().expect("shell state");
            state.accepting = mode == PermissionMode::FullAccess;
            if mode == PermissionMode::Confirm {
                state.sessions.clear();
            }
            Ok(())
        })
    }

    fn shutdown<'a>(&'a self) -> ToolLifecycleFuture<'a> {
        Box::pin(async move {
            let mut state = self.state.lock().expect("shell state");
            state.accepting = false;
            state.sessions.clear();
            Ok(())
        })
    }
}

fn shell_result(
    invocation: &ToolInvocation,
    status: ToolTerminalStatus,
    code: &str,
    output: Option<String>,
) -> ToolResult {
    ToolResult {
        schema_version: SchemaVersion,
        call_id: invocation.call.call_id.clone(),
        tool_name: invocation.call.name.clone(),
        status,
        code: code.to_owned(),
        output,
    }
}

fn running_receipt(session_id: &str) -> String {
    serde_json::to_string(
        &ShellReceipt::new(
            ShellSessionId::new(session_id).expect("session id"),
            ShellSessionState::Running,
            None,
            String::new(),
            false,
        )
        .expect("receipt"),
    )
    .expect("receipt JSON")
}

fn shell_tool_run(name: &str, arguments: serde_json::Value) -> MockRun {
    MockRun::Events(vec![
        StreamEvent::ToolCallFragments {
            fragments: vec![ToolCallFragment {
                call_id: ToolCallId::new(format!("call-{name}")).expect("call id"),
                stream_id: Some(format!("stream-{name}")),
                name: Some(name.to_owned()),
                arguments_delta: Some(arguments.to_string()),
                arguments_complete: true,
                index: Some(0),
            }],
        },
        StreamEvent::Terminal {
            outcome: TerminalOutcome::Completed,
        },
    ])
}

fn full_tool_definitions() -> Vec<minimax_protocol::ToolDefinition> {
    let definitions = BuiltinToolPort::definitions_for(PermissionMode::FullAccess)
        .expect("full-access definitions");
    assert_eq!(
        definitions
            .iter()
            .map(|definition| definition.name.as_str())
            .collect::<Vec<_>>(),
        FULL_ACCESS_TOOL_NAMES
    );
    definitions
}

#[tokio::test]
async fn conversation_reconstructs_then_lists_resumes_continues_retries_and_compacts() {
    let project = tempfile::tempdir().expect("temporary project");
    let first_session;
    let first_turn;
    {
        let provider = MockProvider {
            runs: VecDeque::from([MockProvider::completed(&["first ", "answer"])]),
        };
        let mut driver = RuntimeDriver::open(
            project.path(),
            binding(),
            provider,
            DriverIds::new("process-one", 10_000),
        )
        .expect("first process");
        first_session = driver.active_session_id().expect("active session");
        let report = driver
            .run_prompt("first prompt", 128)
            .await
            .expect("first turn");
        first_turn = report.receipt.turn_id.clone();
        assert_eq!(driver.latest_retryable_turn_id(), Some(first_turn.clone()));
        assert_eq!(exit_for_report(&report), ExitClass::Completed);
        assert_eq!(driver.list_sessions().expect("list").len(), 1);

        let second_session = driver.create_session(binding()).expect("new session");
        assert_ne!(first_session, second_session);
        driver.resume(first_session.clone()).expect("resume first");
        assert_eq!(driver.list_sessions().expect("list two").len(), 2);
        assert!(matches!(
            RuntimeStore::open(project.path()),
            Err(RuntimeStoreError::Busy)
        ));
    }

    {
        let provider = MockProvider {
            runs: VecDeque::from([
                MockProvider::completed(&["continued"]),
                MockProvider::completed(&["retried"]),
            ]),
        };
        let mut driver = RuntimeDriver::open(
            project.path(),
            binding(),
            provider,
            DriverIds::new("process-two", 20_000),
        )
        .expect("restarted process");
        assert_eq!(driver.active_session_id(), Some(first_session.clone()));
        let reconstructed = driver
            .session(&first_session)
            .expect("reconstructed session");
        assert_eq!(reconstructed.turns.len(), 1);
        assert_eq!(
            reconstructed.turns[0]
                .assistant_message
                .as_ref()
                .expect("assistant")
                .content,
            "first answer"
        );

        driver
            .run_prompt("second prompt", 128)
            .await
            .expect("continued turn");
        let retry = driver
            .retry_turn(first_turn.clone(), 128)
            .await
            .expect("retry turn");
        let compact = driver
            .compact_active(CompactionBudget {
                max_record_bytes: 64 * 1024,
                retain_recent_turns: 2,
            })
            .expect("local compaction");
        assert_eq!(compact.retained_recent_turns.len(), 2);
        assert_ne!(retry.receipt.turn_id, first_turn);
    }

    let store = RuntimeStore::open(project.path()).expect("third process recovery");
    let session = store
        .machine()
        .sessions()
        .get(&first_session)
        .expect("persisted first session");
    assert_eq!(session.turns.len(), 3);
    assert!(session.turns.iter().all(|turn| turn.status.is_terminal()));
    assert!(session.compaction.is_some());
}

#[tokio::test]
async fn retry_and_continue_execute_distinct_durable_outcomes() {
    let project = tempfile::tempdir().expect("temporary project");
    let provider = MockProvider {
        runs: VecDeque::from([
            MockProvider::completed(&["source answer"]),
            MockProvider::completed(&["continued answer"]),
            MockProvider::completed(&["retried answer"]),
        ]),
    };
    let mut driver = RuntimeDriver::open(
        project.path(),
        binding(),
        provider,
        DriverIds::new("retry-continue", 25_000),
    )
    .expect("driver");

    let source = driver
        .run_prompt("source prompt", 128)
        .await
        .expect("source turn");
    let session_id = source.receipt.session_id.clone();
    let source_turn_id = source.receipt.turn_id.clone();
    let source_before = driver.session(&session_id).expect("session").turns[0].clone();

    let continued = driver
        .run_prompt("continue with a new prompt", 128)
        .await
        .expect("continued turn");
    let retried = driver
        .retry_turn(source_turn_id.clone(), 128)
        .await
        .expect("retried turn");

    assert_ne!(continued.receipt.turn_id, source_turn_id);
    assert_ne!(retried.receipt.turn_id, source_turn_id);
    assert_ne!(continued.receipt.turn_id, retried.receipt.turn_id);
    assert_ne!(continued.receipt.request_id, source.receipt.request_id);
    assert_ne!(retried.receipt.request_id, source.receipt.request_id);
    assert_ne!(continued.receipt.request_id, retried.receipt.request_id);
    assert_eq!(continued.receipt.outcome, RuntimeTerminalOutcome::Completed);
    assert_eq!(retried.receipt.outcome, RuntimeTerminalOutcome::Completed);

    let turns = &driver.session(&session_id).expect("session").turns;
    assert_eq!(turns.len(), 3);
    assert_eq!(turns[0], source_before, "retry must not rewrite its source");
    assert!(turns[1].retry_of.is_none(), "continue is a normal new turn");
    assert_eq!(
        turns[2].retry_of.as_ref(),
        Some(&source_turn_id),
        "retry records its immutable terminal source"
    );
    assert!(turns.iter().all(|turn| turn.status.is_terminal()));
    drop(driver);

    let replayed = RuntimeStore::open(project.path()).expect("persisted replay");
    let turns = &replayed
        .machine()
        .sessions()
        .get(&session_id)
        .expect("replayed session")
        .turns;
    assert_eq!(turns.len(), 3);
    assert_eq!(turns[0], source_before);
    assert!(turns[1].retry_of.is_none());
    assert_eq!(turns[2].retry_of.as_ref(), Some(&source_turn_id));
    assert!(turns.iter().all(|turn| turn.status.is_terminal()));
}

#[tokio::test]
async fn controlled_cancellation_persists_once_and_releases_lease() {
    let project = tempfile::tempdir().expect("temporary project");
    let session_id;
    let turn_id: TurnId;
    {
        let provider = MockProvider {
            runs: VecDeque::from([MockRun::WaitForCancellation]),
        };
        let mut driver = RuntimeDriver::open(
            project.path(),
            binding(),
            provider,
            DriverIds::new("cancel", 30_000),
        )
        .expect("driver");
        session_id = driver.active_session_id().expect("active session");
        let cancellation = driver.cancellation_token();
        tokio::spawn(async move {
            tokio::task::yield_now().await;
            cancellation.cancel();
        });
        let report = driver
            .run_prompt("partial request", 128)
            .await
            .expect("interrupted report");
        turn_id = report.receipt.turn_id.clone();
        assert_eq!(exit_for_report(&report), ExitClass::Interrupted);
        assert_eq!(report.receipt.outcome, RuntimeTerminalOutcome::Interrupted);
    }

    let store = RuntimeStore::open(project.path()).expect("lease released after shutdown");
    let session = store
        .machine()
        .sessions()
        .get(&session_id)
        .expect("recovered session");
    let matching = session
        .turns
        .iter()
        .filter(|turn| turn.turn_id == turn_id)
        .collect::<Vec<_>>();
    assert_eq!(matching.len(), 1);
    assert_eq!(matching[0].status, TurnStatus::Interrupted);
    assert_eq!(
        matching[0]
            .receipt
            .as_ref()
            .expect("one durable receipt")
            .outcome,
        RuntimeTerminalOutcome::Interrupted
    );
}

#[tokio::test]
async fn restart_returns_to_confirm_and_old_shell_session_id_is_not_reused() {
    let project = tempfile::tempdir().expect("temporary project");
    let old_session_id;
    {
        let mut driver = RuntimeDriver::open_with_agent_ports(
            project.path(),
            binding(),
            MockProvider {
                runs: VecDeque::from([
                    shell_tool_run(
                        "shell_command",
                        serde_json::json!({"command": "long-running"}),
                    ),
                    MockProvider::completed(&["started"]),
                ]),
            },
            DriverIds::new("shell-process-one", 40_000),
            Box::new(HeadlessApprovalPort),
            Box::new(ProcessScopedShellPort::default()),
            full_tool_definitions(),
            AgentLimits::default(),
        )
        .expect("first process");
        driver
            .set_permission_mode(PermissionMode::FullAccess)
            .await
            .expect("enable Shell");
        let report = driver
            .run_agent("start", 128)
            .await
            .expect("start Shell session");
        let receipt: ShellReceipt = serde_json::from_str(
            report.tool_results[0]
                .output
                .as_deref()
                .expect("running receipt"),
        )
        .expect("running receipt JSON");
        old_session_id = receipt.session_id;
        driver
            .shutdown_tools()
            .await
            .expect("first process shutdown");
    }

    let mut restarted = RuntimeDriver::open_with_agent_ports(
        project.path(),
        binding(),
        MockProvider {
            runs: VecDeque::from([
                shell_tool_run(
                    "shell_session",
                    serde_json::json!({
                        "session_id": old_session_id.as_str(),
                        "action": "poll"
                    }),
                ),
                MockProvider::completed(&["not found"]),
            ]),
        },
        DriverIds::new("shell-process-two", 41_000),
        Box::new(HeadlessApprovalPort),
        Box::new(ProcessScopedShellPort::default()),
        full_tool_definitions(),
        AgentLimits::default(),
    )
    .expect("restarted process");
    assert_eq!(restarted.permission_mode(), PermissionMode::Confirm);
    restarted
        .set_permission_mode(PermissionMode::FullAccess)
        .await
        .expect("enable new process Shell");
    let report = restarted
        .run_agent("poll old session", 128)
        .await
        .expect("terminal old session result");
    assert_eq!(report.tool_results[0].status, ToolTerminalStatus::Rejected);
    assert_eq!(report.tool_results[0].code, "shell_session_not_found");
}

fn binding() -> ModelBinding {
    ModelBinding {
        provider_id: ProviderId::new("fixture").expect("provider id"),
        model_id: ModelId::new("fixture-model").expect("model id"),
        protocol: ProviderProtocolKind::Responses,
    }
}
