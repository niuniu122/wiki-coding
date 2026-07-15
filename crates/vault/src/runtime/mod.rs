mod index;
mod journal;
mod lease;
mod recovery;

use std::collections::BTreeSet;
use std::fmt;
use std::path::{Path, PathBuf};

use minimax_core::{SessionCommand, SessionEffect, SessionMachine};
use minimax_protocol::{RecordId, RuntimeErrorCode, SessionRecordV1};

use self::index::RuntimeIndex;
use self::journal::{JournalLoad, RuntimeJournal};
use self::lease::WorkspaceLease;
use self::recovery::recover_abandoned_turns;

pub const RUNTIME_DIRECTORY: &str = ".minimax/runtime/v1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeStoreError {
    Busy,
    Io,
    Recovery,
    RecordTooLarge,
    IndexTooLarge,
    IndexConflict,
    Command(RuntimeErrorCode),
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
    record_ids: BTreeSet<RecordId>,
    current_index: PathBuf,
}

impl RuntimeStore {
    pub fn open(project_root: impl AsRef<Path>) -> Result<Self, RuntimeStoreError> {
        let runtime_dir = project_root.as_ref().join(RUNTIME_DIRECTORY);
        std::fs::create_dir_all(&runtime_dir).map_err(|_| RuntimeStoreError::Io)?;
        let lease = WorkspaceLease::acquire(&runtime_dir)?;
        let mut journal = RuntimeJournal::open(&runtime_dir)?;
        let JournalLoad { records, .. } = journal.load()?;
        let mut machine =
            SessionMachine::replay(records).map_err(|_| RuntimeStoreError::Recovery)?;

        for record in recover_abandoned_turns(&mut machine)? {
            journal.append(&record)?;
        }

        let record_ids = journal.record_ids().clone();
        let current_index = RuntimeIndex::ensure(&runtime_dir, &journal, &machine)?;
        Ok(Self {
            runtime_dir,
            _lease: lease,
            journal,
            machine,
            record_ids,
            current_index,
        })
    }

    pub fn append(&mut self, record: SessionRecordV1) -> Result<(), RuntimeStoreError> {
        if self.record_ids.contains(&record.record_id) {
            return Ok(());
        }
        let mut next = self.machine.clone();
        next.apply(SessionCommand::Replay(record.clone()))
            .map_err(|_| RuntimeStoreError::Recovery)?;
        self.journal.append(&record)?;
        self.record_ids.insert(record.record_id.clone());
        self.machine = next;
        self.current_index = RuntimeIndex::ensure(&self.runtime_dir, &self.journal, &self.machine)?;
        Ok(())
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
    pub fn repair_directory(&self) -> PathBuf {
        self.runtime_dir.join("repairs")
    }
}
