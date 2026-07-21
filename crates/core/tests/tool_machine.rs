use minimax_core::{
    AgentBudget, BudgetKind, InvocationEffect, InvocationError, InvocationInput, InvocationMachine,
    InvocationRegistry, InvocationState, PermissionMode, ToolExecutionContext, ToolSandboxPolicy,
};
use minimax_protocol::{
    AgentLimits, SchemaVersion, ToolCall, ToolCallId, ToolDecision, ToolDecisionKind, ToolEffect,
    ToolInvocation, ToolResult, ToolTerminalStatus,
};

fn invocation(id: &str) -> ToolInvocation {
    ToolInvocation::new(
        ToolCall::new(
            ToolCallId::new(id).expect("call ID"),
            "read_file",
            r#"{"path":"README.md"}"#,
        )
        .expect("call"),
        ToolEffect::Read,
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
    ToolResult {
        schema_version: SchemaVersion,
        call_id: ToolCallId::new(id).expect("result ID"),
        tool_name: "read_file".to_owned(),
        status,
        code: code.to_owned(),
        output: None,
    }
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
