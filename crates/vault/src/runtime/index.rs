use std::fs::{OpenOptions, read_dir};
use std::io::Write;
use std::path::{Path, PathBuf};

use minimax_core::SessionMachine;
use minimax_protocol::{SessionId, SessionStatus};
use serde::{Deserialize, Serialize};

use super::RuntimeStoreError;
use super::journal::{JournalSnapshot, MAX_RECORD_BYTES, RuntimeJournal};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RuntimeIndexV1 {
    schema_version: u32,
    journal_len: u64,
    record_count: u64,
    journal_hash: String,
    sessions: Vec<IndexSession>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct IndexSession {
    session_id: SessionId,
    status: SessionStatus,
    updated_at_unix_ms: u64,
    turn_count: u64,
}

pub(crate) struct RuntimeIndex;

impl RuntimeIndex {
    pub(crate) fn inspect_read_only(
        runtime_dir: &Path,
        journal: &JournalSnapshot,
        machine: &SessionMachine,
    ) -> Result<(), RuntimeStoreError> {
        let index_dir = runtime_dir.join("indexes");
        let expected = RuntimeIndexV1::from_snapshot(journal, machine)?;
        let mut matched = false;
        for entry in read_dir(&index_dir).map_err(|_| RuntimeStoreError::IndexConflict)? {
            let path = entry.map_err(|_| RuntimeStoreError::IndexConflict)?.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let bytes = std::fs::read(&path).map_err(|_| RuntimeStoreError::Io)?;
            if bytes.len() > MAX_RECORD_BYTES {
                return Err(RuntimeStoreError::IndexTooLarge);
            }
            let existing: RuntimeIndexV1 =
                serde_json::from_slice(&bytes).map_err(|_| RuntimeStoreError::IndexConflict)?;
            if existing.schema_version != 1 {
                return Err(RuntimeStoreError::IndexConflict);
            }
            if existing.journal_len == expected.journal_len
                && existing.record_count == expected.record_count
            {
                if existing != expected {
                    return Err(RuntimeStoreError::IndexConflict);
                }
                matched = true;
            }
        }
        if matched {
            Ok(())
        } else {
            Err(RuntimeStoreError::IndexConflict)
        }
    }

    pub(crate) fn ensure(
        runtime_dir: &Path,
        journal: &RuntimeJournal,
        machine: &SessionMachine,
    ) -> Result<PathBuf, RuntimeStoreError> {
        let index_dir = runtime_dir.join("indexes");
        std::fs::create_dir_all(&index_dir).map_err(|_| RuntimeStoreError::Io)?;
        let expected = RuntimeIndexV1::from_state(journal, machine)?;
        let mut found_expected = None;

        for entry in read_dir(&index_dir).map_err(|_| RuntimeStoreError::Io)? {
            let path = entry.map_err(|_| RuntimeStoreError::Io)?.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let bytes = std::fs::read(&path).map_err(|_| RuntimeStoreError::Io)?;
            if bytes.len() > MAX_RECORD_BYTES {
                return Err(RuntimeStoreError::IndexTooLarge);
            }
            let existing: RuntimeIndexV1 =
                serde_json::from_slice(&bytes).map_err(|_| RuntimeStoreError::IndexConflict)?;
            if existing.schema_version != 1 {
                return Err(RuntimeStoreError::IndexConflict);
            }
            if existing.journal_len == expected.journal_len
                && existing.record_count == expected.record_count
            {
                if existing != expected {
                    return Err(RuntimeStoreError::IndexConflict);
                }
                found_expected = Some(path);
            }
        }

        if let Some(path) = found_expected {
            return Ok(path);
        }
        Self::publish(&index_dir, &expected)
    }

    fn publish(index_dir: &Path, index: &RuntimeIndexV1) -> Result<PathBuf, RuntimeStoreError> {
        let final_path = index_dir.join(format!(
            "index-{}-{}-{}.json",
            index.journal_hash, index.journal_len, index.record_count
        ));
        let pending_path = index_dir.join(format!(
            ".pending-{}-{}-{}",
            index.journal_hash, index.journal_len, index.record_count
        ));
        let mut bytes = serde_json::to_vec(index).map_err(|_| RuntimeStoreError::Recovery)?;
        bytes.push(b'\n');
        if bytes.len() > MAX_RECORD_BYTES {
            return Err(RuntimeStoreError::IndexTooLarge);
        }
        let mut pending = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&pending_path)
            .map_err(|_| RuntimeStoreError::Io)?;
        pending
            .write_all(&bytes)
            .and_then(|()| pending.flush())
            .and_then(|()| pending.sync_all())
            .map_err(|_| RuntimeStoreError::Io)?;
        drop(pending);
        match std::fs::rename(&pending_path, &final_path) {
            Ok(()) => Ok(final_path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let existing = std::fs::read(&final_path).map_err(|_| RuntimeStoreError::Io)?;
                if existing == bytes {
                    Ok(final_path)
                } else {
                    Err(RuntimeStoreError::IndexConflict)
                }
            }
            Err(_) => Err(RuntimeStoreError::Io),
        }
    }
}

impl RuntimeIndexV1 {
    fn from_state(
        journal: &RuntimeJournal,
        machine: &SessionMachine,
    ) -> Result<Self, RuntimeStoreError> {
        Ok(Self {
            schema_version: 1,
            journal_len: journal.len(),
            record_count: journal.record_count(),
            journal_hash: format!("{:016x}", journal.hash()),
            sessions: index_sessions(machine)?,
        })
    }

    fn from_snapshot(
        journal: &JournalSnapshot,
        machine: &SessionMachine,
    ) -> Result<Self, RuntimeStoreError> {
        Ok(Self {
            schema_version: 1,
            journal_len: journal.len,
            record_count: journal.record_count,
            journal_hash: format!("{:016x}", journal.hash),
            sessions: index_sessions(machine)?,
        })
    }
}

fn index_sessions(machine: &SessionMachine) -> Result<Vec<IndexSession>, RuntimeStoreError> {
    machine
        .sessions()
        .values()
        .map(|session| {
            Ok(IndexSession {
                session_id: session.session_id.clone(),
                status: session.status,
                updated_at_unix_ms: session.updated_at_unix_ms,
                turn_count: u64::try_from(session.turns.len())
                    .map_err(|_| RuntimeStoreError::IndexTooLarge)?,
            })
        })
        .collect()
}
