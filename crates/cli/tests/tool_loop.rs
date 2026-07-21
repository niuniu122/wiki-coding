use std::collections::VecDeque;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use minimax_cli::{
    DriverError, DriverIds, HeadlessApprovalPort, InteractiveApprovalPort, ProviderPort,
    RuntimeDriver,
};
use minimax_core::{
    ApprovalFuture, ApprovalPort, CancellationPort, PermissionMode, ToolExecutionContext,
    ToolFuture, ToolLifecycleError, ToolLifecycleFuture, ToolPort, ToolSandboxPolicy,
};
use minimax_protocol::{
    AgentLimits, ConversationItem, FULL_ACCESS_TOOL_NAMES, JournalRecord, ModelBinding, ModelId,
    ProviderId, ProviderProtocolKind, RuntimeErrorCode, RuntimeFailure, SHELL_TOOL_NAMES,
    SchemaVersion, ShellReceipt, ShellSessionId, ShellSessionState, StreamEvent, TerminalOutcome,
    ToolCall, ToolCallFragment, ToolCallId, ToolDecision, ToolDecisionKind, ToolDefinition,
    ToolEffect, ToolInvocation, ToolResult, ToolTerminalStatus, TraceCode, TurnRequest,
    V1_TOOL_NAMES,
};
use minimax_tools::BuiltinToolPort;
use minimax_tui::ApprovalInput;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
struct ScriptRound {
    events: Vec<StreamEvent>,
    required_terminal_records: usize,
}

struct ScriptedProvider {
    rounds: VecDeque<ScriptRound>,
    requests: Arc<Mutex<Vec<TurnRequest>>>,
    journal_path: PathBuf,
}

impl ProviderPort for ScriptedProvider {
    fn rebind(&mut self, _binding: &ModelBinding) {}

    fn stream<'a>(
        &'a mut self,
        request: &'a TurnRequest,
        _cancellation: &'a CancellationToken,
        emit: &'a mut (dyn FnMut(StreamEvent) + Send),
    ) -> Pin<Box<dyn Future<Output = Result<(), RuntimeFailure>> + Send + 'a>> {
        Box::pin(async move {
            self.requests
                .lock()
                .expect("request capture")
                .push(request.clone());
            let round = self
                .rounds
                .pop_front()
                .ok_or_else(|| RuntimeFailure::new(RuntimeErrorCode::ProtocolPrematureEof))?;
            if round.required_terminal_records > 0 {
                let journal = std::fs::read_to_string(&self.journal_path)
                    .expect("journal before next provider round");
                assert_eq!(
                    journal.matches(r#""type":"tool_terminal""#).count(),
                    round.required_terminal_records,
                    "every tool result must be durable before the next Provider request"
                );
            }
            for event in round.events {
                emit(event);
                tokio::task::yield_now().await;
            }
            Ok(())
        })
    }
}

struct ApprovalSpy {
    decisions: Mutex<VecDeque<ToolDecisionKind>>,
    calls: Arc<Mutex<Vec<ToolCallId>>>,
    unavailable: bool,
}

impl ApprovalPort for ApprovalSpy {
    fn decide<'a>(&'a self, invocation: &'a ToolInvocation) -> ApprovalFuture<'a> {
        Box::pin(async move {
            self.calls
                .lock()
                .expect("approval calls")
                .push(invocation.call.call_id.clone());
            let decision = if self.unavailable {
                ToolDecisionKind::Rejected
            } else {
                self.decisions
                    .lock()
                    .expect("approval decisions")
                    .pop_front()
                    .expect("scripted decision")
            };
            ToolDecision {
                schema_version: SchemaVersion,
                call_id: invocation.call.call_id.clone(),
                decision,
                code: if self.unavailable {
                    "approval_unavailable"
                } else if decision == ToolDecisionKind::Approved {
                    "approved"
                } else {
                    "rejected"
                }
                .to_owned(),
            }
        })
    }
}

struct ToolSpy {
    preflight_calls: Arc<Mutex<Vec<(ToolCallId, ToolExecutionContext)>>>,
    execute_calls: Arc<Mutex<Vec<(ToolCallId, ToolExecutionContext)>>>,
    deny_preflight: bool,
}

struct CancellingApproval {
    cancellation: Arc<Mutex<Option<CancellationToken>>>,
    calls: Arc<Mutex<Vec<ToolCallId>>>,
}

impl ApprovalPort for CancellingApproval {
    fn decide<'a>(&'a self, invocation: &'a ToolInvocation) -> ApprovalFuture<'a> {
        Box::pin(async move {
            self.calls
                .lock()
                .expect("approval calls")
                .push(invocation.call.call_id.clone());
            self.cancellation
                .lock()
                .expect("cancellation")
                .as_ref()
                .expect("driver cancellation token")
                .cancel();
            std::future::pending::<ToolDecision>().await
        })
    }
}

struct CancellingTool {
    cancellation: Arc<Mutex<Option<CancellationToken>>>,
    execute_calls: Arc<Mutex<Vec<(ToolCallId, ToolExecutionContext)>>>,
}

impl ToolPort for CancellingTool {
    fn preflight(
        &self,
        _invocation: &ToolInvocation,
        _context: ToolExecutionContext,
        _cancellation: &dyn CancellationPort,
    ) -> Result<(), ToolResult> {
        Ok(())
    }

    fn execute<'a>(
        &'a self,
        invocation: &'a ToolInvocation,
        context: ToolExecutionContext,
        cancellation: &'a dyn CancellationPort,
    ) -> ToolFuture<'a> {
        Box::pin(async move {
            self.execute_calls
                .lock()
                .expect("execute calls")
                .push((invocation.call.call_id.clone(), context));
            self.cancellation
                .lock()
                .expect("cancellation")
                .as_ref()
                .expect("driver cancellation token")
                .cancel();
            cancellation.cancelled().await;
            result_for(
                invocation,
                ToolTerminalStatus::Indeterminate,
                "effect_unknown",
                None,
            )
        })
    }
}

impl ToolPort for ToolSpy {
    fn preflight(
        &self,
        invocation: &ToolInvocation,
        context: ToolExecutionContext,
        _cancellation: &dyn CancellationPort,
    ) -> Result<(), ToolResult> {
        self.preflight_calls
            .lock()
            .expect("preflight calls")
            .push((invocation.call.call_id.clone(), context));
        if self.deny_preflight {
            Err(result_for(
                invocation,
                ToolTerminalStatus::Failed,
                "preflight_denied",
                None,
            ))
        } else {
            Ok(())
        }
    }

    fn execute<'a>(
        &'a self,
        invocation: &'a ToolInvocation,
        context: ToolExecutionContext,
        _cancellation: &'a dyn CancellationPort,
    ) -> ToolFuture<'a> {
        Box::pin(async move {
            self.execute_calls
                .lock()
                .expect("execute calls")
                .push((invocation.call.call_id.clone(), context));
            result_for(
                invocation,
                ToolTerminalStatus::Succeeded,
                "ok",
                Some(format!("contents-for-{}", invocation.call.call_id.as_str())),
            )
        })
    }
}

#[derive(Default)]
struct FakeShellState {
    accepting: bool,
    transitions: Vec<PermissionMode>,
    shutdown_calls: usize,
    spawn_calls: usize,
    sessions: Vec<String>,
    cleanup_failure_ids: Vec<String>,
}

#[derive(Clone, Default)]
struct FakeShellPort {
    state: Arc<Mutex<FakeShellState>>,
}

impl FakeShellPort {
    fn spawn_count(&self) -> usize {
        self.state.lock().expect("fake shell state").spawn_calls
    }

    fn transitions(&self) -> Vec<PermissionMode> {
        self.state
            .lock()
            .expect("fake shell state")
            .transitions
            .clone()
    }

    fn shutdown_count(&self) -> usize {
        self.state.lock().expect("fake shell state").shutdown_calls
    }

    fn fail_cleanup_for(&self, session_ids: &[&str]) {
        self.state
            .lock()
            .expect("fake shell state")
            .cleanup_failure_ids = session_ids.iter().map(|id| (*id).to_owned()).collect();
    }
}

impl ToolPort for FakeShellPort {
    fn preflight(
        &self,
        invocation: &ToolInvocation,
        context: ToolExecutionContext,
        _cancellation: &dyn CancellationPort,
    ) -> Result<(), ToolResult> {
        if SHELL_TOOL_NAMES.contains(&invocation.call.name.as_str()) {
            let accepting = self.state.lock().expect("fake shell state").accepting;
            if context.permission_mode() != PermissionMode::FullAccess || !accepting {
                return Err(result_for(
                    invocation,
                    ToolTerminalStatus::Rejected,
                    "shell_requires_full_access",
                    None,
                ));
            }
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
                    let mut state = self.state.lock().expect("fake shell state");
                    if !state.accepting {
                        return result_for(
                            invocation,
                            ToolTerminalStatus::Rejected,
                            "shell_requires_full_access",
                            None,
                        );
                    }
                    state.spawn_calls += 1;
                    let session_id = format!("shell-fake-{:04}", state.spawn_calls);
                    state.sessions.push(session_id.clone());
                    let receipt = ShellReceipt::new(
                        ShellSessionId::new(session_id).expect("fake session id"),
                        ShellSessionState::Running,
                        None,
                        "DO_NOT_PERSIST_OUTPUT sk-output-secret".to_owned(),
                        false,
                    )
                    .expect("fake shell receipt");
                    result_for(
                        invocation,
                        ToolTerminalStatus::Succeeded,
                        "shell_running",
                        Some(serde_json::to_string(&receipt).expect("receipt JSON")),
                    )
                }
                "shell_session" => {
                    let arguments: serde_json::Value =
                        serde_json::from_str(&invocation.call.arguments_json)
                            .expect("shell session arguments");
                    let session_id = arguments["session_id"].as_str().unwrap_or_default();
                    let state = self.state.lock().expect("fake shell state");
                    if !state.sessions.iter().any(|known| known == session_id) {
                        return result_for(
                            invocation,
                            ToolTerminalStatus::Rejected,
                            "shell_session_not_found",
                            None,
                        );
                    }
                    let receipt = ShellReceipt::new(
                        ShellSessionId::new(session_id).expect("known session id"),
                        ShellSessionState::Running,
                        None,
                        String::new(),
                        false,
                    )
                    .expect("fake shell receipt");
                    result_for(
                        invocation,
                        ToolTerminalStatus::Succeeded,
                        "shell_running",
                        Some(serde_json::to_string(&receipt).expect("receipt JSON")),
                    )
                }
                _ => result_for(invocation, ToolTerminalStatus::Succeeded, "ok", None),
            }
        })
    }

    fn transition_permission<'a>(&'a self, mode: PermissionMode) -> ToolLifecycleFuture<'a> {
        Box::pin(async move {
            let mut state = self.state.lock().expect("fake shell state");
            state.transitions.push(mode);
            state.accepting = mode == PermissionMode::FullAccess;
            if mode == PermissionMode::Confirm && !state.cleanup_failure_ids.is_empty() {
                return Err(ToolLifecycleError {
                    code: "shell_stop_indeterminate",
                    session_ids: state.cleanup_failure_ids.clone(),
                });
            }
            if mode == PermissionMode::Confirm {
                state.sessions.clear();
            }
            Ok(())
        })
    }

    fn shutdown<'a>(&'a self) -> ToolLifecycleFuture<'a> {
        Box::pin(async move {
            let mut state = self.state.lock().expect("fake shell state");
            state.shutdown_calls += 1;
            state.accepting = false;
            state.sessions.clear();
            Ok(())
        })
    }
}

fn result_for(
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

fn binding(protocol: ProviderProtocolKind) -> ModelBinding {
    ModelBinding {
        provider_id: ProviderId::new("fixture").expect("provider"),
        model_id: ModelId::new("fixture-model").expect("model"),
        protocol,
    }
}

fn definition() -> ToolDefinition {
    ToolDefinition::new(
        "read_file",
        "Read one bounded local file",
        serde_json::json!({
            "type": "object",
            "properties": { "path": { "type": "string" } },
            "required": ["path"],
            "additionalProperties": false
        }),
    )
    .expect("definition")
}

fn tool_round(calls: &[(&str, &str)]) -> ScriptRound {
    ScriptRound {
        events: vec![
            StreamEvent::ToolCallFragments {
                fragments: calls
                    .iter()
                    .enumerate()
                    .map(|(index, (call_id, path))| ToolCallFragment {
                        call_id: ToolCallId::new(*call_id).expect("call"),
                        stream_id: Some(format!("stream-{index}")),
                        name: Some("read_file".to_owned()),
                        arguments_delta: Some(format!(r#"{{"path":"{path}"}}"#)),
                        arguments_complete: true,
                        index: Some(u32::try_from(index).expect("index")),
                    })
                    .collect(),
            },
            StreamEvent::Terminal {
                outcome: TerminalOutcome::Completed,
            },
        ],
        required_terminal_records: 0,
    }
}

fn shell_command_round(call_id: &str, command: &str, cwd: &str) -> ScriptRound {
    ScriptRound {
        events: vec![
            StreamEvent::ToolCallFragments {
                fragments: vec![ToolCallFragment {
                    call_id: ToolCallId::new(call_id).expect("call"),
                    stream_id: Some(format!("stream-{call_id}")),
                    name: Some("shell_command".to_owned()),
                    arguments_delta: Some(
                        serde_json::json!({"command": command, "cwd": cwd}).to_string(),
                    ),
                    arguments_complete: true,
                    index: Some(0),
                }],
            },
            StreamEvent::Terminal {
                outcome: TerminalOutcome::Completed,
            },
        ],
        required_terminal_records: 0,
    }
}

fn tool_names(request: &TurnRequest) -> Vec<&str> {
    request
        .tools
        .iter()
        .map(|tool| tool.name.as_str())
        .collect()
}

fn all_tool_definitions() -> Vec<ToolDefinition> {
    BuiltinToolPort::definitions_for(PermissionMode::FullAccess).expect("full-access definitions")
}

fn shell_invocation(call_id: &str, name: &str, arguments: serde_json::Value) -> ToolInvocation {
    ToolInvocation::new(
        ToolCall::new(
            ToolCallId::new(call_id).expect("call id"),
            name,
            arguments.to_string(),
        )
        .expect("tool call"),
        ToolEffect::Process,
    )
    .expect("tool invocation")
}

fn final_round(required_terminal_records: usize) -> ScriptRound {
    ScriptRound {
        events: vec![
            StreamEvent::VisibleTextDelta {
                delta: "final answer".to_owned(),
            },
            StreamEvent::Terminal {
                outcome: TerminalOutcome::Completed,
            },
        ],
        required_terminal_records,
    }
}

fn journal_path(root: &Path) -> PathBuf {
    root.join(".minimax/runtime/v1/sessions.jsonl")
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct E2eFixture {
    schema_version: u16,
    cases: Vec<E2eCase>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct E2eCase {
    provider_protocol: String,
    calls: Vec<E2eCall>,
    final_answer: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct E2eCall {
    call_id: String,
    tool: String,
    path: String,
}

fn e2e_fixture() -> E2eFixture {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("repository root");
    let raw = std::fs::read_to_string(root.join("fixtures/compat/tools/e2e.v1.json"))
        .expect("E2E fixture");
    serde_json::from_str(&raw).expect("strict E2E fixture")
}

fn protocol_from_fixture(value: &str) -> ProviderProtocolKind {
    match value {
        "responses" => ProviderProtocolKind::Responses,
        "chat_completions" => ProviderProtocolKind::ChatCompletions,
        _ => panic!("unknown fixture protocol"),
    }
}

fn fixture_tool_round(case: &E2eCase) -> ScriptRound {
    assert_eq!(case.calls.len(), 2);
    assert!(case.calls.iter().all(|call| call.tool == "read_file"));
    let calls = case
        .calls
        .iter()
        .map(|call| (call.call_id.as_str(), call.path.as_str()))
        .collect::<Vec<_>>();
    tool_round(&calls)
}

#[tokio::test]
async fn confirm_mode_preserves_order_ids_and_durability_for_both_provider_protocols() {
    for protocol in [
        ProviderProtocolKind::Responses,
        ProviderProtocolKind::ChatCompletions,
    ] {
        let project = tempfile::tempdir().expect("project");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let approvals = Arc::new(Mutex::new(Vec::new()));
        let preflights = Arc::new(Mutex::new(Vec::new()));
        let executions = Arc::new(Mutex::new(Vec::new()));
        let provider = ScriptedProvider {
            rounds: VecDeque::from([
                tool_round(&[("call-a", "A.md"), ("call-b", "B.md")]),
                final_round(2),
            ]),
            requests: Arc::clone(&requests),
            journal_path: journal_path(project.path()),
        };
        let approval = ApprovalSpy {
            decisions: Mutex::new(VecDeque::from([
                ToolDecisionKind::Rejected,
                ToolDecisionKind::Approved,
            ])),
            calls: Arc::clone(&approvals),
            unavailable: false,
        };
        let tools = ToolSpy {
            preflight_calls: Arc::clone(&preflights),
            execute_calls: Arc::clone(&executions),
            deny_preflight: false,
        };
        let mut driver = RuntimeDriver::open_with_agent_ports(
            project.path(),
            binding(protocol),
            provider,
            DriverIds::new("confirm", 1_000),
            Box::new(approval),
            Box::new(tools),
            vec![definition()],
            AgentLimits::default(),
        )
        .expect("driver");

        let report = driver
            .run_agent("inspect two files", 128)
            .await
            .expect("run");
        assert!(matches!(
            report.receipt.outcome,
            minimax_protocol::RuntimeTerminalOutcome::Completed
        ));
        assert_eq!(
            approvals
                .lock()
                .expect("approvals")
                .iter()
                .map(ToolCallId::as_str)
                .collect::<Vec<_>>(),
            ["call-a", "call-b"]
        );
        assert_eq!(
            preflights
                .lock()
                .expect("preflights")
                .iter()
                .map(|(call_id, _)| call_id.as_str())
                .collect::<Vec<_>>(),
            ["call-a", "call-b"]
        );
        assert_eq!(
            executions
                .lock()
                .expect("executions")
                .iter()
                .map(|(call_id, _)| call_id.as_str())
                .collect::<Vec<_>>(),
            ["call-b"]
        );
        assert_eq!(
            executions
                .lock()
                .expect("executions")
                .iter()
                .map(|(_, context)| context.sandbox_policy())
                .collect::<Vec<_>>(),
            [ToolSandboxPolicy::Restricted]
        );
        assert!(
            preflights
                .lock()
                .expect("preflights")
                .iter()
                .all(|(_, context)| *context
                    == ToolExecutionContext::for_permission_mode(PermissionMode::Confirm))
        );

        let captured = requests.lock().expect("requests");
        assert_eq!(captured.len(), 2);
        assert_eq!(captured[0].protocol, protocol);
        assert_eq!(captured[0].tools.len(), 1);
        let history = &captured[1].messages;
        let ConversationItem::AssistantToolCalls(batch) = &history[history.len() - 3] else {
            panic!("assistant tool-call batch");
        };
        assert_eq!(
            batch
                .tool_calls
                .iter()
                .map(|call| call.call_id.as_str())
                .collect::<Vec<_>>(),
            ["call-a", "call-b"]
        );
        let statuses = history[history.len() - 2..]
            .iter()
            .map(|item| match item {
                ConversationItem::ToolResult(message) => (
                    message.tool_result.call_id.as_str(),
                    message.tool_result.status,
                ),
                _ => panic!("tool result"),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            statuses,
            [
                ("call-a", ToolTerminalStatus::Rejected),
                ("call-b", ToolTerminalStatus::Succeeded)
            ]
        );
        drop(captured);
        drop(driver);

        let journal = std::fs::read_to_string(journal_path(project.path())).expect("journal");
        assert!(!journal.contains(r#""permissionMode""#));
        assert!(!journal.contains(r#""confirm""#));
        let records = journal
            .lines()
            .map(|line| {
                minimax_protocol::parse_session_record_v1(line).expect("valid journal record")
            })
            .collect::<Vec<_>>();
        let position = |predicate: &dyn Fn(&JournalRecord) -> bool| {
            records
                .iter()
                .position(|record| predicate(&record.record))
                .expect("journal boundary")
        };
        let requested = position(&|record| {
            matches!(record, JournalRecord::ToolRequested { invocation, .. }
                if invocation.call.call_id.as_str() == "call-b")
        });
        let decided = position(&|record| {
            matches!(record, JournalRecord::ToolDecisionRecorded { decision, .. }
                if decision.call_id.as_str() == "call-b")
        });
        let started = position(&|record| {
            matches!(record, JournalRecord::ToolStarted { call_id, .. }
                if call_id.as_str() == "call-b")
        });
        let terminal = position(&|record| {
            matches!(record, JournalRecord::ToolTerminal { result, .. }
                if result.call_id.as_str() == "call-b")
        });
        assert!(requested < decided && decided < started && started < terminal);
    }
}

#[tokio::test]
async fn full_access_skips_prompt_but_still_preflights_persists_and_executes() {
    let project = tempfile::tempdir().expect("project");
    let requests = Arc::new(Mutex::new(Vec::new()));
    let approvals = Arc::new(Mutex::new(Vec::new()));
    let preflights = Arc::new(Mutex::new(Vec::new()));
    let executions = Arc::new(Mutex::new(Vec::new()));
    let provider = ScriptedProvider {
        rounds: VecDeque::from([tool_round(&[("call-full", "README.md")]), final_round(1)]),
        requests,
        journal_path: journal_path(project.path()),
    };
    let mut driver = RuntimeDriver::open_with_agent_ports(
        project.path(),
        binding(ProviderProtocolKind::Responses),
        provider,
        DriverIds::new("full", 2_000),
        Box::new(ApprovalSpy {
            decisions: Mutex::new(VecDeque::new()),
            calls: Arc::clone(&approvals),
            unavailable: false,
        }),
        Box::new(ToolSpy {
            preflight_calls: Arc::clone(&preflights),
            execute_calls: Arc::clone(&executions),
            deny_preflight: false,
        }),
        vec![definition()],
        AgentLimits::default(),
    )
    .expect("driver");
    driver
        .set_permission_mode(PermissionMode::FullAccess)
        .await
        .expect("enable full-access");
    driver.run_agent("read", 128).await.expect("run");
    assert!(approvals.lock().expect("approvals").is_empty());
    assert_eq!(preflights.lock().expect("preflights").len(), 1);
    assert_eq!(executions.lock().expect("executions").len(), 1);
    assert_eq!(
        executions
            .lock()
            .expect("executions")
            .iter()
            .map(|(_, context)| context.sandbox_policy())
            .collect::<Vec<_>>(),
        [ToolSandboxPolicy::Disabled]
    );
    assert_eq!(
        preflights.lock().expect("preflights")[0].1,
        executions.lock().expect("executions")[0].1
    );
    drop(driver);

    let journal = std::fs::read_to_string(journal_path(project.path())).expect("journal");
    assert!(!journal.contains(r#""permissionMode""#));
    assert!(!journal.contains("full_access"));
    assert!(journal.contains("policy_approved"));

    let restarted = RuntimeDriver::open_with_agent_ports(
        project.path(),
        binding(ProviderProtocolKind::Responses),
        ScriptedProvider {
            rounds: VecDeque::new(),
            requests: Arc::new(Mutex::new(Vec::new())),
            journal_path: journal_path(project.path()),
        },
        DriverIds::new("restart-confirm", 2_500),
        Box::new(HeadlessApprovalPort),
        Box::new(ToolSpy {
            preflight_calls: Arc::new(Mutex::new(Vec::new())),
            execute_calls: Arc::new(Mutex::new(Vec::new())),
            deny_preflight: false,
        }),
        vec![definition()],
        AgentLimits::default(),
    )
    .expect("restarted driver");
    assert_eq!(restarted.permission_mode(), PermissionMode::Confirm);
    assert_matrix_responsibility(
        "test/permission-service.test.ts",
        "ts-permission-reset-on-restart",
        "full_access_skips_prompt_but_still_preflights_persists_and_executes",
    );
}

#[tokio::test]
async fn provider_definitions_are_permission_filtered_and_forged_confirm_shell_is_rejected() {
    let project = tempfile::tempdir().expect("project");
    let requests = Arc::new(Mutex::new(Vec::new()));
    let approvals = Arc::new(Mutex::new(Vec::new()));
    let tools = FakeShellPort::default();
    let provider = ScriptedProvider {
        rounds: VecDeque::from([
            shell_command_round(
                "call-forged-shell",
                "echo DO_NOT_PERSIST_COMMAND sk-command-secret",
                "C:\\DO_NOT_PERSIST_CWD",
            ),
            final_round(1),
        ]),
        requests: Arc::clone(&requests),
        journal_path: journal_path(project.path()),
    };
    let mut driver = RuntimeDriver::open_with_agent_ports(
        project.path(),
        binding(ProviderProtocolKind::Responses),
        provider,
        DriverIds::new("forged-shell", 2_600),
        Box::new(ApprovalSpy {
            decisions: Mutex::new(VecDeque::new()),
            calls: Arc::clone(&approvals),
            unavailable: false,
        }),
        Box::new(tools.clone()),
        all_tool_definitions(),
        AgentLimits::default(),
    )
    .expect("driver");

    let report = driver
        .run_agent("forged shell", 128)
        .await
        .expect("terminal run");
    assert_eq!(report.tool_results.len(), 1);
    assert_eq!(report.tool_results[0].code, "shell_requires_full_access");
    assert_eq!(report.tool_results[0].status, ToolTerminalStatus::Rejected);
    assert!(approvals.lock().expect("approval calls").is_empty());
    assert_eq!(tools.spawn_count(), 0);

    let captured = requests.lock().expect("provider requests");
    assert_eq!(captured.len(), 2);
    assert_eq!(tool_names(&captured[0]), V1_TOOL_NAMES);
    assert_eq!(tool_names(&captured[1]), V1_TOOL_NAMES);
    let session = driver.session(&report.receipt.session_id).expect("session");
    assert_eq!(
        session.turns[0].tool_invocations[0]
            .terminal_result
            .as_ref()
            .expect("durable forged result")
            .code,
        "shell_requires_full_access"
    );
}

#[tokio::test]
async fn full_access_shell_lifecycle_is_idempotent_and_trace_is_metadata_only() {
    let project = tempfile::tempdir().expect("project");
    let requests = Arc::new(Mutex::new(Vec::new()));
    let approvals = Arc::new(Mutex::new(Vec::new()));
    let tools = FakeShellPort::default();
    let provider = ScriptedProvider {
        rounds: VecDeque::from([
            shell_command_round(
                "call-running-shell",
                "echo DO_NOT_PERSIST_COMMAND sk-command-secret",
                "C:\\DO_NOT_PERSIST_CWD",
            ),
            final_round(1),
        ]),
        requests: Arc::clone(&requests),
        journal_path: journal_path(project.path()),
    };
    let mut driver = RuntimeDriver::open_with_agent_ports(
        project.path(),
        binding(ProviderProtocolKind::Responses),
        provider,
        DriverIds::new("running-shell", 2_700),
        Box::new(ApprovalSpy {
            decisions: Mutex::new(VecDeque::new()),
            calls: Arc::clone(&approvals),
            unavailable: false,
        }),
        Box::new(tools.clone()),
        all_tool_definitions(),
        AgentLimits::default(),
    )
    .expect("driver");

    driver
        .set_permission_mode(PermissionMode::FullAccess)
        .await
        .expect("enable full-access");
    driver
        .set_permission_mode(PermissionMode::FullAccess)
        .await
        .expect("repeat full-access");
    assert_eq!(tools.transitions(), [PermissionMode::FullAccess]);

    let report = driver
        .run_agent("start persistent shell", 128)
        .await
        .expect("agent run");
    assert!(approvals.lock().expect("approval calls").is_empty());
    assert_eq!(tools.spawn_count(), 1);
    assert_eq!(report.tool_results[0].code, "shell_running");
    {
        let captured = requests.lock().expect("provider requests");
        assert_eq!(tool_names(&captured[0]), FULL_ACCESS_TOOL_NAMES);
        assert_eq!(tool_names(&captured[1]), FULL_ACCESS_TOOL_NAMES);
    }

    let trace = driver
        .active_trace_entries()
        .into_iter()
        .find(|entry| entry.code == TraceCode::ToolCompleted)
        .expect("Shell completion trace");
    assert_eq!(
        trace.facts.keys().map(String::as_str).collect::<Vec<_>>(),
        [
            "elapsed_ms",
            "exit_code",
            "output_bytes",
            "session_id",
            "state",
            "tool",
            "truncated",
        ]
    );
    assert_eq!(trace.facts["tool"], "shell_command");
    assert_eq!(trace.facts["state"], "running");
    let serialized = serde_json::to_string(&trace).expect("trace JSON");
    for prohibited in [
        "DO_NOT_PERSIST_COMMAND",
        "DO_NOT_PERSIST_CWD",
        "DO_NOT_PERSIST_OUTPUT",
        "sk-command-secret",
        "sk-output-secret",
    ] {
        assert!(
            !serialized.contains(prohibited),
            "trace leaked {prohibited}"
        );
    }

    driver
        .set_permission_mode(PermissionMode::Confirm)
        .await
        .expect("disable Shell");
    driver
        .set_permission_mode(PermissionMode::Confirm)
        .await
        .expect("repeat confirm");
    assert_eq!(driver.permission_mode(), PermissionMode::Confirm);
    assert_eq!(
        tools.transitions(),
        [PermissionMode::FullAccess, PermissionMode::Confirm]
    );
    let new_work = shell_invocation(
        "call-after-downgrade",
        "shell_command",
        serde_json::json!({"command": "echo blocked"}),
    );
    let denial = tools
        .preflight(
            &new_work,
            ToolExecutionContext::for_permission_mode(PermissionMode::FullAccess),
            &minimax_tools::NeverCancelled,
        )
        .expect_err("draining port rejects new Shell work");
    assert_eq!(denial.code, "shell_requires_full_access");

    driver.shutdown_tools().await.expect("tool shutdown");
    assert_eq!(tools.shutdown_count(), 1);
}

#[tokio::test]
async fn permission_cleanup_failure_stays_confirm_and_reports_exact_session_ids() {
    let project = tempfile::tempdir().expect("project");
    let tools = FakeShellPort::default();
    let mut driver = RuntimeDriver::open_with_agent_ports(
        project.path(),
        binding(ProviderProtocolKind::Responses),
        ScriptedProvider {
            rounds: VecDeque::new(),
            requests: Arc::new(Mutex::new(Vec::new())),
            journal_path: journal_path(project.path()),
        },
        DriverIds::new("cleanup-failure", 2_800),
        Box::new(HeadlessApprovalPort),
        Box::new(tools.clone()),
        all_tool_definitions(),
        AgentLimits::default(),
    )
    .expect("driver");
    driver
        .set_permission_mode(PermissionMode::FullAccess)
        .await
        .expect("enable full-access");
    tools.fail_cleanup_for(&["shell-failed-0001", "shell-failed-0002"]);

    let error = driver
        .set_permission_mode(PermissionMode::Confirm)
        .await
        .expect_err("cleanup must report indeterminate sessions");
    assert_eq!(driver.permission_mode(), PermissionMode::Confirm);
    assert_eq!(
        error,
        DriverError::ToolLifecycle(ToolLifecycleError {
            code: "shell_stop_indeterminate",
            session_ids: vec![
                "shell-failed-0001".to_owned(),
                "shell-failed-0002".to_owned(),
            ],
        })
    );
    assert_eq!(
        error.to_string(),
        "tool lifecycle error: shell_stop_indeterminate: shell-failed-0001,shell-failed-0002"
    );
    assert_eq!(
        tools.transitions(),
        [PermissionMode::FullAccess, PermissionMode::Confirm]
    );
}

#[tokio::test]
async fn unavailable_approval_returns_rejection_and_invokes_zero_tools() {
    let project = tempfile::tempdir().expect("project");
    let requests = Arc::new(Mutex::new(Vec::new()));
    let approvals = Arc::new(Mutex::new(Vec::new()));
    let executions = Arc::new(Mutex::new(Vec::new()));
    let provider = ScriptedProvider {
        rounds: VecDeque::from([
            tool_round(&[("call-unavailable", "README.md")]),
            final_round(1),
        ]),
        requests: Arc::clone(&requests),
        journal_path: journal_path(project.path()),
    };
    let mut driver = RuntimeDriver::open_with_agent_ports(
        project.path(),
        binding(ProviderProtocolKind::ChatCompletions),
        provider,
        DriverIds::new("unavailable", 3_000),
        Box::new(ApprovalSpy {
            decisions: Mutex::new(VecDeque::new()),
            calls: Arc::clone(&approvals),
            unavailable: true,
        }),
        Box::new(ToolSpy {
            preflight_calls: Arc::new(Mutex::new(Vec::new())),
            execute_calls: Arc::clone(&executions),
            deny_preflight: false,
        }),
        vec![definition()],
        AgentLimits::default(),
    )
    .expect("driver");
    driver.run_agent("read", 128).await.expect("run");
    assert_eq!(approvals.lock().expect("approvals").len(), 1);
    assert!(executions.lock().expect("executions").is_empty());
    let captured = requests.lock().expect("requests");
    let ConversationItem::ToolResult(message) = captured[1].messages.last().expect("result") else {
        panic!("tool result");
    };
    assert_eq!(message.tool_result.status, ToolTerminalStatus::Rejected);
    assert_eq!(message.tool_result.code, "approval_unavailable");
}

#[tokio::test]
async fn full_access_cannot_bypass_a_preflight_denial() {
    let project = tempfile::tempdir().expect("project");
    let approvals = Arc::new(Mutex::new(Vec::new()));
    let executions = Arc::new(Mutex::new(Vec::new()));
    let requests = Arc::new(Mutex::new(Vec::new()));
    let provider = ScriptedProvider {
        rounds: VecDeque::from([
            tool_round(&[("call-denied", "outside.txt")]),
            final_round(1),
        ]),
        requests: Arc::clone(&requests),
        journal_path: journal_path(project.path()),
    };
    let mut driver = RuntimeDriver::open_with_agent_ports(
        project.path(),
        binding(ProviderProtocolKind::Responses),
        provider,
        DriverIds::new("denied", 4_000),
        Box::new(ApprovalSpy {
            decisions: Mutex::new(VecDeque::new()),
            calls: Arc::clone(&approvals),
            unavailable: false,
        }),
        Box::new(ToolSpy {
            preflight_calls: Arc::new(Mutex::new(Vec::new())),
            execute_calls: Arc::clone(&executions),
            deny_preflight: true,
        }),
        vec![definition()],
        AgentLimits::default(),
    )
    .expect("driver");
    driver
        .set_permission_mode(PermissionMode::FullAccess)
        .await
        .expect("enable full-access");
    driver.run_agent("read", 128).await.expect("run");
    assert!(approvals.lock().expect("approvals").is_empty());
    assert!(executions.lock().expect("executions").is_empty());
    let captured = requests.lock().expect("requests");
    let ConversationItem::ToolResult(message) = captured[1].messages.last().expect("result") else {
        panic!("tool result");
    };
    assert_eq!(message.tool_result.status, ToolTerminalStatus::Failed);
    assert_eq!(message.tool_result.code, "preflight_denied");
}

#[tokio::test]
async fn provider_round_budget_exhaustion_is_one_durable_terminal_failure() {
    let project = tempfile::tempdir().expect("project");
    let requests = Arc::new(Mutex::new(Vec::new()));
    let provider = ScriptedProvider {
        rounds: VecDeque::from([tool_round(&[("call-budget", "README.md")])]),
        requests: Arc::clone(&requests),
        journal_path: journal_path(project.path()),
    };
    let mut driver = RuntimeDriver::open_with_agent_ports(
        project.path(),
        binding(ProviderProtocolKind::Responses),
        provider,
        DriverIds::new("budget", 5_000),
        Box::new(ApprovalSpy {
            decisions: Mutex::new(VecDeque::new()),
            calls: Arc::new(Mutex::new(Vec::new())),
            unavailable: false,
        }),
        Box::new(ToolSpy {
            preflight_calls: Arc::new(Mutex::new(Vec::new())),
            execute_calls: Arc::new(Mutex::new(Vec::new())),
            deny_preflight: false,
        }),
        vec![definition()],
        AgentLimits {
            max_provider_rounds: 1,
            ..AgentLimits::default()
        },
    )
    .expect("driver");
    driver
        .set_permission_mode(PermissionMode::FullAccess)
        .await
        .expect("enable full-access");
    let report = driver
        .run_agent("read", 128)
        .await
        .expect("terminal report");
    assert_eq!(requests.lock().expect("requests").len(), 1);
    assert_eq!(
        report.receipt.outcome,
        minimax_protocol::RuntimeTerminalOutcome::Failed {
            failure: RuntimeFailure::new(RuntimeErrorCode::AgentBudgetExhausted)
        }
    );
    let session = driver
        .session(&report.receipt.session_id)
        .expect("durable session");
    assert_eq!(
        session.turns.last().expect("turn").status,
        minimax_protocol::TurnStatus::Failed
    );
}

#[tokio::test]
async fn cancellation_during_confirmation_persists_cancelled_and_executes_nothing() {
    let project = tempfile::tempdir().expect("project");
    let requests = Arc::new(Mutex::new(Vec::new()));
    let approval_calls = Arc::new(Mutex::new(Vec::new()));
    let executions = Arc::new(Mutex::new(Vec::new()));
    let cancellation = Arc::new(Mutex::new(None));
    let provider = ScriptedProvider {
        rounds: VecDeque::from([tool_round(&[("call-cancel", "README.md")])]),
        requests: Arc::clone(&requests),
        journal_path: journal_path(project.path()),
    };
    let mut driver = RuntimeDriver::open_with_agent_ports(
        project.path(),
        binding(ProviderProtocolKind::Responses),
        provider,
        DriverIds::new("cancel-confirm", 6_000),
        Box::new(CancellingApproval {
            cancellation: Arc::clone(&cancellation),
            calls: Arc::clone(&approval_calls),
        }),
        Box::new(ToolSpy {
            preflight_calls: Arc::new(Mutex::new(Vec::new())),
            execute_calls: Arc::clone(&executions),
            deny_preflight: false,
        }),
        vec![definition()],
        AgentLimits::default(),
    )
    .expect("driver");
    *cancellation.lock().expect("cancellation") = Some(driver.cancellation_token());

    let report = driver
        .run_agent("read", 128)
        .await
        .expect("interrupted run");
    assert_eq!(
        report.receipt.outcome,
        minimax_protocol::RuntimeTerminalOutcome::Interrupted
    );
    assert_eq!(requests.lock().expect("requests").len(), 1);
    assert_eq!(approval_calls.lock().expect("approvals").len(), 1);
    assert!(executions.lock().expect("executions").is_empty());
    let session = driver.session(&report.receipt.session_id).expect("session");
    assert_eq!(
        session.turns[0].tool_invocations[0]
            .terminal_result
            .as_ref()
            .map(|result| result.status),
        Some(ToolTerminalStatus::Cancelled)
    );
}

#[tokio::test]
async fn cancellation_after_started_persists_indeterminate_and_never_reexecutes() {
    let project = tempfile::tempdir().expect("project");
    let requests = Arc::new(Mutex::new(Vec::new()));
    let executions = Arc::new(Mutex::new(Vec::new()));
    let cancellation = Arc::new(Mutex::new(None));
    let provider = ScriptedProvider {
        rounds: VecDeque::from([tool_round(&[("call-started-cancel", "README.md")])]),
        requests: Arc::clone(&requests),
        journal_path: journal_path(project.path()),
    };
    let mut driver = RuntimeDriver::open_with_agent_ports(
        project.path(),
        binding(ProviderProtocolKind::Responses),
        provider,
        DriverIds::new("cancel-started", 7_000),
        Box::new(ApprovalSpy {
            decisions: Mutex::new(VecDeque::new()),
            calls: Arc::new(Mutex::new(Vec::new())),
            unavailable: false,
        }),
        Box::new(CancellingTool {
            cancellation: Arc::clone(&cancellation),
            execute_calls: Arc::clone(&executions),
        }),
        vec![definition()],
        AgentLimits::default(),
    )
    .expect("driver");
    driver
        .set_permission_mode(PermissionMode::FullAccess)
        .await
        .expect("enable full-access");
    *cancellation.lock().expect("cancellation") = Some(driver.cancellation_token());

    let report = driver
        .run_agent("read", 128)
        .await
        .expect("interrupted run");
    assert_eq!(
        report.receipt.outcome,
        minimax_protocol::RuntimeTerminalOutcome::Interrupted
    );
    assert_eq!(requests.lock().expect("requests").len(), 1);
    assert_eq!(executions.lock().expect("executions").len(), 1);
    let session = driver.session(&report.receipt.session_id).expect("session");
    let invocation = &session.turns[0].tool_invocations[0];
    assert!(invocation.started_at_unix_ms.is_some());
    assert_eq!(
        invocation
            .terminal_result
            .as_ref()
            .map(|result| result.status),
        Some(ToolTerminalStatus::Indeterminate)
    );
}

#[derive(Clone, Copy)]
enum ApprovalAnswer {
    Text(&'static str),
    Eof,
    Interrupted,
}

struct FixedApprovalInput {
    interactive: bool,
    answer: ApprovalAnswer,
}

struct QueuedApprovalInput {
    answers: Mutex<VecDeque<&'static str>>,
}

impl ApprovalInput for QueuedApprovalInput {
    fn is_interactive(&self) -> bool {
        true
    }

    fn read_approval(&self) -> std::io::Result<Option<String>> {
        Ok(self
            .answers
            .lock()
            .expect("approval answers")
            .pop_front()
            .map(str::to_owned))
    }
}

impl ApprovalInput for FixedApprovalInput {
    fn is_interactive(&self) -> bool {
        self.interactive
    }

    fn read_approval(&self) -> std::io::Result<Option<String>> {
        match self.answer {
            ApprovalAnswer::Text(answer) => Ok(Some(answer.to_owned())),
            ApprovalAnswer::Eof => Ok(None),
            ApprovalAnswer::Interrupted => Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "fixture interrupt",
            )),
        }
    }
}

fn approval_invocation() -> ToolInvocation {
    ToolInvocation::new(
        ToolCall::new(
            ToolCallId::new("call-approval").expect("call id"),
            "read_file",
            r#"{"path":"README.md"}"#,
        )
        .expect("call"),
        ToolEffect::Read,
    )
    .expect("invocation")
}

#[tokio::test]
async fn interactive_approval_accepts_only_exact_yes_and_never_retries() {
    let cases = [
        (
            true,
            ApprovalAnswer::Text("yes\n"),
            ToolDecisionKind::Approved,
            "user_approved",
        ),
        (
            true,
            ApprovalAnswer::Text("no\n"),
            ToolDecisionKind::Rejected,
            "user_rejected",
        ),
        (
            true,
            ApprovalAnswer::Text(" yes\n"),
            ToolDecisionKind::Rejected,
            "approval_invalid",
        ),
        (
            true,
            ApprovalAnswer::Text("YES\n"),
            ToolDecisionKind::Rejected,
            "approval_invalid",
        ),
        (
            true,
            ApprovalAnswer::Eof,
            ToolDecisionKind::Rejected,
            "approval_eof",
        ),
        (
            true,
            ApprovalAnswer::Interrupted,
            ToolDecisionKind::Rejected,
            "approval_interrupted",
        ),
        (
            false,
            ApprovalAnswer::Text("yes\n"),
            ToolDecisionKind::Rejected,
            "approval_noninteractive",
        ),
    ];
    for (interactive, answer, expected, code) in cases {
        let port = InteractiveApprovalPort::new(Box::new(FixedApprovalInput {
            interactive,
            answer,
        }));
        let decision = port.decide(&approval_invocation()).await;
        assert_eq!(decision.decision, expected);
        assert_eq!(decision.code, code);
        assert_eq!(decision.call_id.as_str(), "call-approval");
    }
}

#[tokio::test]
async fn concrete_builtin_tools_complete_on_both_provider_protocols_in_full_access() {
    let fixture = e2e_fixture();
    assert_eq!(fixture.schema_version, 1);
    assert_eq!(fixture.cases.len(), 2);
    for case in fixture.cases {
        let protocol = protocol_from_fixture(&case.provider_protocol);
        let project = tempfile::tempdir().expect("project");
        std::fs::write(project.path().join("A.md"), "bounded fixture A").expect("fixture A");
        std::fs::write(project.path().join("B.md"), "bounded fixture B").expect("fixture B");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let provider = ScriptedProvider {
            rounds: VecDeque::from([fixture_tool_round(&case), final_round(2)]),
            requests: Arc::clone(&requests),
            journal_path: journal_path(project.path()),
        };
        let mut driver = RuntimeDriver::open_with_builtin_tools(
            project.path(),
            binding(protocol),
            provider,
            DriverIds::new("concrete", 8_000),
            Box::new(HeadlessApprovalPort),
        )
        .expect("builtin tools");
        driver
            .set_permission_mode(PermissionMode::FullAccess)
            .await
            .expect("enable full-access");

        let report = driver
            .run_agent("read fixture", 128)
            .await
            .expect("agent run");
        assert_eq!(report.tool_results.len(), 2);
        assert_eq!(
            report
                .tool_results
                .iter()
                .map(|result| result.call_id.as_str())
                .collect::<Vec<_>>(),
            case.calls
                .iter()
                .map(|call| call.call_id.as_str())
                .collect::<Vec<_>>()
        );
        assert!(report.tool_results.iter().all(|result| {
            result.status == ToolTerminalStatus::Succeeded
                && result.code == "ok"
                && result
                    .output
                    .as_deref()
                    .is_some_and(|output| output.contains("bounded fixture"))
        }));
        assert!(report.events.iter().any(|event| {
            matches!(
                &event.event,
                minimax_protocol::RuntimeEvent::VisibleTextDelta { delta }
                    if delta == &case.final_answer
            )
        }));
        assert_eq!(requests.lock().expect("requests").len(), 2);
    }
}

#[tokio::test]
async fn concrete_confirm_mode_approves_rejects_and_headless_fails_closed() {
    for (answer, expected_status, expected_code) in [
        (Some("yes\n"), ToolTerminalStatus::Succeeded, "ok"),
        (Some("no\n"), ToolTerminalStatus::Rejected, "user_rejected"),
        (None, ToolTerminalStatus::Rejected, "approval_unavailable"),
    ] {
        let project = tempfile::tempdir().expect("project");
        std::fs::write(project.path().join("README.md"), "bounded fixture").expect("fixture file");
        let provider = ScriptedProvider {
            rounds: VecDeque::from([
                tool_round(&[("call-confirm-concrete", "README.md")]),
                final_round(1),
            ]),
            requests: Arc::new(Mutex::new(Vec::new())),
            journal_path: journal_path(project.path()),
        };
        let approval: Box<dyn ApprovalPort> = answer.map_or_else(
            || Box::new(HeadlessApprovalPort) as Box<dyn ApprovalPort>,
            |answer| {
                Box::new(InteractiveApprovalPort::new(Box::new(FixedApprovalInput {
                    interactive: true,
                    answer: ApprovalAnswer::Text(answer),
                }))) as Box<dyn ApprovalPort>
            },
        );
        let mut driver = RuntimeDriver::open_with_builtin_tools(
            project.path(),
            binding(ProviderProtocolKind::Responses),
            provider,
            DriverIds::new("confirm-concrete", 9_000),
            approval,
        )
        .expect("builtin tools");

        let report = driver
            .run_agent("read fixture", 128)
            .await
            .expect("agent run");
        assert_eq!(report.tool_results.len(), 1);
        assert_eq!(report.tool_results[0].status, expected_status);
        assert_eq!(report.tool_results[0].code, expected_code);
    }
}

#[tokio::test]
async fn fixture_confirm_mode_binds_one_answer_to_each_ordered_call() {
    let case = e2e_fixture().cases.remove(0);
    let project = tempfile::tempdir().expect("project");
    std::fs::write(project.path().join("A.md"), "bounded fixture A").expect("fixture A");
    std::fs::write(project.path().join("B.md"), "bounded fixture B").expect("fixture B");
    let provider = ScriptedProvider {
        rounds: VecDeque::from([fixture_tool_round(&case), final_round(2)]),
        requests: Arc::new(Mutex::new(Vec::new())),
        journal_path: journal_path(project.path()),
    };
    let approval = InteractiveApprovalPort::new(Box::new(QueuedApprovalInput {
        answers: Mutex::new(VecDeque::from(["yes\n", "no\n"])),
    }));
    let mut driver = RuntimeDriver::open_with_builtin_tools(
        project.path(),
        binding(protocol_from_fixture(&case.provider_protocol)),
        provider,
        DriverIds::new("confirm-fixture", 10_000),
        Box::new(approval),
    )
    .expect("builtin tools");

    let report = driver.run_agent("read both", 128).await.expect("agent run");
    assert_eq!(report.tool_results.len(), 2);
    assert_eq!(report.tool_results[0].status, ToolTerminalStatus::Succeeded);
    assert_eq!(report.tool_results[1].status, ToolTerminalStatus::Rejected);
    assert_eq!(report.tool_results[1].code, "user_rejected");
    assert_eq!(
        report
            .tool_results
            .iter()
            .map(|result| result.call_id.as_str())
            .collect::<Vec<_>>(),
        case.calls
            .iter()
            .map(|call| call.call_id.as_str())
            .collect::<Vec<_>>()
    );
}

fn assert_matrix_responsibility(source_path: &str, id: &str, test_name: &str) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
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
                        .any(|item| item["path"] == "crates/cli/tests/tool_loop.rs"
                            && item["test"] == test_name)))
    );
}
