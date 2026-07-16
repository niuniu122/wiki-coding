use std::fs::OpenOptions;
use std::io::Write;

use minimax_core::{SessionCommand, SessionEffect, SessionMachine};
use minimax_protocol::{
    ModelBinding, ModelId, ProjectId, ProviderId, ProviderProtocolKind, RecordId, RequestId,
    RuntimeTerminalOutcome, SessionId, SessionRecordV1, TurnId, TurnReceipt,
};
use minimax_vault::{
    ProjectVault, RuntimeStore, RuntimeStoreError, VaultError, finalize_runtime_session,
};

fn persisted(effects: Vec<SessionEffect>) -> SessionRecordV1 {
    effects
        .into_iter()
        .find_map(|effect| match effect {
            SessionEffect::Persist(record) => Some(record),
            _ => None,
        })
        .expect("persist effect")
}

fn records(user_input: &str) -> (SessionId, Vec<SessionRecordV1>) {
    let mut machine = SessionMachine::new();
    let session_id = SessionId::new("session-final").expect("session");
    let create = persisted(
        machine
            .apply(SessionCommand::Create {
                record_id: RecordId::new("create").expect("record"),
                session_id: session_id.clone(),
                binding: ModelBinding {
                    provider_id: ProviderId::new("provider:test").expect("provider"),
                    model_id: ModelId::new("model-test").expect("model"),
                    protocol: ProviderProtocolKind::Responses,
                },
                now_unix_ms: 1,
            })
            .expect("create"),
    );
    let turn_id = TurnId::new("turn-final").expect("turn");
    let request_id = RequestId::new("request-final").expect("request");
    let start = persisted(
        machine
            .apply(SessionCommand::Continue {
                record_id: RecordId::new("start").expect("record"),
                turn_id: turn_id.clone(),
                request_id: request_id.clone(),
                user_input: user_input.to_owned(),
                max_output_tokens: 128,
                now_unix_ms: 2,
            })
            .expect("start"),
    );
    let terminal = persisted(
        machine
            .apply(SessionCommand::Finalize {
                record_id: RecordId::new("terminal").expect("record"),
                receipt: TurnReceipt {
                    session_id: session_id.clone(),
                    turn_id,
                    request_id,
                    outcome: RuntimeTerminalOutcome::Completed,
                    usage: None,
                },
                assistant_content: Some("durable answer".to_owned()),
                now_unix_ms: 3,
            })
            .expect("terminal"),
    );
    (session_id, vec![create, start, terminal])
}

fn setup() -> (tempfile::TempDir, tempfile::TempDir, ProjectVault) {
    let project = tempfile::tempdir().expect("project");
    let vault_root = tempfile::tempdir().expect("vault");
    let vault = ProjectVault::bootstrap(
        project.path(),
        vault_root.path(),
        ProjectId::new("project").expect("project ID"),
        1,
    )
    .expect("bootstrap");
    (project, vault_root, vault)
}

#[test]
fn terminal_session_finalizes_once_and_blocks_later_append() {
    let (project, _vault_root, vault) = setup();
    let (session_id, session_records) = records("remember this architecture decision");
    {
        let mut store = RuntimeStore::open(project.path()).expect("runtime");
        for record in session_records {
            store.append(record).expect("append");
        }
    }
    let first =
        finalize_runtime_session(project.path(), &vault, &session_id, 10).expect("finalize");
    let second = finalize_runtime_session(project.path(), &vault, &session_id, 10)
        .expect("idempotent finalize");
    assert_eq!(first, second);
    let raw_dir = vault.root().join("raw/sessions");
    let entries = std::fs::read_dir(raw_dir)
        .expect("raw sessions")
        .collect::<Result<Vec<_>, _>>()
        .expect("entries");
    assert_eq!(entries.len(), 1);
    assert!(entries[0].path().join("session.json").is_file());
    assert!(entries[0].path().join("events.jsonl").is_file());

    let mut store = RuntimeStore::open(project.path()).expect("runtime reopen");
    let (_, later) = records("another request");
    assert!(matches!(
        store.append(later[1].clone()),
        Err(RuntimeStoreError::Finalized)
    ));
}

#[test]
fn final_fragment_is_quarantined_but_middle_corruption_fails_closed() {
    let (project, _vault_root, vault) = setup();
    let (session_id, session_records) = records("safe evidence");
    let journal = {
        let mut store = RuntimeStore::open(project.path()).expect("runtime");
        for record in session_records {
            store.append(record).expect("append");
        }
        store.journal_path().to_path_buf()
    };
    OpenOptions::new()
        .append(true)
        .open(&journal)
        .expect("journal")
        .write_all(b"{unfinished")
        .expect("fragment");
    finalize_runtime_session(project.path(), &vault, &session_id, 10).expect("repair and finalize");
    assert!(
        std::fs::read_dir(vault.root().join(".minimax/recovery"))
            .expect("recovery")
            .next()
            .is_some()
    );

    let (project, _vault_root, vault) = setup();
    let (session_id, _) = records("safe evidence");
    let runtime_dir = project.path().join(".minimax/runtime/v1");
    std::fs::create_dir_all(&runtime_dir).expect("runtime dir");
    std::fs::write(runtime_dir.join("sessions.jsonl"), b"{bad}\n").expect("corrupt");
    assert!(matches!(
        finalize_runtime_session(project.path(), &vault, &session_id, 10),
        Err(VaultError::RecoveryRequired)
    ));
}

#[test]
fn nonterminal_and_sensitive_sessions_never_publish_raw() {
    let (project, _vault_root, vault) = setup();
    let (session_id, session_records) = records("api_key=abcdefghijklmnop");
    {
        let mut store = RuntimeStore::open(project.path()).expect("runtime");
        for record in session_records {
            store.append(record).expect("append");
        }
    }
    assert!(matches!(
        finalize_runtime_session(project.path(), &vault, &session_id, 10),
        Err(VaultError::SensitiveContent)
    ));
    assert_eq!(
        std::fs::read_dir(vault.root().join("raw/sessions"))
            .expect("raw sessions")
            .count(),
        0
    );
}
