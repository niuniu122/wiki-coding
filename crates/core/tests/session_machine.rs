use minimax_core::{SessionCommand, SessionEffect, SessionMachine};
use minimax_protocol::{
    ConversationItem, MessageRole, ModelBinding, ModelId, ProviderId, ProviderProtocolKind,
    RecordId, RequestId, RuntimeErrorCode, RuntimeTerminalOutcome, SchemaVersion, SessionId,
    SessionRecordV1, SessionStatus, ToolCall, ToolCallId, ToolDecision, ToolDecisionKind,
    ToolEffect, ToolInvocation, ToolResult, ToolTerminalStatus, TurnId, TurnReceipt, TurnStatus,
};

fn record_id(value: &str) -> RecordId {
    RecordId::new(value).expect("record")
}

fn binding() -> ModelBinding {
    ModelBinding {
        provider_id: ProviderId::new("provider:test").expect("provider"),
        model_id: ModelId::new("model-test").expect("model"),
        protocol: ProviderProtocolKind::Responses,
    }
}

fn create(machine: &mut SessionMachine, suffix: &str) -> SessionId {
    let session_id = SessionId::new(format!("session-{suffix}")).expect("session");
    let effects = machine
        .apply(SessionCommand::Create {
            record_id: record_id(&format!("record-create-{suffix}")),
            session_id: session_id.clone(),
            binding: binding(),
            now_unix_ms: 1,
        })
        .expect("create session");
    assert!(matches!(effects.first(), Some(SessionEffect::Persist(_))));
    session_id
}

fn start(machine: &mut SessionMachine, turn: &str, record: &str) {
    let effects = machine
        .apply(SessionCommand::Continue {
            record_id: record_id(record),
            turn_id: TurnId::new(turn).expect("turn"),
            request_id: RequestId::new(format!("request-{turn}")).expect("request"),
            user_input: "question".to_owned(),
            max_output_tokens: 128,
            now_unix_ms: 2,
        })
        .expect("start turn");
    assert!(matches!(effects[0], SessionEffect::Persist(_)));
    let SessionEffect::StartTurn(request) = &effects[1] else {
        panic!("start effect expected");
    };
    assert!(matches!(
        request.messages.last(),
        Some(ConversationItem::Message(message)) if message.role == MessageRole::User
    ));
}

fn persisted(effects: &[SessionEffect]) -> minimax_protocol::SessionRecordV1 {
    effects
        .iter()
        .find_map(|effect| match effect {
            SessionEffect::Persist(record) => Some(record.clone()),
            _ => None,
        })
        .expect("persist effect")
}

fn invocation(call_id: &str) -> ToolInvocation {
    ToolInvocation::new(
        ToolCall::new(
            ToolCallId::new(call_id).expect("call"),
            "read_file",
            r#"{"path":"README.md"}"#,
        )
        .expect("tool call"),
        ToolEffect::Read,
    )
    .expect("invocation")
}

fn decision(call_id: &str, kind: ToolDecisionKind) -> ToolDecision {
    ToolDecision {
        schema_version: SchemaVersion,
        call_id: ToolCallId::new(call_id).expect("call"),
        decision: kind,
        code: match kind {
            ToolDecisionKind::Approved => "approved",
            ToolDecisionKind::Rejected => "rejected",
        }
        .to_owned(),
    }
}

fn result(call_id: &str, status: ToolTerminalStatus) -> ToolResult {
    ToolResult {
        schema_version: SchemaVersion,
        call_id: ToolCallId::new(call_id).expect("call"),
        tool_name: "read_file".to_owned(),
        status,
        code: "done".to_owned(),
        output: (status == ToolTerminalStatus::Succeeded).then(|| "contents".to_owned()),
    }
}

#[test]
fn create_list_resume_continue_finalize_and_replay_are_deterministic() {
    let mut machine = SessionMachine::new();
    let first = create(&mut machine, "first");
    let second = create(&mut machine, "second");
    let listed = machine.apply(SessionCommand::List).expect("list");
    let SessionEffect::Listed(items) = &listed[0] else {
        panic!("list effect expected");
    };
    assert_eq!(items.len(), 2);
    assert_eq!(
        items
            .iter()
            .find(|item| item.session_id == first)
            .map(|item| item.status),
        Some(SessionStatus::Archived)
    );
    machine
        .apply(SessionCommand::Resume {
            record_id: record_id("record-resume"),
            session_id: second,
            now_unix_ms: 2,
        })
        .expect("resume");
    start(&mut machine, "turn-1", "record-turn-1");
    let session_id = machine.active_session().expect("active").session_id.clone();
    let receipt = TurnReceipt {
        session_id,
        turn_id: TurnId::new("turn-1").expect("turn"),
        request_id: RequestId::new("request-turn-1").expect("request"),
        outcome: RuntimeTerminalOutcome::Completed,
        usage: None,
    };
    machine
        .apply(SessionCommand::Finalize {
            record_id: record_id("record-final"),
            receipt,
            assistant_content: Some("answer".to_owned()),
            now_unix_ms: 3,
        })
        .expect("finalize");

    assert_eq!(
        machine.active_session().expect("active").turns[0].status,
        TurnStatus::Completed
    );
}

#[test]
fn journal_replay_reconstructs_state_and_ignores_duplicate_records() {
    let mut machine = SessionMachine::new();
    let session_id = SessionId::new("session-replay").expect("session");
    let mut records = Vec::new();

    let effects = machine
        .apply(SessionCommand::Create {
            record_id: record_id("record-replay-create"),
            session_id: session_id.clone(),
            binding: binding(),
            now_unix_ms: 1,
        })
        .expect("create");
    records.push(persisted(&effects));

    let effects = machine
        .apply(SessionCommand::Continue {
            record_id: record_id("record-replay-turn"),
            turn_id: TurnId::new("turn-replay").expect("turn"),
            request_id: RequestId::new("request-replay").expect("request"),
            user_input: "question".to_owned(),
            max_output_tokens: 128,
            now_unix_ms: 2,
        })
        .expect("continue");
    records.push(persisted(&effects));

    let effects = machine
        .apply(SessionCommand::Finalize {
            record_id: record_id("record-replay-final"),
            receipt: TurnReceipt {
                session_id,
                turn_id: TurnId::new("turn-replay").expect("turn"),
                request_id: RequestId::new("request-replay").expect("request"),
                outcome: RuntimeTerminalOutcome::Completed,
                usage: None,
            },
            assistant_content: Some("answer".to_owned()),
            now_unix_ms: 3,
        })
        .expect("finalize");
    records.push(persisted(&effects));

    let replayed = SessionMachine::replay(records.clone()).expect("replay");
    assert_eq!(replayed.sessions(), machine.sessions());
    assert_eq!(replayed.active_session(), machine.active_session());

    let mut duplicated = records.clone();
    duplicated.extend(records);
    let replayed_twice = SessionMachine::replay(duplicated).expect("duplicate replay");
    assert_eq!(replayed_twice, replayed);
}

#[test]
fn retry_uses_new_identity_and_keeps_terminal_source_immutable() {
    let mut machine = SessionMachine::new();
    create(&mut machine, "retry");
    start(&mut machine, "turn-old", "record-old");
    let session_id = machine.active_session().expect("active").session_id.clone();
    machine
        .apply(SessionCommand::Finalize {
            record_id: record_id("record-old-final"),
            receipt: TurnReceipt {
                session_id,
                turn_id: TurnId::new("turn-old").expect("turn"),
                request_id: RequestId::new("request-turn-old").expect("request"),
                outcome: RuntimeTerminalOutcome::Interrupted,
                usage: None,
            },
            assistant_content: Some("partial must stay evidence only".to_owned()),
            now_unix_ms: 3,
        })
        .expect("finalize old");
    let effects = machine
        .apply(SessionCommand::Retry {
            record_id: record_id("record-retry"),
            source_turn_id: TurnId::new("turn-old").expect("old"),
            new_turn_id: TurnId::new("turn-new").expect("new"),
            request_id: RequestId::new("request-new").expect("request"),
            max_output_tokens: 128,
            now_unix_ms: 4,
        })
        .expect("retry");
    let SessionEffect::StartTurn(request) = &effects[1] else {
        panic!("start effect");
    };
    assert_eq!(request.messages.len(), 1);
    assert!(!format!("{request:?}").contains("partial must stay evidence only"));
    let turns = &machine.active_session().expect("active").turns;
    assert_eq!(turns[0].status, TurnStatus::Interrupted);
    assert_eq!(
        turns[1].retry_of.as_ref().map(TurnId::as_str),
        Some("turn-old")
    );
}

#[test]
fn recovery_is_one_durable_interruption_and_duplicate_record_is_idempotent() {
    let mut machine = SessionMachine::new();
    create(&mut machine, "recover");
    start(&mut machine, "turn-stale", "record-stale");
    let effects = machine
        .apply(SessionCommand::Recover {
            record_id: record_id("record-recover"),
            turn_id: TurnId::new("turn-stale").expect("turn"),
            partial_assistant: Some("saved partial".to_owned()),
            now_unix_ms: 10,
        })
        .expect("recover");
    let SessionEffect::Persist(record) = effects[0].clone() else {
        panic!("persist recovery");
    };
    assert_eq!(
        machine.active_session().expect("active").turns[0].status,
        TurnStatus::Interrupted
    );
    assert!(
        machine
            .apply(SessionCommand::Replay(record))
            .expect("duplicate replay")
            .is_empty()
    );
    assert!(
        machine
            .apply(SessionCommand::Recover {
                record_id: record_id("record-recover-again"),
                turn_id: TurnId::new("turn-stale").expect("turn"),
                partial_assistant: None,
                now_unix_ms: 11,
            })
            .is_err()
    );
}

#[test]
fn concurrent_turn_and_terminal_mutation_fail_closed() {
    let mut machine = SessionMachine::new();
    create(&mut machine, "busy");
    start(&mut machine, "turn-live", "record-live");
    assert_eq!(
        machine.apply(SessionCommand::Continue {
            record_id: record_id("record-second"),
            turn_id: TurnId::new("turn-second").expect("turn"),
            request_id: RequestId::new("request-second").expect("request"),
            user_input: "second".to_owned(),
            max_output_tokens: 128,
            now_unix_ms: 3,
        }),
        Err(minimax_protocol::RuntimeErrorCode::WorkspaceBusy)
    );
}

#[test]
fn tool_records_project_request_decision_start_and_terminal_in_order() {
    let mut machine = SessionMachine::new();
    create(&mut machine, "tools");
    start(&mut machine, "turn-tools", "record-turn-tools");
    let turn_id = TurnId::new("turn-tools").expect("turn");

    machine
        .apply(SessionCommand::RecordToolRequested {
            record_id: record_id("record-tool-request"),
            turn_id: turn_id.clone(),
            invocation: invocation("call-tools"),
            now_unix_ms: 3,
        })
        .expect("request");
    assert_eq!(
        machine.apply(SessionCommand::RecordToolStarted {
            record_id: record_id("record-tool-early-start"),
            turn_id: turn_id.clone(),
            call_id: ToolCallId::new("call-tools").expect("call"),
            now_unix_ms: 4,
        }),
        Err(RuntimeErrorCode::Recovery)
    );
    machine
        .apply(SessionCommand::RecordToolDecision {
            record_id: record_id("record-tool-decision"),
            turn_id: turn_id.clone(),
            decision: decision("call-tools", ToolDecisionKind::Approved),
            now_unix_ms: 5,
        })
        .expect("decision");
    machine
        .apply(SessionCommand::RecordToolStarted {
            record_id: record_id("record-tool-start"),
            turn_id: turn_id.clone(),
            call_id: ToolCallId::new("call-tools").expect("call"),
            now_unix_ms: 6,
        })
        .expect("started");

    let session_id = machine.active_session().expect("active").session_id.clone();
    let receipt = TurnReceipt {
        session_id,
        turn_id: turn_id.clone(),
        request_id: RequestId::new("request-turn-tools").expect("request"),
        outcome: RuntimeTerminalOutcome::Completed,
        usage: None,
    };
    assert_eq!(
        machine.apply(SessionCommand::Finalize {
            record_id: record_id("record-tool-early-final"),
            receipt: receipt.clone(),
            assistant_content: Some("too early".to_owned()),
            now_unix_ms: 7,
        }),
        Err(RuntimeErrorCode::Recovery)
    );
    machine
        .apply(SessionCommand::RecordToolTerminal {
            record_id: record_id("record-tool-terminal"),
            turn_id: turn_id.clone(),
            result: result("call-tools", ToolTerminalStatus::Succeeded),
            now_unix_ms: 8,
        })
        .expect("terminal");
    machine
        .apply(SessionCommand::Finalize {
            record_id: record_id("record-tool-final"),
            receipt,
            assistant_content: Some("answer".to_owned()),
            now_unix_ms: 9,
        })
        .expect("finalize");

    let projected = &machine.active_session().expect("active").turns[0].tool_invocations[0];
    assert_eq!(projected.requested_at_unix_ms, 3);
    assert_eq!(projected.decision_at_unix_ms, Some(5));
    assert_eq!(projected.started_at_unix_ms, Some(6));
    assert_eq!(projected.terminal_at_unix_ms, Some(8));
    assert_eq!(
        projected.terminal_result.as_ref().map(|value| value.status),
        Some(ToolTerminalStatus::Succeeded)
    );
}

#[test]
fn tool_records_reject_wrong_turn_call_duplicate_identity_and_conflicting_replay() {
    let mut machine = SessionMachine::new();
    create(&mut machine, "tool-errors");
    start(&mut machine, "turn-tool-errors", "record-turn-tool-errors");
    let turn_id = TurnId::new("turn-tool-errors").expect("turn");
    let effects = machine
        .apply(SessionCommand::RecordToolRequested {
            record_id: record_id("record-request-stable"),
            turn_id: turn_id.clone(),
            invocation: invocation("call-stable"),
            now_unix_ms: 3,
        })
        .expect("request");
    let original = persisted(&effects);

    assert_eq!(
        machine.apply(SessionCommand::RecordToolRequested {
            record_id: record_id("record-request-duplicate-call"),
            turn_id: turn_id.clone(),
            invocation: invocation("call-stable"),
            now_unix_ms: 4,
        }),
        Err(RuntimeErrorCode::Recovery)
    );
    assert_eq!(
        machine.apply(SessionCommand::RecordToolDecision {
            record_id: record_id("record-wrong-call"),
            turn_id: turn_id.clone(),
            decision: decision("call-other", ToolDecisionKind::Approved),
            now_unix_ms: 4,
        }),
        Err(RuntimeErrorCode::Recovery)
    );
    assert_eq!(
        machine.apply(SessionCommand::RecordToolTerminal {
            record_id: record_id("record-wrong-turn"),
            turn_id: TurnId::new("turn-other").expect("turn"),
            result: result("call-stable", ToolTerminalStatus::Failed),
            now_unix_ms: 4,
        }),
        Err(RuntimeErrorCode::Recovery)
    );
    assert!(
        machine
            .apply(SessionCommand::Replay(original.clone()))
            .expect("identical replay")
            .is_empty()
    );
    let mut conflicting: SessionRecordV1 = original;
    conflicting.record = minimax_protocol::JournalRecord::ToolRequested {
        session_id: machine.active_session().expect("active").session_id.clone(),
        turn_id,
        invocation: invocation("call-conflict"),
        requested_at_unix_ms: 99,
    };
    assert_eq!(
        machine.apply(SessionCommand::Replay(conflicting)),
        Err(RuntimeErrorCode::Recovery)
    );
}
