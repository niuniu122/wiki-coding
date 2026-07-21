use minimax_core::{
    AgentBudget, BudgetKind, InvocationEffect, InvocationError, InvocationInput, InvocationMachine,
    InvocationRegistry, InvocationState, PermissionMode, SessionCommand, SessionMachine,
    ToolExecutionContext, ToolSandboxPolicy,
};
use minimax_protocol::{
    AgentLimits, ModelBinding, ModelId, ProviderId, ProviderProtocolKind, RecordId, RequestId,
    RuntimeErrorCode, SchemaVersion, SessionId, ToolCall, ToolCallId, ToolDecision,
    ToolDecisionKind, ToolEffect, ToolInvocation, ToolResult, ToolTerminalStatus, TurnId,
};

fn invocation(id: &str) -> ToolInvocation {
    invocation_for(id, "read_file", ToolEffect::Read)
}

fn invocation_for(id: &str, tool_name: &str, effect: ToolEffect) -> ToolInvocation {
    ToolInvocation::new(
        ToolCall::new(
            ToolCallId::new(id).expect("call ID"),
            tool_name,
            r#"{"path":"README.md"}"#,
        )
        .expect("call"),
        effect,
    )
    .expect("invocation")
}

fn decision(id: &str, kind: ToolDecisionKind, code: &str) -> ToolDecision {
    ToolDecision {
        schema_version: SchemaVersion,
        call_id: ToolCallId::new(id).expect("decision ID"),
        decision: kind,
        code: code.to_owned(),
    }
}

fn result(id: &str, status: ToolTerminalStatus, code: &str) -> ToolResult {
    result_for_tool(id, "read_file", status, code)
}

fn result_for_tool(
    id: &str,
    tool_name: &str,
    status: ToolTerminalStatus,
    code: &str,
) -> ToolResult {
    ToolResult {
        schema_version: SchemaVersion,
        call_id: ToolCallId::new(id).expect("result ID"),
        tool_name: tool_name.to_owned(),
        status,
        code: code.to_owned(),
        output: None,
    }
}

fn started_session_machine(
    call_id: &str,
    tool_name: &str,
    effect: ToolEffect,
) -> (SessionMachine, TurnId) {
    let mut machine = SessionMachine::new();
    machine
        .apply(SessionCommand::Create {
            record_id: RecordId::new(format!("record-create-{call_id}")).expect("record"),
            session_id: SessionId::new(format!("session-{call_id}")).expect("session"),
            binding: ModelBinding {
                provider_id: ProviderId::new("provider-test").expect("provider"),
                model_id: ModelId::new("model-test").expect("model"),
                protocol: ProviderProtocolKind::Responses,
            },
            now_unix_ms: 1,
        })
        .expect("create session");
    let turn_id = TurnId::new(format!("turn-{call_id}")).expect("turn");
    machine
        .apply(SessionCommand::Continue {
            record_id: RecordId::new(format!("record-turn-{call_id}")).expect("record"),
            turn_id: turn_id.clone(),
            request_id: RequestId::new(format!("request-{call_id}")).expect("request"),
            user_input: "test".to_owned(),
            max_output_tokens: 128,
            now_unix_ms: 2,
        })
        .expect("continue session");
    machine
        .apply(SessionCommand::RecordToolRequested {
            record_id: RecordId::new(format!("record-request-{call_id}")).expect("record"),
            turn_id: turn_id.clone(),
            invocation: invocation_for(call_id, tool_name, effect),
            now_unix_ms: 3,
        })
        .expect("request tool");
    machine
        .apply(SessionCommand::RecordToolDecision {
            record_id: RecordId::new(format!("record-decision-{call_id}")).expect("record"),
            turn_id: turn_id.clone(),
            decision: decision(call_id, ToolDecisionKind::Approved, "policy_approved"),
            now_unix_ms: 4,
        })
        .expect("approve tool");
    machine
        .apply(SessionCommand::RecordToolStarted {
            record_id: RecordId::new(format!("record-start-{call_id}")).expect("record"),
            turn_id: turn_id.clone(),
            call_id: ToolCallId::new(call_id).expect("call"),
            now_unix_ms: 5,
        })
        .expect("start tool");
    (machine, turn_id)
}

#[test]
fn permission_mode_has_only_confirm_and_full_access_and_defaults_to_confirm() {
    fn label(mode: PermissionMode) -> &'static str {
        match mode {
            PermissionMode::Confirm => "confirm",
            PermissionMode::FullAccess => "full_access",
        }
    }

    assert_eq!(PermissionMode::default(), PermissionMode::Confirm);
    assert_eq!(label(PermissionMode::Confirm), "confirm");
    assert_eq!(label(PermissionMode::FullAccess), "full_access");
}

#[test]
fn execution_context_maps_permission_to_sandbox_once() {
    let confirm = ToolExecutionContext::for_permission_mode(PermissionMode::Confirm);
    assert_eq!(confirm.permission_mode(), PermissionMode::Confirm);
    assert_eq!(confirm.sandbox_policy(), ToolSandboxPolicy::Restricted);

    let full = ToolExecutionContext::for_permission_mode(PermissionMode::FullAccess);
    assert_eq!(full.permission_mode(), PermissionMode::FullAccess);
    assert_eq!(full.sandbox_policy(), ToolSandboxPolicy::Disabled);
}

#[test]
fn approved_invocation_executes_with_the_recorded_permission_snapshot() {
    let (mut machine, _) = InvocationMachine::request(invocation("context-snapshot"));
    let context = ToolExecutionContext::for_permission_mode(PermissionMode::Confirm);
    let effects = machine
        .apply(InvocationInput::PreflightAllowed {
            permission_mode: context.permission_mode(),
        })
        .expect("preflight");
    assert!(matches!(
        effects.as_slice(),
        [InvocationEffect::RequestApproval(_)]
    ));
    let effects = machine
        .apply(InvocationInput::Decision {
            decision: decision("context-snapshot", ToolDecisionKind::Approved, "approved"),
            permission_mode: context.permission_mode(),
        })
        .expect("decision");
    assert!(matches!(
        effects.as_slice(),
        [InvocationEffect::PersistDecision(_)]
    ));
    let effects = machine.apply(InvocationInput::Start).expect("start");
    assert!(matches!(
        effects.as_slice(),
        [
            InvocationEffect::PersistStarted(_),
            InvocationEffect::Execute {
                context: actual,
                ..
            }
        ] if *actual == context
    ));
}

#[test]
fn confirm_requires_one_durable_matching_decision_before_execute() {
    let invocation = invocation("call-1");
    let (mut machine, requested) = InvocationMachine::request(invocation.clone());
    assert_eq!(
        requested,
        vec![InvocationEffect::PersistRequested(invocation.clone())]
    );
    let approval = machine
        .apply(InvocationInput::PreflightAllowed {
            permission_mode: PermissionMode::Confirm,
        })
        .expect("preflight");
    assert_eq!(
        approval,
        vec![InvocationEffect::RequestApproval(invocation.clone())]
    );
    assert_eq!(
        machine.apply(InvocationInput::Start),
        Err(InvocationError::InvalidTransition)
    );
    assert_eq!(
        machine.apply(InvocationInput::Decision {
            decision: decision("other-call", ToolDecisionKind::Approved, "approved"),
            permission_mode: PermissionMode::Confirm,
        }),
        Err(InvocationError::WrongCallId)
    );

    let approved = decision("call-1", ToolDecisionKind::Approved, "approved");
    assert_eq!(
        machine
            .apply(InvocationInput::Decision {
                decision: approved.clone(),
                permission_mode: PermissionMode::Confirm,
            })
            .expect("decision"),
        vec![InvocationEffect::PersistDecision(approved)]
    );
    let started = machine.apply(InvocationInput::Start).expect("start");
    assert!(matches!(
        started.as_slice(),
        [
            InvocationEffect::PersistStarted(_),
            InvocationEffect::Execute { .. }
        ]
    ));
    let succeeded = result("call-1", ToolTerminalStatus::Succeeded, "ok");
    let terminal = machine
        .apply(InvocationInput::Complete {
            result: succeeded.clone(),
        })
        .expect("terminal");
    assert_eq!(
        terminal,
        vec![
            InvocationEffect::PersistTerminal(succeeded.clone()),
            InvocationEffect::PublishTerminal(succeeded),
        ]
    );
    assert_eq!(
        machine.apply(InvocationInput::Complete {
            result: result("call-1", ToolTerminalStatus::Succeeded, "ok"),
        }),
        Err(InvocationError::DuplicateTerminal)
    );
}

#[test]
fn rejection_conflict_and_pre_start_cancellation_never_execute() {
    let cases = [
        (
            InvocationInput::Decision {
                decision: decision("call-1", ToolDecisionKind::Rejected, "user_rejected"),
                permission_mode: PermissionMode::Confirm,
            },
            ToolTerminalStatus::Rejected,
        ),
        (InvocationInput::Cancel, ToolTerminalStatus::Cancelled),
    ];
    for (terminal_input, expected_status) in cases {
        let (mut machine, _) = InvocationMachine::request(invocation("call-1"));
        machine
            .apply(InvocationInput::PreflightAllowed {
                permission_mode: PermissionMode::Confirm,
            })
            .expect("await approval");
        let effects = machine.apply(terminal_input).expect("pre-start terminal");
        assert!(
            effects
                .iter()
                .all(|effect| !matches!(effect, InvocationEffect::Execute { .. }))
        );
        assert!(effects.iter().any(
            |effect| matches!(effect, InvocationEffect::PersistTerminal(result) if result.status == expected_status)
        ));
    }

    let (mut machine, _) = InvocationMachine::request(invocation("call-1"));
    machine
        .apply(InvocationInput::PreflightAllowed {
            permission_mode: PermissionMode::Confirm,
        })
        .expect("await approval");
    machine
        .apply(InvocationInput::Decision {
            decision: decision("call-1", ToolDecisionKind::Approved, "approved"),
            permission_mode: PermissionMode::Confirm,
        })
        .expect("first decision");
    assert_eq!(
        machine.apply(InvocationInput::Decision {
            decision: decision("call-1", ToolDecisionKind::Rejected, "conflict"),
            permission_mode: PermissionMode::Confirm,
        }),
        Err(InvocationError::DuplicateDecision)
    );
}

#[test]
fn full_access_skips_only_prompt_and_both_modes_share_preflight_denial() {
    let denied = result("call-1", ToolTerminalStatus::Failed, "hard_gate_denied");
    let mut denial_effects = Vec::new();
    for mode in [PermissionMode::Confirm, PermissionMode::FullAccess] {
        let (mut machine, _) = InvocationMachine::request(invocation("call-1"));
        let effects = machine
            .apply(InvocationInput::PreflightDenied {
                result: denied.clone(),
            })
            .expect("hard gate denial");
        assert!(
            effects.iter().all(|effect| {
                !matches!(
                    effect,
                    InvocationEffect::RequestApproval(_) | InvocationEffect::Execute { .. }
                )
            }),
            "{mode:?}"
        );
        denial_effects.push(effects);
    }
    assert_eq!(denial_effects[0], denial_effects[1]);

    let (mut machine, _) = InvocationMachine::request(invocation("call-1"));
    let effects = machine
        .apply(InvocationInput::PreflightAllowed {
            permission_mode: PermissionMode::FullAccess,
        })
        .expect("full access decision");
    assert!(matches!(
        effects.as_slice(),
        [InvocationEffect::PersistDecision(decision)]
            if decision.decision == ToolDecisionKind::Approved
    ));
    let InvocationState::Approved { snapshot } = machine.state() else {
        panic!("approved state");
    };
    assert_eq!(snapshot.permission_mode, PermissionMode::FullAccess);
    assert_eq!(
        machine.apply(InvocationInput::Decision {
            decision: decision("call-1", ToolDecisionKind::Rejected, "mode_changed"),
            permission_mode: PermissionMode::Confirm,
        }),
        Err(InvocationError::DuplicateDecision)
    );
    let InvocationState::Approved { snapshot } = machine.state() else {
        panic!("decision remains frozen");
    };
    assert_eq!(snapshot.permission_mode, PermissionMode::FullAccess);
}

#[test]
fn post_start_cancel_or_recovery_is_indeterminate_and_never_reexecutes() {
    for input in [InvocationInput::Cancel, InvocationInput::Recover] {
        let (mut machine, _) = InvocationMachine::request(invocation("call-1"));
        machine
            .apply(InvocationInput::PreflightAllowed {
                permission_mode: PermissionMode::FullAccess,
            })
            .expect("decision");
        let started = machine.apply(InvocationInput::Start).expect("start");
        assert_eq!(
            started
                .iter()
                .filter(|effect| matches!(effect, InvocationEffect::Execute { .. }))
                .count(),
            1
        );
        let recovered = machine.apply(input).expect("honest terminal");
        assert!(recovered.iter().any(
            |effect| matches!(effect, InvocationEffect::PersistTerminal(result)
                if result.status == ToolTerminalStatus::Indeterminate)
        ));
        assert!(
            recovered
                .iter()
                .all(|effect| !matches!(effect, InvocationEffect::Execute { .. }))
        );
        assert!(
            machine
                .apply(InvocationInput::Recover)
                .expect("idempotent recovery")
                .is_empty()
        );
    }
}

#[test]
fn registry_and_all_four_budgets_fail_before_an_extra_effect() {
    let invocation = invocation("call-1");
    let mut registry = InvocationRegistry::default();
    registry.register(&invocation).expect("first ID");
    assert_eq!(
        registry.register(&invocation),
        Err(InvocationError::DuplicateCallId)
    );

    let limits = AgentLimits {
        max_provider_rounds: 1,
        max_tool_calls: 1,
        max_elapsed_ms: 10,
        max_tool_result_bytes: 2,
    };
    let mut budget = AgentBudget::new(limits, 100).expect("budget");
    budget.consume_provider_round(100).expect("first round");
    let rounds = budget
        .consume_provider_round(100)
        .expect_err("round ceiling");
    assert_eq!(rounds.kind, BudgetKind::ProviderRounds);
    budget.consume_tool_call(100).expect("first tool");
    let calls = budget.consume_tool_call(100).expect_err("tool ceiling");
    assert_eq!(calls.kind, BudgetKind::ToolCalls);
    budget.consume_result_bytes(2, 100).expect("first result");
    let bytes = budget
        .consume_result_bytes(1, 100)
        .expect_err("result ceiling");
    assert_eq!(bytes.kind, BudgetKind::ToolResultBytes);
    let elapsed = budget
        .consume_result_bytes(0, 111)
        .expect_err("elapsed ceiling");
    assert_eq!(elapsed.kind, BudgetKind::Elapsed);
    assert_eq!(
        budget.failure_result(&invocation, calls).status,
        ToolTerminalStatus::Failed
    );
}

#[test]
fn a_pre_start_budget_failure_is_terminal_without_started_or_execute_effects() {
    let (mut machine, _) = InvocationMachine::request(invocation("call-budget"));
    machine
        .apply(InvocationInput::PreflightAllowed {
            permission_mode: PermissionMode::FullAccess,
        })
        .expect("approved");
    let failure = result(
        "call-budget",
        ToolTerminalStatus::Failed,
        "tool_call_budget_exhausted",
    );
    let effects = machine
        .apply(InvocationInput::PreStartFailed {
            result: failure.clone(),
        })
        .expect("budget terminal");
    assert_eq!(
        effects,
        vec![
            InvocationEffect::PersistTerminal(failure.clone()),
            InvocationEffect::PublishTerminal(failure),
        ]
    );
    assert!(effects.iter().all(|effect| !matches!(
        effect,
        InvocationEffect::PersistStarted(_) | InvocationEffect::Execute { .. }
    )));
}

#[test]
fn a_late_adapter_rejection_after_start_remains_illegal_for_non_shell_tools() {
    let (mut machine, _) = InvocationMachine::request(invocation("call-late-rejection"));
    machine
        .apply(InvocationInput::PreflightAllowed {
            permission_mode: PermissionMode::FullAccess,
        })
        .expect("policy approval");
    machine.apply(InvocationInput::Start).expect("start");
    let rejected = result(
        "call-late-rejection",
        ToolTerminalStatus::Rejected,
        "shell_session_not_found",
    );

    assert_eq!(
        machine.apply(InvocationInput::Complete { result: rejected }),
        Err(InvocationError::InvalidTerminal)
    );
    assert!(matches!(machine.state(), InvocationState::Started { .. }));
}

const REAL_LATE_SHELL_REJECTIONS: [(&str, &str); 10] = [
    ("shell_command", "invalid_arguments"),
    ("shell_command", "input_limit"),
    ("shell_command", "path_not_found"),
    ("shell_command", "wrong_file_type"),
    ("shell_command", "shell_requires_full_access"),
    ("shell_command", "shell_session_limit"),
    ("shell_session", "invalid_arguments"),
    ("shell_session", "input_limit"),
    ("shell_session", "shell_requires_full_access"),
    ("shell_session", "shell_session_not_found"),
];

const FORGED_LATE_SHELL_REJECTIONS: [(&str, &str); 6] = [
    ("shell_session", "path_not_found"),
    ("shell_session", "wrong_file_type"),
    ("shell_session", "shell_session_limit"),
    ("shell_command", "shell_session_not_found"),
    ("shell_command", "forged_rejection"),
    ("shell_session", "forged_rejection"),
];

#[test]
fn shell_tools_accept_only_real_late_adapter_rejection_codes_after_start() {
    for (index, (tool_name, code)) in REAL_LATE_SHELL_REJECTIONS.into_iter().enumerate() {
        let call_id = format!("call-{tool_name}-{index}");
        let (mut machine, _) =
            InvocationMachine::request(invocation_for(&call_id, tool_name, ToolEffect::Process));
        machine
            .apply(InvocationInput::PreflightAllowed {
                permission_mode: PermissionMode::FullAccess,
            })
            .expect("policy approval");
        machine.apply(InvocationInput::Start).expect("start");
        let rejected = result_for_tool(&call_id, tool_name, ToolTerminalStatus::Rejected, code);

        let effects = machine
            .apply(InvocationInput::Complete {
                result: rejected.clone(),
            })
            .expect("real late Shell rejection");
        assert_eq!(
            effects,
            vec![
                InvocationEffect::PersistTerminal(rejected.clone()),
                InvocationEffect::PublishTerminal(rejected),
            ]
        );
    }
}

#[test]
fn shell_tools_reject_forged_late_adapter_rejection_codes() {
    for (index, (tool_name, code)) in FORGED_LATE_SHELL_REJECTIONS.into_iter().enumerate() {
        let call_id = format!("call-forged-{tool_name}-{index}");
        let (mut machine, _) =
            InvocationMachine::request(invocation_for(&call_id, tool_name, ToolEffect::Process));
        machine
            .apply(InvocationInput::PreflightAllowed {
                permission_mode: PermissionMode::FullAccess,
            })
            .expect("policy approval");
        machine.apply(InvocationInput::Start).expect("start");

        assert_eq!(
            machine.apply(InvocationInput::Complete {
                result: result_for_tool(&call_id, tool_name, ToolTerminalStatus::Rejected, code),
            }),
            Err(InvocationError::InvalidTerminal)
        );
    }
}

#[test]
fn started_shell_cancellation_is_legal_only_for_the_real_cancelled_code() {
    for (index, tool_name) in ["shell_command", "shell_session"].into_iter().enumerate() {
        let call_id = format!("call-cancelled-{tool_name}-{index}");
        let cancelled = result_for_tool(
            &call_id,
            tool_name,
            ToolTerminalStatus::Cancelled,
            "cancelled",
        );
        let (mut invocation_machine, _) =
            InvocationMachine::request(invocation_for(&call_id, tool_name, ToolEffect::Process));
        invocation_machine
            .apply(InvocationInput::PreflightAllowed {
                permission_mode: PermissionMode::FullAccess,
            })
            .expect("policy approval");
        invocation_machine
            .apply(InvocationInput::Start)
            .expect("start");
        invocation_machine
            .apply(InvocationInput::Complete {
                result: cancelled.clone(),
            })
            .expect("runtime projection accepts honest Shell cancellation");

        let (mut session_machine, turn_id) =
            started_session_machine(&call_id, tool_name, ToolEffect::Process);
        session_machine
            .apply(SessionCommand::RecordToolTerminal {
                record_id: RecordId::new(format!("record-terminal-cancelled-{index}"))
                    .expect("record"),
                turn_id,
                result: cancelled,
                now_unix_ms: 6,
            })
            .expect("durable projection accepts honest Shell cancellation");
    }
}

#[test]
fn started_cancellation_rejects_forged_shell_codes_and_every_non_shell_tool() {
    let cases = [
        ("shell_command", ToolEffect::Process, "forged_cancelled"),
        ("shell_session", ToolEffect::Process, "forged_cancelled"),
        ("read_file", ToolEffect::Read, "cancelled"),
        ("list_directory", ToolEffect::Read, "cancelled"),
        ("apply_patch", ToolEffect::Write, "cancelled"),
        ("write_file", ToolEffect::Write, "cancelled"),
        ("run_diagnostic", ToolEffect::Process, "cancelled"),
        ("git_status", ToolEffect::Process, "cancelled"),
        ("git_diff", ToolEffect::Process, "cancelled"),
        ("npm_diagnostic", ToolEffect::Process, "cancelled"),
    ];
    for (index, (tool_name, effect, code)) in cases.into_iter().enumerate() {
        let call_id = format!("call-illegal-cancelled-{index}");
        let cancelled = result_for_tool(&call_id, tool_name, ToolTerminalStatus::Cancelled, code);
        let (mut invocation_machine, _) =
            InvocationMachine::request(invocation_for(&call_id, tool_name, effect));
        invocation_machine
            .apply(InvocationInput::PreflightAllowed {
                permission_mode: PermissionMode::FullAccess,
            })
            .expect("policy approval");
        invocation_machine
            .apply(InvocationInput::Start)
            .expect("start");
        assert_eq!(
            invocation_machine.apply(InvocationInput::Complete {
                result: cancelled.clone(),
            }),
            Err(InvocationError::InvalidTerminal),
            "runtime projection accepted illegal cancellation for {tool_name}"
        );

        let (mut session_machine, turn_id) = started_session_machine(&call_id, tool_name, effect);
        assert_eq!(
            session_machine.apply(SessionCommand::RecordToolTerminal {
                record_id: RecordId::new(format!("record-terminal-illegal-cancelled-{index}"))
                    .expect("record"),
                turn_id,
                result: cancelled,
                now_unix_ms: 6,
            }),
            Err(RuntimeErrorCode::Recovery),
            "durable projection accepted illegal cancellation for {tool_name}"
        );
    }
}

#[test]
fn durable_session_projection_matches_late_shell_rejection_policy() {
    let (mut read_machine, read_turn) =
        started_session_machine("call-durable-read", "read_file", ToolEffect::Read);
    assert_eq!(
        read_machine.apply(SessionCommand::RecordToolTerminal {
            record_id: RecordId::new("record-terminal-read").expect("record"),
            turn_id: read_turn,
            result: result_for_tool(
                "call-durable-read",
                "read_file",
                ToolTerminalStatus::Rejected,
                "shell_session_not_found",
            ),
            now_unix_ms: 6,
        }),
        Err(RuntimeErrorCode::Recovery)
    );

    for (index, (tool_name, code)) in REAL_LATE_SHELL_REJECTIONS.into_iter().enumerate() {
        let call_id = format!("call-durable-{tool_name}-{index}");
        let (mut machine, turn_id) =
            started_session_machine(&call_id, tool_name, ToolEffect::Process);
        machine
            .apply(SessionCommand::RecordToolTerminal {
                record_id: RecordId::new(format!("record-terminal-{tool_name}-{index}"))
                    .expect("record"),
                turn_id,
                result: result_for_tool(&call_id, tool_name, ToolTerminalStatus::Rejected, code),
                now_unix_ms: 6,
            })
            .expect("durable real late Shell rejection");
    }

    for (index, (tool_name, code)) in FORGED_LATE_SHELL_REJECTIONS.into_iter().enumerate() {
        let call_id = format!("call-durable-forged-{tool_name}-{index}");
        let (mut machine, turn_id) =
            started_session_machine(&call_id, tool_name, ToolEffect::Process);
        assert_eq!(
            machine.apply(SessionCommand::RecordToolTerminal {
                record_id: RecordId::new(format!("record-terminal-forged-{tool_name}-{index}"))
                    .expect("record"),
                turn_id,
                result: result_for_tool(&call_id, tool_name, ToolTerminalStatus::Rejected, code),
                now_unix_ms: 6,
            }),
            Err(RuntimeErrorCode::Recovery)
        );
    }
}
