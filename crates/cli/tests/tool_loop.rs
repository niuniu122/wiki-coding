use std::collections::VecDeque;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use minimax_cli::{DriverIds, ProviderPort, RuntimeDriver};
use minimax_core::{ApprovalFuture, ApprovalPort, PermissionMode, ToolFuture, ToolPort};
use minimax_protocol::{
    AgentLimits, ConversationItem, JournalRecord, ModelBinding, ModelId, ProviderId,
    ProviderProtocolKind, RuntimeErrorCode, RuntimeFailure, SchemaVersion, StreamEvent,
    TerminalOutcome, ToolCallFragment, ToolCallId, ToolDecision, ToolDecisionKind, ToolDefinition,
    ToolInvocation, ToolResult, ToolTerminalStatus, TurnRequest,
};
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
    preflight_calls: Arc<Mutex<Vec<ToolCallId>>>,
    execute_calls: Arc<Mutex<Vec<ToolCallId>>>,
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
    execute_calls: Arc<Mutex<Vec<ToolCallId>>>,
}

impl ToolPort for CancellingTool {
    fn preflight(&self, _invocation: &ToolInvocation) -> Result<(), ToolResult> {
        Ok(())
    }

    fn execute<'a>(&'a self, invocation: &'a ToolInvocation) -> ToolFuture<'a> {
        Box::pin(async move {
            self.execute_calls
                .lock()
                .expect("execute calls")
                .push(invocation.call.call_id.clone());
            self.cancellation
                .lock()
                .expect("cancellation")
                .as_ref()
                .expect("driver cancellation token")
                .cancel();
            std::future::pending::<ToolResult>().await
        })
    }
}

impl ToolPort for ToolSpy {
    fn preflight(&self, invocation: &ToolInvocation) -> Result<(), ToolResult> {
        self.preflight_calls
            .lock()
            .expect("preflight calls")
            .push(invocation.call.call_id.clone());
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

    fn execute<'a>(&'a self, invocation: &'a ToolInvocation) -> ToolFuture<'a> {
        Box::pin(async move {
            self.execute_calls
                .lock()
                .expect("execute calls")
                .push(invocation.call.call_id.clone());
            result_for(
                invocation,
                ToolTerminalStatus::Succeeded,
                "ok",
                Some(format!("contents-for-{}", invocation.call.call_id.as_str())),
            )
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
                .map(ToolCallId::as_str)
                .collect::<Vec<_>>(),
            ["call-a", "call-b"]
        );
        assert_eq!(
            executions
                .lock()
                .expect("executions")
                .iter()
                .map(ToolCallId::as_str)
                .collect::<Vec<_>>(),
            ["call-b"]
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
    driver.set_permission_mode(PermissionMode::FullAccess);
    driver.run_agent("read", 128).await.expect("run");
    assert!(approvals.lock().expect("approvals").is_empty());
    assert_eq!(preflights.lock().expect("preflights").len(), 1);
    assert_eq!(executions.lock().expect("executions").len(), 1);
    drop(driver);

    let journal = std::fs::read_to_string(journal_path(project.path())).expect("journal");
    assert!(!journal.contains(r#""permissionMode""#));
    assert!(!journal.contains("full_access"));
    assert!(journal.contains("policy_approved"));
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
    driver.set_permission_mode(PermissionMode::FullAccess);
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
    driver.set_permission_mode(PermissionMode::FullAccess);
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
    driver.set_permission_mode(PermissionMode::FullAccess);
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
