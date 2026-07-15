use std::cell::Cell;
use std::collections::BTreeMap;

use minimax_core::{
    CompactionBudget, CompactionError, LocalCompactor, SafeTraceFact, SafeTraceRecorder,
};
use minimax_protocol::{
    CompactionId, MessageRole, ModelBinding, ModelId, ProviderId, ProviderProtocolKind, RequestId,
    SessionId, SessionRecord, SessionStatus, TraceCode, TurnId, TurnRecord, TurnStatus,
    VisibleMessage,
};

fn binding() -> ModelBinding {
    ModelBinding {
        provider_id: ProviderId::new("provider:test").expect("provider"),
        model_id: ModelId::new("model-test").expect("model"),
        protocol: ProviderProtocolKind::Responses,
    }
}

fn turn(
    index: usize,
    status: TurnStatus,
    user: &str,
    assistant: &str,
    partial: bool,
) -> TurnRecord {
    TurnRecord {
        turn_id: TurnId::new(format!("turn-{index}")).expect("turn"),
        request_id: RequestId::new(format!("request-{index}")).expect("request"),
        started_at_unix_ms: index as u64,
        completed_at_unix_ms: status.is_terminal().then_some(index as u64 + 1),
        retry_of: None,
        status,
        user_message: VisibleMessage {
            role: MessageRole::User,
            content: user.to_owned(),
            partial: false,
        },
        assistant_message: Some(VisibleMessage {
            role: MessageRole::Assistant,
            content: assistant.to_owned(),
            partial,
        }),
        usage: None,
        receipt: None,
    }
}

fn session() -> SessionRecord {
    SessionRecord {
        session_id: SessionId::new("session-compact").expect("session"),
        created_at_unix_ms: 1,
        updated_at_unix_ms: 10,
        status: SessionStatus::Active,
        binding: binding(),
        turns: vec![
            turn(
                1,
                TurnStatus::Completed,
                "Original goal: build a deterministic local context engine.",
                "Decision: use Rust. <think>DO_NOT_PERSIST_REASONING</think> Visible answer.",
                false,
            ),
            turn(
                2,
                TurnStatus::Completed,
                "Constraint: it must stay offline. API_KEY=sk-supersecret",
                "Open item: which Unicode strategy should we choose?",
                false,
            ),
            turn(
                3,
                TurnStatus::Interrupted,
                "interrupted user evidence",
                "DO_NOT_PERSIST_PARTIAL raw_frame tool_body",
                true,
            ),
            turn(
                4,
                TurnStatus::Failed,
                "failed user evidence",
                "DO_NOT_PERSIST_FAILED",
                true,
            ),
        ],
        compaction: None,
    }
}

#[test]
fn compaction_is_byte_stable_structured_and_excludes_unsafe_history() {
    let budget = CompactionBudget {
        max_record_bytes: 16 * 1024,
        retain_recent_turns: 2,
    };
    let first = LocalCompactor::compact(
        &session(),
        CompactionId::new("compact-1").expect("compaction"),
        budget,
    )
    .expect("compact");
    let second = LocalCompactor::compact(
        &session(),
        CompactionId::new("compact-1").expect("compaction"),
        budget,
    )
    .expect("compact again");
    let first_json = serde_json::to_vec(&first).expect("first JSON");
    let second_json = serde_json::to_vec(&second).expect("second JSON");

    assert_eq!(first_json, second_json);
    assert_eq!(first.covered_through_turn_id.as_str(), "turn-2");
    assert_eq!(first.retained_recent_turns.len(), 2);
    assert!(!first.goal.is_empty());
    assert!(!first.constraints.is_empty());
    assert!(!first.decisions.is_empty());
    assert!(!first.open_items.is_empty());
    let serialized = String::from_utf8(first_json).expect("UTF-8");
    for marker in [
        "sk-supersecret",
        "DO_NOT_PERSIST_REASONING",
        "DO_NOT_PERSIST_PARTIAL",
        "DO_NOT_PERSIST_FAILED",
        "raw_frame",
        "tool_body",
    ] {
        assert!(!serialized.contains(marker), "unsafe marker: {marker}");
    }
    assert!(serialized.contains("[REDACTED]"));
    assert!(serialized.contains("Visible answer"));
}

#[test]
fn compaction_never_slices_required_entries_and_rejects_too_small_budgets() {
    let result = LocalCompactor::compact(
        &session(),
        CompactionId::new("compact-small").expect("compaction"),
        CompactionBudget {
            max_record_bytes: 100,
            retain_recent_turns: 2,
        },
    );
    assert_eq!(result, Err(CompactionError::BudgetTooSmall));

    let mut unicode = session();
    unicode.turns[0].user_message.content = "界".repeat(1400);
    assert_eq!(
        LocalCompactor::compact(
            &unicode,
            CompactionId::new("compact-large").expect("compaction"),
            CompactionBudget {
                max_record_bytes: 32 * 1024,
                retain_recent_turns: 1,
            },
        ),
        Err(CompactionError::EntryTooLarge)
    );
}

#[test]
fn local_compaction_has_no_provider_execution_path() {
    struct CountingProvider(Cell<u64>);
    let provider = CountingProvider(Cell::new(0));
    LocalCompactor::compact(
        &session(),
        CompactionId::new("compact-offline").expect("compaction"),
        CompactionBudget {
            max_record_bytes: 16 * 1024,
            retain_recent_turns: 1,
        },
    )
    .expect("local compaction");
    assert_eq!(provider.0.get(), 0);
}

#[test]
fn trace_keeps_only_bounded_allowlisted_safe_facts_and_folds_deterministically() {
    let entry = SafeTraceRecorder::record(
        10,
        TraceCode::ProviderFailed,
        BTreeMap::from([
            (
                "provider_id".to_owned(),
                SafeTraceFact::String("provider:test".to_owned()),
            ),
            (
                "request_id".to_owned(),
                SafeTraceFact::String("DO_NOT_PERSIST_THIS sk-supersecret".to_owned()),
            ),
            ("retryable".to_owned(), SafeTraceFact::Bool(false)),
            (
                "raw_frame".to_owned(),
                SafeTraceFact::String("{\"choices\":[{\"delta\":{}}]}".to_owned()),
            ),
            ("status".to_owned(), SafeTraceFact::U64(503)),
            ("kind".to_owned(), SafeTraceFact::String("x".repeat(129))),
        ]),
    );
    assert_eq!(
        entry.facts,
        BTreeMap::from([
            ("provider_id".to_owned(), "provider:test".to_owned()),
            ("request_id".to_owned(), "[REDACTED]".to_owned()),
            ("retryable".to_owned(), "false".to_owned()),
            ("status".to_owned(), "503".to_owned()),
        ])
    );
    let serialized = serde_json::to_string(&entry).expect("trace JSON");
    let debug = format!("{entry:?}");
    for marker in [
        "DO_NOT_PERSIST_THIS",
        "sk-supersecret",
        "choices",
        "raw_frame",
    ] {
        assert!(!serialized.contains(marker));
        assert!(!debug.contains(marker));
    }

    let folded = SafeTraceRecorder::fold(&[
        entry.clone(),
        SafeTraceRecorder::record(11, TraceCode::ProviderFailed, BTreeMap::new()),
        SafeTraceRecorder::record(12, TraceCode::TurnRecovered, BTreeMap::new()),
    ]);
    assert_eq!(folded.total, 3);
    assert_eq!(folded.last_recorded_at_unix_ms, Some(12));
    assert_eq!(folded.counts.get(&TraceCode::ProviderFailed), Some(&2));
    assert_eq!(folded.counts.get(&TraceCode::TurnRecovered), Some(&1));
}
