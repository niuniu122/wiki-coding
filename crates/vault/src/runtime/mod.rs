mod index;
mod journal;
pub(crate) mod lease;
mod recovery;

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use minimax_core::{SessionCommand, SessionEffect, SessionMachine};
use minimax_protocol::{RecordId, RuntimeErrorCode, SessionId, SessionRecordV1, TraceEntry};

use crate::{FinalizedSessionEvidence, ProjectVault, VaultError};

use self::index::RuntimeIndex;
use self::journal::{JournalLoad, RuntimeJournal};
use self::lease::WorkspaceLease;
use self::recovery::{recover_abandoned_invocations, recover_abandoned_turns};

pub const RUNTIME_DIRECTORY: &str = ".minimax/runtime/v1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeStoreError {
    Busy,
    Io,
    Recovery,
    RecordTooLarge,
    IndexTooLarge,
    IndexConflict,
    Finalized,
    Command(RuntimeErrorCode),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeInspection {
    Uninitialized,
    Healthy,
}

impl fmt::Display for RuntimeStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Busy => "the project runtime is already open for writing",
            Self::Io => "the project runtime could not complete a local file operation",
            Self::Recovery => "the project runtime journal could not be recovered safely",
            Self::RecordTooLarge => "a runtime journal record exceeds the one MiB limit",
            Self::IndexTooLarge => "the derived runtime index exceeds the one MiB limit",
            Self::IndexConflict => "the derived runtime index conflicts with the journal",
            Self::Finalized => "the session is finalized and cannot accept more runtime records",
            Self::Command(code) => return code.fmt(formatter),
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for RuntimeStoreError {}

pub struct RuntimeStore {
    runtime_dir: PathBuf,
    _lease: WorkspaceLease,
    journal: RuntimeJournal,
    machine: SessionMachine,
    records_by_id: BTreeMap<RecordId, SessionRecordV1>,
    current_index: PathBuf,
}

impl RuntimeStore {
    pub fn inspect_read_only(
        project_root: impl AsRef<Path>,
    ) -> Result<RuntimeInspection, RuntimeStoreError> {
        let project_root = project_root
            .as_ref()
            .canonicalize()
            .map_err(|_| RuntimeStoreError::Io)?;
        let runtime_dir = project_root.join(RUNTIME_DIRECTORY);
        if !runtime_dir.exists() {
            return Ok(RuntimeInspection::Uninitialized);
        }
        if !runtime_dir.is_dir() {
            return Err(RuntimeStoreError::Io);
        }
        let _inspection_lease = WorkspaceLease::probe(&runtime_dir.join("writer.lock"))?
            .ok_or(RuntimeStoreError::Recovery)?;
        let snapshot = journal::inspect_read_only(&runtime_dir.join("sessions.jsonl"))?;
        let machine = SessionMachine::replay(snapshot.records.clone())
            .map_err(|_| RuntimeStoreError::Recovery)?;
        RuntimeIndex::inspect_read_only(&runtime_dir, &snapshot, &machine)?;
        Ok(RuntimeInspection::Healthy)
    }

    pub fn open(project_root: impl AsRef<Path>) -> Result<Self, RuntimeStoreError> {
        let project_root = project_root
            .as_ref()
            .canonicalize()
            .map_err(|_| RuntimeStoreError::Io)?;
        let runtime_dir = project_root.join(RUNTIME_DIRECTORY);
        std::fs::create_dir_all(&runtime_dir).map_err(|_| RuntimeStoreError::Io)?;
        let lease = WorkspaceLease::acquire(&runtime_dir)?;
        let mut journal = RuntimeJournal::open(&runtime_dir)?;
        let JournalLoad { records, .. } = journal.load()?;
        let mut records_by_id = records
            .iter()
            .cloned()
            .map(|record| (record.record_id.clone(), record))
            .collect::<BTreeMap<_, _>>();
        let mut machine =
            SessionMachine::replay(records).map_err(|_| RuntimeStoreError::Recovery)?;

        for record in recover_abandoned_invocations(&mut machine)? {
            journal.append(&record)?;
            records_by_id.insert(record.record_id.clone(), record);
        }
        for record in recover_abandoned_turns(&mut machine)? {
            journal.append(&record)?;
            records_by_id.insert(record.record_id.clone(), record);
        }

        let current_index = RuntimeIndex::ensure(&runtime_dir, &journal, &machine)?;
        Ok(Self {
            runtime_dir,
            _lease: lease,
            journal,
            machine,
            records_by_id,
            current_index,
        })
    }

    pub fn append(&mut self, record: SessionRecordV1) -> Result<(), RuntimeStoreError> {
        if let Some(session_id) = record_session_id(&record)
            && crate::raw::finalization_marker_path(&self.runtime_dir, session_id).is_file()
        {
            return Err(RuntimeStoreError::Finalized);
        }
        if let Some(existing) = self.records_by_id.get(&record.record_id) {
            return if existing == &record {
                Ok(())
            } else {
                Err(RuntimeStoreError::Recovery)
            };
        }
        let mut next = self.machine.clone();
        next.apply(SessionCommand::Replay(record.clone()))
            .map_err(|_| RuntimeStoreError::Recovery)?;
        self.journal.append(&record)?;
        self.records_by_id.insert(record.record_id.clone(), record);
        self.machine = next;
        self.current_index = RuntimeIndex::ensure(&self.runtime_dir, &self.journal, &self.machine)?;
        Ok(())
    }

    #[must_use]
    pub fn session_is_finalized(&self, session_id: &SessionId) -> bool {
        crate::raw::finalization_marker_path(&self.runtime_dir, session_id).is_file()
    }

    pub fn finalize_session(
        &self,
        vault: &ProjectVault,
        session_id: &SessionId,
        finalized_at_unix_ms: u64,
    ) -> Result<FinalizedSessionEvidence, VaultError> {
        crate::raw::finalize_runtime_session_from_open_store(
            &self.runtime_dir,
            self.journal.path(),
            vault,
            session_id,
            finalized_at_unix_ms,
        )
    }

    pub fn apply_command(
        &mut self,
        command: SessionCommand,
    ) -> Result<Vec<SessionEffect>, RuntimeStoreError> {
        let mut preview = self.machine.clone();
        let effects = preview.apply(command).map_err(RuntimeStoreError::Command)?;
        for effect in &effects {
            if let SessionEffect::Persist(record) = effect {
                self.append(record.clone())?;
            }
        }
        Ok(effects)
    }

    #[must_use]
    pub fn machine(&self) -> &SessionMachine {
        &self.machine
    }

    #[must_use]
    pub fn journal_path(&self) -> &Path {
        self.journal.path()
    }

    #[must_use]
    pub fn current_index_path(&self) -> &Path {
        &self.current_index
    }

    #[must_use]
    pub fn trace_entries(&self, session_id: &SessionId) -> Vec<TraceEntry> {
        let mut entries = self
            .records_by_id
            .values()
            .filter_map(|record| match &record.record {
                minimax_protocol::JournalRecord::TraceStored {
                    session_id: stored_session_id,
                    entry,
                } if stored_session_id == session_id => Some(entry.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.recorded_at_unix_ms);
        entries
    }

    #[must_use]
    pub fn repair_directory(&self) -> PathBuf {
        self.runtime_dir.join("repairs")
    }
}

pub(crate) fn record_session_id(record: &SessionRecordV1) -> Option<&minimax_protocol::SessionId> {
    use minimax_protocol::JournalRecord;

    match &record.record {
        JournalRecord::SessionCreated { session } => Some(&session.session_id),
        JournalRecord::SessionActivated { session_id, .. }
        | JournalRecord::TurnStarted { session_id, .. }
        | JournalRecord::TurnDelta { session_id, .. }
        | JournalRecord::TurnTerminal { session_id, .. }
        | JournalRecord::ToolRequested { session_id, .. }
        | JournalRecord::ToolDecisionRecorded { session_id, .. }
        | JournalRecord::ToolStarted { session_id, .. }
        | JournalRecord::ToolTerminal { session_id, .. }
        | JournalRecord::RecoveryApplied { session_id, .. }
        | JournalRecord::CompactionStored { session_id, .. }
        | JournalRecord::TraceStored { session_id, .. } => Some(session_id),
    }
}
