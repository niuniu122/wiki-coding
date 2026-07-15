use minimax_core::{RunEffect, RunInput, RunMachine, RunState};
use minimax_protocol::{
    AgentLimits, AssistantToolCallBatch, ConversationItem, MessageRole, ModelId, ModelMessage,
    OutputSettings, ProtocolErrorCode, ProviderId, ProviderProtocolKind, RequestId,
    RuntimeErrorCode, RuntimeEvent, RuntimeFailure, RuntimeTerminalOutcome, SchemaVersion,
    SessionId, StreamEvent, TerminalOutcome, ToolCallFragment, ToolCallId, ToolDefinition,
    ToolResult, ToolResultMessage, ToolTerminalStatus, TurnId, TurnRequest, Usage,
};
use serde_json::json;

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

fn agent_request() -> TurnRequest {
    let mut request = request();
    request.tools = vec![definition("read_file"), definition("list_directory")];
    request.agent_limits = Some(AgentLimits::default());
    request
}

fn definition(name: &str) -> ToolDefinition {
    ToolDefinition::new(
        name,
        "Bounded fixture tool.",
        json!({
            "type":"object",
            "properties":{"path":{"type":"string"}},
            "required":["path"],
            "additionalProperties":false
        }),
    )
    .expect("definition")
}

fn complete_call(id: &str, name: &str, path: &str, index: u32) -> StreamEvent {
    StreamEvent::ToolCallFragments {
        fragments: vec![ToolCallFragment {
            call_id: ToolCallId::new(id).expect("call ID"),
            stream_id: None,
            name: Some(name.to_owned()),
            arguments_delta: Some(format!(r#"{{"path":"{path}"}}"#)),
            arguments_complete: true,
            index: Some(index),
        }],
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

#[test]
fn agent_rounds_pause_for_zero_one_or_multiple_complete_calls() {
    let mut empty = RunMachine::new();
    empty
        .apply(RunInput::Begin(agent_request()))
        .expect("agent begin");
    let completed = empty
        .apply(RunInput::ProviderEvent(StreamEvent::Terminal {
            outcome: TerminalOutcome::Completed,
        }))
        .expect("empty round completes");
    assert!(
        completed
            .iter()
            .any(|effect| matches!(effect, RunEffect::Finalize(_)))
    );

    for calls in [
        vec![("call-z", "read_file", "README.md")],
        vec![
            ("call-z", "read_file", "README.md"),
            ("call-a", "list_directory", "crates"),
        ],
    ] {
        let mut machine = RunMachine::new();
        machine
            .apply(RunInput::Begin(agent_request()))
            .expect("agent begin");
        for (index, (id, name, path)) in calls.iter().enumerate() {
            machine
                .apply(RunInput::ProviderEvent(complete_call(
                    id,
                    name,
                    path,
                    u32::try_from(index).expect("small index"),
                )))
                .expect("complete call");
        }
        let effects = machine
            .apply(RunInput::ProviderEvent(StreamEvent::Terminal {
                outcome: TerminalOutcome::Completed,
            }))
            .expect("round terminal");
        let Some(RunEffect::BeginTools(invocations)) = effects.first() else {
            panic!("agent must pause for durable tools");
        };
        assert_eq!(invocations.len(), calls.len());
        assert_eq!(invocations[0].call.call_id.as_str(), "call-z");
        if calls.len() == 2 {
            assert_eq!(invocations[1].call.call_id.as_str(), "call-a");
        }
        assert!(matches!(machine.state(), RunState::AwaitingTools { .. }));
    }
}

#[test]
fn agent_continues_only_after_every_matching_result_is_in_native_history() {
    let mut machine = RunMachine::new();
    let initial = agent_request();
    machine
        .apply(RunInput::Begin(initial.clone()))
        .expect("agent begin");
    for event in [
        complete_call("call-z", "read_file", "README.md", 0),
        complete_call("call-a", "list_directory", "crates", 1),
    ] {
        machine
            .apply(RunInput::ProviderEvent(event))
            .expect("complete call");
    }
    let effects = machine
        .apply(RunInput::ProviderEvent(StreamEvent::Terminal {
            outcome: TerminalOutcome::Completed,
        }))
        .expect("round complete");
    let RunEffect::BeginTools(invocations) = &effects[0] else {
        panic!("tool batch");
    };

    let mut incomplete = initial.clone();
    incomplete
        .messages
        .push(ConversationItem::AssistantToolCalls(
            AssistantToolCallBatch {
                tool_calls: invocations
                    .iter()
                    .map(|invocation| invocation.call.clone())
                    .collect(),
            },
        ));
    incomplete
        .messages
        .push(ConversationItem::ToolResult(ToolResultMessage {
            tool_result: ToolResult {
                schema_version: SchemaVersion,
                call_id: invocations[0].call.call_id.clone(),
                tool_name: invocations[0].call.name.clone(),
                status: ToolTerminalStatus::Rejected,
                code: "user_rejected".to_owned(),
                output: None,
            },
        }));
    assert_eq!(
        machine.apply(RunInput::ContinueAfterTools(incomplete.clone())),
        Err(RuntimeErrorCode::Recovery)
    );

    let mut complete = incomplete;
    complete
        .messages
        .push(ConversationItem::ToolResult(ToolResultMessage {
            tool_result: ToolResult {
                schema_version: SchemaVersion,
                call_id: invocations[1].call.call_id.clone(),
                tool_name: invocations[1].call.name.clone(),
                status: ToolTerminalStatus::Succeeded,
                code: "ok".to_owned(),
                output: Some("protocol".to_owned()),
            },
        }));
    let continuation = machine
        .apply(RunInput::ContinueAfterTools(complete))
        .expect("all results durable");
    assert!(matches!(
        continuation.as_slice(),
        [RunEffect::OpenProvider(_)]
    ));
    machine
        .apply(RunInput::ProviderEvent(StreamEvent::VisibleTextDelta {
            delta: "done".to_owned(),
        }))
        .expect("final text");
    machine
        .apply(RunInput::ProviderEvent(StreamEvent::Terminal {
            outcome: TerminalOutcome::Completed,
        }))
        .expect("final terminal");
    assert!(matches!(machine.state(), RunState::Terminal { .. }));
}
