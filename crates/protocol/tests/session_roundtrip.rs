use std::collections::BTreeMap;

use minimax_protocol::{
    CompactionId, CompactionPointer, JournalRecord, MessageRole, ModelBinding, ModelId, ProviderId,
    ProviderProtocolKind, RecordId, RequestId, RuntimeErrorCode, RuntimeFailure,
    RuntimeTerminalOutcome, SessionId, SessionRecord, SessionRecordV1, SessionStatus, TraceCode,
    TraceEntry, TurnId, TurnReceipt, TurnRecord, TurnStatus, VisibleMessage,
    parse_session_record_v1,
};

fn binding() -> ModelBinding {
    ModelBinding {
        provider_id: ProviderId::new("provider:test").expect("provider"),
        model_id: ModelId::new("model-test").expect("model"),
        protocol: ProviderProtocolKind::Responses,
    }
}

fn session_id() -> SessionId {
    SessionId::new("session-1").expect("session")
}

fn turn_id() -> TurnId {
    TurnId::new("turn-1").expect("turn")
}

fn turn() -> TurnRecord {
    TurnRecord {
        turn_id: turn_id(),
        request_id: RequestId::new("request-1").expect("request"),
        started_at_unix_ms: 10,
        completed_at_unix_ms: None,
        retry_of: None,
        status: TurnStatus::Running,
        user_message: VisibleMessage {
            role: MessageRole::User,
            content: "hello".to_owned(),
            partial: false,
        },
        assistant_message: None,
        usage: None,
        receipt: None,
    }
}

fn round_trip(record: JournalRecord, suffix: &str) {
    let envelope = SessionRecordV1::new(
        RecordId::new(format!("record-{suffix}")).expect("record"),
        record,
    );
    let json = serde_json::to_string(&envelope).expect("serialize session record");
    assert_eq!(
        parse_session_record_v1(&json).expect("parse session record"),
        envelope
    );
}

#[test]
fn every_session_record_variant_round_trips_strictly() {
    let session = SessionRecord {
        session_id: session_id(),
        created_at_unix_ms: 1,
        updated_at_unix_ms: 1,
        status: SessionStatus::Active,
        binding: binding(),
        turns: Vec::new(),
        compaction: None,
    };
    round_trip(JournalRecord::SessionCreated { session }, "created");
    round_trip(
        JournalRecord::SessionActivated {
            session_id: session_id(),
            activated_at_unix_ms: 2,
        },
        "activated",
    );
    round_trip(
        JournalRecord::TurnStarted {
            session_id: session_id(),
            binding: binding(),
            turn: Box::new(turn()),
        },
        "turn-started",
    );
    round_trip(
        JournalRecord::TurnDelta {
            session_id: session_id(),
            turn_id: turn_id(),
            delta: "partial".to_owned(),
            recorded_at_unix_ms: 11,
        },
        "delta",
    );
    let receipt = TurnReceipt {
        session_id: session_id(),
        turn_id: turn_id(),
        request_id: RequestId::new("request-1").expect("request"),
        outcome: RuntimeTerminalOutcome::Failed {
            failure: RuntimeFailure::new(RuntimeErrorCode::TransportNetwork),
        },
        usage: None,
    };
    round_trip(
        JournalRecord::TurnTerminal {
            session_id: session_id(),
            receipt: receipt.clone(),
            assistant_message: Some(VisibleMessage {
                role: MessageRole::Assistant,
                content: "partial".to_owned(),
                partial: true,
            }),
            completed_at_unix_ms: 12,
        },
        "terminal",
    );
    round_trip(
        JournalRecord::RecoveryApplied {
            session_id: session_id(),
            receipt,
            partial_assistant_message: None,
            recovered_at_unix_ms: 13,
        },
        "recovery",
    );
    round_trip(
        JournalRecord::CompactionStored {
            session_id: session_id(),
            pointer: CompactionPointer {
                compaction_id: CompactionId::new("compact-1").expect("compaction"),
                covered_through_turn_id: turn_id(),
            },
            stored_at_unix_ms: 14,
        },
        "compaction",
    );
    round_trip(
        JournalRecord::TraceStored {
            session_id: session_id(),
            entry: TraceEntry {
                recorded_at_unix_ms: 15,
                code: TraceCode::TurnRecovered,
                facts: BTreeMap::from([("turn_id".to_owned(), "turn-1".to_owned())]),
            },
        },
        "trace",
    );
}

#[test]
fn session_records_reject_unknown_fields_versions_and_invalid_ids() {
    let unknown = r#"{"schemaVersion":1,"recordId":"record-1","record":{"type":"session_activated","session_id":"session-1","activated_at_unix_ms":2,"secret":"no"}}"#;
    assert_eq!(
        parse_session_record_v1(unknown),
        Err(RuntimeErrorCode::Recovery)
    );
    let version = r#"{"schemaVersion":2,"recordId":"record-1","record":{"type":"session_activated","session_id":"session-1","activated_at_unix_ms":2}}"#;
    assert_eq!(
        parse_session_record_v1(version),
        Err(RuntimeErrorCode::Recovery)
    );
    assert!(RecordId::new(" ").is_err());
    assert!(CompactionId::new("").is_err());
}

#[test]
fn terminal_status_classification_is_explicit() {
    assert!(!TurnStatus::Running.is_terminal());
    for status in [
        TurnStatus::Completed,
        TurnStatus::Failed,
        TurnStatus::Interrupted,
        TurnStatus::Stopped,
    ] {
        assert!(status.is_terminal());
    }
}
