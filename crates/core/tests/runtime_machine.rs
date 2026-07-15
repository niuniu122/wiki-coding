use minimax_core::{RunEffect, RunInput, RunMachine, RunState};
use minimax_protocol::{
    MessageRole, ModelId, ModelMessage, OutputSettings, ProtocolErrorCode, ProviderId,
    ProviderProtocolKind, RequestId, RuntimeErrorCode, RuntimeEvent, RuntimeFailure,
    RuntimeTerminalOutcome, SessionId, StreamEvent, TerminalOutcome, ToolCallFragment, ToolCallId,
    TurnId, TurnRequest, Usage,
};

fn request() -> TurnRequest {
    TurnRequest {
        session_id: SessionId::new("session-1").expect("session"),
        turn_id: TurnId::new("turn-1").expect("turn"),
        request_id: RequestId::new("request-1").expect("request"),
        provider_id: ProviderId::new("provider:test").expect("provider"),
        model_id: ModelId::new("model-test").expect("model"),
        protocol: ProviderProtocolKind::Responses,
        messages: vec![
            ModelMessage {
                role: MessageRole::User,
                content: "hello".to_owned(),
            }
            .into(),
        ],
        tools: Vec::new(),
        agent_limits: None,
        output: OutputSettings::new(64).expect("output"),
    }
}

fn assert_persist_before_publish(effects: &[RunEffect]) {
    for (index, effect) in effects.iter().enumerate() {
        if let RunEffect::Publish(published) = effect {
            assert!(
                effects[..index]
                    .iter()
                    .any(|prior| matches!(prior, RunEffect::Persist(value) if value == published))
            );
        }
    }
}

fn start(machine: &mut RunMachine) {
    let effects = machine
        .apply(RunInput::Begin(request()))
        .expect("begin should work");
    assert_persist_before_publish(&effects);
    assert!(matches!(effects.last(), Some(RunEffect::OpenProvider(_))));
}

#[test]
fn completed_turn_persists_every_event_before_publish_and_finalizes_once() {
    let mut machine = RunMachine::new();
    start(&mut machine);
    for event in [
        StreamEvent::VisibleTextDelta {
            delta: "visible".to_owned(),
        },
        StreamEvent::Usage {
            usage: Usage {
                input_tokens: Some(3),
                output_tokens: Some(2),
                total_tokens: Some(5),
            },
        },
        StreamEvent::Terminal {
            outcome: TerminalOutcome::Completed,
        },
    ] {
        let effects = machine
            .apply(RunInput::ProviderEvent(event))
            .expect("valid event");
        assert_persist_before_publish(&effects);
    }
    let RunState::Terminal { receipt } = machine.state() else {
        panic!("terminal receipt expected");
    };
    assert_eq!(receipt.outcome, RuntimeTerminalOutcome::Completed);
    assert_eq!(receipt.usage.and_then(|usage| usage.total_tokens), Some(5));
    assert!(
        machine
            .apply(RunInput::Shutdown)
            .expect("idempotent")
            .is_empty()
    );
}

#[test]
fn cancellation_and_failures_have_one_truthful_terminal() {
    let cases = [
        (
            RunInput::Cancel,
            RuntimeTerminalOutcome::Interrupted,
            RuntimeErrorCode::ProtocolEventAfterTerminal,
        ),
        (
            RunInput::ProviderFailed(RuntimeFailure::new(RuntimeErrorCode::TransportTimeout)),
            RuntimeTerminalOutcome::Failed {
                failure: RuntimeFailure::new(RuntimeErrorCode::TransportTimeout),
            },
            RuntimeErrorCode::ProtocolEventAfterTerminal,
        ),
        (
            RunInput::ProviderEof,
            RuntimeTerminalOutcome::Failed {
                failure: RuntimeFailure::new(RuntimeErrorCode::ProtocolPrematureEof),
            },
            RuntimeErrorCode::ProtocolEventAfterTerminal,
        ),
    ];
    for (input, expected, post_terminal_error) in cases {
        let mut machine = RunMachine::new();
        start(&mut machine);
        let effects = machine.apply(input).expect("terminal transition");
        assert_persist_before_publish(&effects);
        assert_eq!(
            effects
                .iter()
                .filter(|effect| matches!(effect, RunEffect::Finalize(_)))
                .count(),
            1
        );
        let RunState::Terminal { receipt } = machine.state() else {
            panic!("terminal receipt expected");
        };
        assert_eq!(receipt.outcome, expected);
        assert_eq!(
            machine.apply(RunInput::ProviderEof),
            Err(post_terminal_error)
        );
    }
}

#[test]
fn terminal_order_errors_and_concurrent_begin_are_exact() {
    let mut machine = RunMachine::new();
    start(&mut machine);
    assert_eq!(
        machine.apply(RunInput::Begin(request())),
        Err(RuntimeErrorCode::WorkspaceBusy)
    );
    machine
        .apply(RunInput::ProviderEvent(StreamEvent::Terminal {
            outcome: TerminalOutcome::Completed,
        }))
        .expect("first terminal");
    assert_eq!(
        machine.apply(RunInput::ProviderEvent(StreamEvent::Terminal {
            outcome: TerminalOutcome::Failed {
                code: ProtocolErrorCode::MalformedJson,
            },
        })),
        Err(RuntimeErrorCode::ProtocolDuplicateTerminal)
    );
    assert_eq!(
        machine.apply(RunInput::ProviderEvent(StreamEvent::VisibleTextDelta {
            delta: "late".to_owned(),
        })),
        Err(RuntimeErrorCode::ProtocolEventAfterTerminal)
    );
}

#[test]
fn observed_tool_call_is_persisted_but_never_executed() {
    let mut machine = RunMachine::new();
    start(&mut machine);
    let effects = machine
        .apply(RunInput::ProviderEvent(StreamEvent::ToolCallFragments {
            fragments: vec![ToolCallFragment {
                call_id: ToolCallId::new("call-1").expect("call"),
                stream_id: None,
                name: Some("read_file".to_owned()),
                arguments_delta: Some("{\"path\":\"README.md\"}".to_owned()),
                arguments_complete: true,
                index: Some(0),
            }],
        }))
        .expect("tool observation should terminate safely");
    assert_persist_before_publish(&effects);
    assert!(effects.iter().any(|effect| matches!(
        effect,
        RunEffect::Persist(event)
            if matches!(event.event, RuntimeEvent::ToolCallObserved { .. })
    )));
    assert!(!format!("{effects:?}").contains("README.md"));
    let RunState::Terminal { receipt } = machine.state() else {
        panic!("tool observation should terminate");
    };
    assert_eq!(
        receipt.outcome,
        RuntimeTerminalOutcome::Failed {
            failure: RuntimeFailure::new(RuntimeErrorCode::ToolUnavailable)
        }
    );
}

#[test]
fn cancel_after_partial_delta_preserves_partial_event_and_interrupts() {
    let mut machine = RunMachine::new();
    start(&mut machine);
    let delta = machine
        .apply(RunInput::ProviderEvent(StreamEvent::VisibleTextDelta {
            delta: "partial".to_owned(),
        }))
        .expect("delta");
    assert_persist_before_publish(&delta);
    let terminal = machine.apply(RunInput::Cancel).expect("cancel");
    assert!(matches!(terminal.first(), Some(RunEffect::AbortProvider)));
    assert_persist_before_publish(&terminal);
}
