use std::fs::OpenOptions;
use std::io::Write;

use minimax_core::{SessionCommand, SessionEffect, SessionMachine};
use minimax_protocol::{
    ModelBinding, ModelId, ProviderId, ProviderProtocolKind, RecordId, RequestId, SessionId,
    SessionRecordV1, TurnId, TurnStatus,
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
