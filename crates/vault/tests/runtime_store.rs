use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::Write;

use minimax_core::{
    SafeTraceFact, SafeTraceRecorder, SessionCommand, SessionEffect, SessionMachine,
};
use minimax_protocol::{
    JournalRecord, ModelBinding, ModelId, ProviderId, ProviderProtocolKind, RecordId, RequestId,
    SchemaVersion, SessionId, SessionRecordV1, ToolCall, ToolCallId, ToolDecision,
    ToolDecisionKind, ToolEffect, ToolInvocation, ToolResult, ToolTerminalStatus, TraceCode,
    TurnId, TurnStatus,
};
use minimax_vault::{RuntimeStore, RuntimeStoreError};
use tempfile::TempDir;

fn record_id(value: &str) -> RecordId {
    RecordId::new(value).expect("record ID")
}

fn binding() -> ModelBinding {
    ModelBinding {
        provider_id: ProviderId::new("provider:test").expect("provider"),
        model_id: ModelId::new("model-test").expect("model"),
        protocol: ProviderProtocolKind::Responses,
    }
}

fn persisted(effects: Vec<SessionEffect>) -> SessionRecordV1 {
    effects
        .into_iter()
        .find_map(|effect| match effect {
            SessionEffect::Persist(record) => Some(record),
            _ => None,
        })
        .expect("persist effect")
}

fn create_record(machine: &mut SessionMachine, suffix: &str) -> SessionRecordV1 {
    persisted(
        machine
            .apply(SessionCommand::Create {
                record_id: record_id(&format!("record-create-{suffix}")),
                session_id: SessionId::new(format!("session-{suffix}")).expect("session"),
                binding: binding(),
                now_unix_ms: 1,
            })
            .expect("create"),
    )
}

fn start_record(machine: &mut SessionMachine, suffix: &str) -> SessionRecordV1 {
    persisted(
        machine
            .apply(SessionCommand::Continue {
                record_id: record_id(&format!("record-start-{suffix}")),
                turn_id: TurnId::new(format!("turn-{suffix}")).expect("turn"),
                request_id: RequestId::new(format!("request-{suffix}")).expect("request"),
                user_input: "keep this user request".to_owned(),
                max_output_tokens: 128,
                now_unix_ms: 2,
            })
            .expect("continue"),
    )
}

fn root() -> TempDir {
    tempfile::tempdir().expect("temporary project")
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

fn decision(call_id: &str) -> ToolDecision {
    ToolDecision {
        schema_version: SchemaVersion,
        call_id: ToolCallId::new(call_id).expect("call"),
        decision: ToolDecisionKind::Approved,
        code: "approved".to_owned(),
    }
}

fn append_abandoned_tool_prefix(
    project: &TempDir,
    suffix: &str,
    started: bool,
) -> std::path::PathBuf {
    let mut policy = SessionMachine::new();
    let create = create_record(&mut policy, suffix);
    let start = start_record(&mut policy, suffix);
    let turn_id = TurnId::new(format!("turn-{suffix}")).expect("turn");
    let requested = persisted(
        policy
            .apply(SessionCommand::RecordToolRequested {
                record_id: record_id(&format!("record-tool-request-{suffix}")),
                turn_id: turn_id.clone(),
                invocation: invocation(&format!("call-{suffix}")),
                now_unix_ms: 3,
            })
            .expect("request"),
    );
    let approved = persisted(
        policy
            .apply(SessionCommand::RecordToolDecision {
                record_id: record_id(&format!("record-tool-decision-{suffix}")),
                turn_id: turn_id.clone(),
                decision: decision(&format!("call-{suffix}")),
                now_unix_ms: 4,
            })
            .expect("decision"),
    );
    let started_record = started.then(|| {
        persisted(
            policy
                .apply(SessionCommand::RecordToolStarted {
                    record_id: record_id(&format!("record-tool-started-{suffix}")),
                    turn_id,
                    call_id: ToolCallId::new(format!("call-{suffix}")).expect("call"),
                    now_unix_ms: 5,
                })
                .expect("started"),
        )
    });
    let mut store = RuntimeStore::open(project.path()).expect("open");
    for record in [create, start, requested, approved] {
        store.append(record).expect("append prefix");
    }
    if let Some(record) = started_record {
        store.append(record).expect("append started");
    }
    store.journal_path().to_path_buf()
}

#[test]
fn a_second_writer_is_busy_and_reopen_after_drop_succeeds() {
    let project = root();
    let first = RuntimeStore::open(project.path()).expect("first writer");
    assert!(matches!(
        RuntimeStore::open(project.path()),
        Err(RuntimeStoreError::Busy)
    ));
    drop(first);
    RuntimeStore::open(project.path()).expect("reopen after drop");
}

#[test]
fn acknowledged_records_and_rebuilt_indexes_survive_reopen() {
    let project = root();
    let mut policy = SessionMachine::new();
    let create = create_record(&mut policy, "durable");
    let old_index = {
        let mut store = RuntimeStore::open(project.path()).expect("open");
        store.append(create).expect("durable append");
        assert_eq!(store.machine().sessions().len(), 1);
        store.current_index_path().to_path_buf()
    };
    std::fs::remove_file(&old_index).expect("remove derived index");

    let reopened = RuntimeStore::open(project.path()).expect("rebuild index");
    assert_eq!(reopened.machine().sessions(), policy.sessions());
    assert!(reopened.current_index_path().is_file());
}

#[test]
fn same_boundary_index_hash_conflict_fails_without_touching_journal() {
    let project = root();
    let mut policy = SessionMachine::new();
    let create = create_record(&mut policy, "index-conflict");
    let (journal_path, index_path) = {
        let mut store = RuntimeStore::open(project.path()).expect("open");
        store.append(create).expect("append");
        (
            store.journal_path().to_path_buf(),
            store.current_index_path().to_path_buf(),
        )
    };
    let journal_before = std::fs::read(&journal_path).expect("journal before conflict");
    let mut index: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&index_path).expect("index")).expect("valid index");
    index["journalHash"] = serde_json::Value::String("0000000000000000".to_owned());
    std::fs::write(
        &index_path,
        serde_json::to_vec(&index).expect("conflicting index"),
    )
    .expect("write conflict");

    assert!(matches!(
        RuntimeStore::open(project.path()),
        Err(RuntimeStoreError::IndexConflict)
    ));
    assert_eq!(
        std::fs::read(journal_path).expect("journal after conflict"),
        journal_before
    );
}

#[test]
fn final_fragment_is_quarantined_trimmed_and_idempotent() {
    let project = root();
    let mut policy = SessionMachine::new();
    let create = create_record(&mut policy, "fragment");
    let journal_path = {
        let mut store = RuntimeStore::open(project.path()).expect("open");
        store.append(create).expect("append");
        store.journal_path().to_path_buf()
    };
    let acknowledged = std::fs::read(&journal_path).expect("read journal");
    let fragment = br#"{"schemaVersion":1,"recordId":"unfinished""#;
    OpenOptions::new()
        .append(true)
        .open(&journal_path)
        .expect("append fragment")
        .write_all(fragment)
        .expect("write fragment");

    let repair_dir = {
        let reopened = RuntimeStore::open(project.path()).expect("repair final fragment");
        assert_eq!(reopened.machine().sessions(), policy.sessions());
        reopened.repair_directory()
    };
    assert_eq!(std::fs::read(&journal_path).expect("trimmed"), acknowledged);
    let evidence = std::fs::read_dir(&repair_dir)
        .expect("repair evidence")
        .collect::<Result<Vec<_>, _>>()
        .expect("repair entries");
    assert_eq!(evidence.len(), 1);
    assert_eq!(
        std::fs::read(evidence[0].path()).expect("evidence"),
        fragment
    );

    RuntimeStore::open(project.path()).expect("idempotent reopen");
    assert_eq!(
        std::fs::read_dir(repair_dir)
            .expect("repairs")
            .collect::<Result<Vec<_>, _>>()
            .expect("repair entries")
            .len(),
        1
    );
}

#[test]
fn middle_corruption_and_invalid_utf8_fail_without_mutation() {
    let project = root();
    let journal_path = {
        let store = RuntimeStore::open(project.path()).expect("open");
        store.journal_path().to_path_buf()
    };
    std::fs::write(&journal_path, b"{not-json}\n").expect("corrupt middle line");
    let before = std::fs::read(&journal_path).expect("before");
    assert!(matches!(
        RuntimeStore::open(project.path()),
        Err(RuntimeStoreError::Recovery)
    ));
    assert_eq!(std::fs::read(&journal_path).expect("after"), before);

    std::fs::write(&journal_path, [0xff, 0xfe]).expect("invalid UTF-8");
    let before = std::fs::read(&journal_path).expect("before UTF-8");
    assert!(matches!(
        RuntimeStore::open(project.path()),
        Err(RuntimeStoreError::Recovery)
    ));
    assert_eq!(std::fs::read(&journal_path).expect("after UTF-8"), before);
}

#[test]
fn abandoned_turn_is_interrupted_exactly_once_across_restarts() {
    let project = root();
    let mut policy = SessionMachine::new();
    let create = create_record(&mut policy, "recovery");
    let start = start_record(&mut policy, "recovery");
    let journal_path = {
        let mut store = RuntimeStore::open(project.path()).expect("open");
        store.append(create).expect("create");
        store.append(start).expect("start");
        store.journal_path().to_path_buf()
    };

    {
        let recovered = RuntimeStore::open(project.path()).expect("recover");
        assert_eq!(
            recovered.machine().active_session().expect("active").turns[0].status,
            TurnStatus::Interrupted
        );
    }
    let once = std::fs::read(&journal_path).expect("journal after recovery");
    assert_eq!(once.iter().filter(|byte| **byte == b'\n').count(), 3);
    RuntimeStore::open(project.path()).expect("second recovery pass");
    assert_eq!(std::fs::read(&journal_path).expect("journal stable"), once);
}

#[test]
fn oversized_record_is_rejected_before_journal_mutation() {
    let project = root();
    let mut policy = SessionMachine::new();
    let create = create_record(&mut policy, "large");
    let start = start_record(&mut policy, "large");
    let mut store = RuntimeStore::open(project.path()).expect("open");
    store.append(create).expect("create");
    store.append(start).expect("start");
    let before = std::fs::read(store.journal_path()).expect("journal before large record");
    let delta = persisted(
        policy
            .apply(SessionCommand::RecordDelta {
                record_id: record_id("record-large-delta"),
                turn_id: TurnId::new("turn-large").expect("turn"),
                delta: "x".repeat(1024 * 1024),
                now_unix_ms: 3,
            })
            .expect("large delta policy"),
    );
    assert_eq!(store.append(delta), Err(RuntimeStoreError::RecordTooLarge));
    assert_eq!(
        std::fs::read(store.journal_path()).expect("journal after large record"),
        before
    );
}

#[test]
fn safe_trace_protocol_record_never_persists_adversarial_input() {
    let project = root();
    let mut policy = SessionMachine::new();
    let create = create_record(&mut policy, "trace");
    let session_id = policy.active_session().expect("active").session_id.clone();
    let entry = SafeTraceRecorder::record(
        5,
        TraceCode::ProviderFailed,
        BTreeMap::from([
            (
                "request_id".to_owned(),
                SafeTraceFact::String("DO_NOT_PERSIST_THIS sk-supersecret".to_owned()),
            ),
            (
                "raw_frame".to_owned(),
                SafeTraceFact::String("{\"choices\":[{\"delta\":{}}]}".to_owned()),
            ),
        ]),
    );
    let trace = SessionRecordV1::new(
        record_id("record-safe-trace"),
        JournalRecord::TraceStored { session_id, entry },
    );
    let journal_path = {
        let mut store = RuntimeStore::open(project.path()).expect("open");
        store.append(create).expect("create");
        store.append(trace).expect("trace");
        store.journal_path().to_path_buf()
    };
    let persisted = std::fs::read_to_string(journal_path).expect("journal text");
    for marker in [
        "DO_NOT_PERSIST_THIS",
        "sk-supersecret",
        "choices",
        "raw_frame",
    ] {
        assert!(!persisted.contains(marker));
    }
    assert!(persisted.contains("[REDACTED]"));
}

#[test]
fn approved_but_not_started_tool_recovers_cancelled_once_without_execution() {
    let project = root();
    let journal_path = append_abandoned_tool_prefix(&project, "pre-start", false);

    {
        let recovered = RuntimeStore::open(project.path()).expect("recover");
        let turn = &recovered.machine().active_session().expect("active").turns[0];
        let invocation = &turn.tool_invocations[0];
        assert_eq!(invocation.started_at_unix_ms, None);
        assert_eq!(
            invocation
                .terminal_result
                .as_ref()
                .map(|result| (result.status, result.code.as_str())),
            Some((ToolTerminalStatus::Cancelled, "recovered_before_start"))
        );
        assert_eq!(turn.status, TurnStatus::Interrupted);
    }

    let once = std::fs::read(&journal_path).expect("journal after recovery");
    RuntimeStore::open(project.path()).expect("stable reopen");
    assert_eq!(std::fs::read(journal_path).expect("stable journal"), once);
}

#[test]
fn requested_without_a_decision_recovers_cancelled_without_approval_or_execution() {
    let project = root();
    let mut policy = SessionMachine::new();
    let create = create_record(&mut policy, "requested-only");
    let start = start_record(&mut policy, "requested-only");
    let requested = persisted(
        policy
            .apply(SessionCommand::RecordToolRequested {
                record_id: record_id("record-requested-only-tool"),
                turn_id: TurnId::new("turn-requested-only").expect("turn"),
                invocation: invocation("call-requested-only"),
                now_unix_ms: 3,
            })
            .expect("request"),
    );
    {
        let mut store = RuntimeStore::open(project.path()).expect("open");
        for record in [create, start, requested] {
            store.append(record).expect("append");
        }
    }

    let recovered = RuntimeStore::open(project.path()).expect("recover");
    let invocation =
        &recovered.machine().active_session().expect("active").turns[0].tool_invocations[0];
    assert!(invocation.decision.is_none());
    assert!(invocation.started_at_unix_ms.is_none());
    assert_eq!(
        invocation
            .terminal_result
            .as_ref()
            .map(|result| result.status),
        Some(ToolTerminalStatus::Cancelled)
    );
}

#[test]
fn started_tool_recovers_indeterminate_once_and_never_claims_success() {
    let project = root();
    let journal_path = append_abandoned_tool_prefix(&project, "started", true);

    {
        let recovered = RuntimeStore::open(project.path()).expect("recover");
        let invocation =
            &recovered.machine().active_session().expect("active").turns[0].tool_invocations[0];
        assert_eq!(invocation.started_at_unix_ms, Some(5));
        assert_eq!(
            invocation
                .terminal_result
                .as_ref()
                .map(|result| (result.status, result.code.as_str())),
            Some((ToolTerminalStatus::Indeterminate, "effect_unknown"))
        );
    }

    let once = std::fs::read(&journal_path).expect("journal after recovery");
    RuntimeStore::open(project.path()).expect("stable reopen");
    assert_eq!(std::fs::read(journal_path).expect("stable journal"), once);
}

#[test]
fn duplicate_record_id_is_idempotent_only_for_identical_payload() {
    let project = root();
    let mut policy = SessionMachine::new();
    let original = create_record(&mut policy, "duplicate-payload");
    let mut store = RuntimeStore::open(project.path()).expect("open");
    store.append(original.clone()).expect("append original");
    store.append(original.clone()).expect("identical duplicate");

    let conflicting = SessionRecordV1::new(
        original.record_id,
        JournalRecord::TraceStored {
            session_id: SessionId::new("session-duplicate-payload").expect("session"),
            entry: minimax_protocol::TraceEntry {
                recorded_at_unix_ms: 9,
                code: TraceCode::CommandRejected,
                facts: BTreeMap::new(),
            },
        },
    );
    assert_eq!(store.append(conflicting), Err(RuntimeStoreError::Recovery));
}

#[test]
fn journal_load_accepts_identical_record_replay_and_rejects_conflicting_payload() {
    let identical_project = root();
    let mut policy = SessionMachine::new();
    let original = create_record(&mut policy, "physical-replay");
    let journal_path = {
        let mut store = RuntimeStore::open(identical_project.path()).expect("open");
        store.append(original.clone()).expect("append");
        store.journal_path().to_path_buf()
    };
    let mut bytes = serde_json::to_vec(&original).expect("serialize replay");
    bytes.push(b'\n');
    OpenOptions::new()
        .append(true)
        .open(&journal_path)
        .expect("open journal")
        .write_all(&bytes)
        .expect("append replay");
    RuntimeStore::open(identical_project.path()).expect("identical replay is idempotent");

    let conflicting_project = root();
    let mut policy = SessionMachine::new();
    let original = create_record(&mut policy, "physical-conflict");
    let journal_path = {
        let mut store = RuntimeStore::open(conflicting_project.path()).expect("open");
        store.append(original.clone()).expect("append");
        store.journal_path().to_path_buf()
    };
    let conflicting = SessionRecordV1::new(
        original.record_id,
        JournalRecord::TraceStored {
            session_id: SessionId::new("session-physical-conflict").expect("session"),
            entry: minimax_protocol::TraceEntry {
                recorded_at_unix_ms: 9,
                code: TraceCode::CommandRejected,
                facts: BTreeMap::new(),
            },
        },
    );
    let mut bytes = serde_json::to_vec(&conflicting).expect("serialize conflict");
    bytes.push(b'\n');
    OpenOptions::new()
        .append(true)
        .open(&journal_path)
        .expect("open journal")
        .write_all(&bytes)
        .expect("append conflict");
    assert!(matches!(
        RuntimeStore::open(conflicting_project.path()),
        Err(RuntimeStoreError::Recovery)
    ));
}

#[test]
fn terminal_tool_record_can_be_replayed_without_duplicate_journal_bytes() {
    let project = root();
    let mut policy = SessionMachine::new();
    let create = create_record(&mut policy, "terminal-replay");
    let start = start_record(&mut policy, "terminal-replay");
    let turn_id = TurnId::new("turn-terminal-replay").expect("turn");
    let requested = persisted(
        policy
            .apply(SessionCommand::RecordToolRequested {
                record_id: record_id("record-terminal-replay-request"),
                turn_id: turn_id.clone(),
                invocation: invocation("call-terminal-replay"),
                now_unix_ms: 3,
            })
            .expect("request"),
    );
    let terminal = persisted(
        policy
            .apply(SessionCommand::RecordToolTerminal {
                record_id: record_id("record-terminal-replay-result"),
                turn_id,
                result: ToolResult {
                    schema_version: SchemaVersion,
                    call_id: ToolCallId::new("call-terminal-replay").expect("call"),
                    tool_name: "read_file".to_owned(),
                    status: ToolTerminalStatus::Failed,
                    code: "preflight_denied".to_owned(),
                    output: None,
                },
                now_unix_ms: 4,
            })
            .expect("terminal"),
    );
    let mut store = RuntimeStore::open(project.path()).expect("open");
    for record in [create, start, requested, terminal.clone()] {
        store.append(record).expect("append");
    }
    let before = std::fs::read(store.journal_path()).expect("before replay");
    store.append(terminal).expect("identical replay");
    assert_eq!(
        std::fs::read(store.journal_path()).expect("after replay"),
        before
    );
}
