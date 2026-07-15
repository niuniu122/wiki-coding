use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use minimax_protocol::{RecordId, SessionRecordV1, parse_session_record_v1};

use super::RuntimeStoreError;

pub(crate) const MAX_RECORD_BYTES: usize = 1024 * 1024;
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

pub(crate) struct JournalLoad {
    pub(crate) records: Vec<SessionRecordV1>,
}

pub(crate) struct RuntimeJournal {
    path: PathBuf,
    file: File,
    len: u64,
    record_count: u64,
    hash: u64,
    record_ids: BTreeSet<RecordId>,
}

impl RuntimeJournal {
    pub(crate) fn open(runtime_dir: &Path) -> Result<Self, RuntimeStoreError> {
        let path = runtime_dir.join("sessions.jsonl");
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|_| RuntimeStoreError::Io)?;
        Ok(Self {
            path,
            file,
            len: 0,
            record_count: 0,
            hash: FNV_OFFSET,
            record_ids: BTreeSet::new(),
        })
    }

    pub(crate) fn load(&mut self) -> Result<JournalLoad, RuntimeStoreError> {
        self.file
            .seek(SeekFrom::Start(0))
            .map_err(|_| RuntimeStoreError::Io)?;
        let mut reader = BufReader::new(&self.file);
        let mut records = Vec::new();
        let mut records_by_id = BTreeMap::new();
        let mut complete_len = 0_u64;
        let mut hash = FNV_OFFSET;

        loop {
            let mut line = Vec::new();
            let read = reader
                .read_until(b'\n', &mut line)
                .map_err(|_| RuntimeStoreError::Io)?;
            if read == 0 {
                break;
            }
            if line.len() > MAX_RECORD_BYTES {
                return Err(RuntimeStoreError::RecordTooLarge);
            }
            std::str::from_utf8(&line).map_err(|_| RuntimeStoreError::Recovery)?;
            if !line.ends_with(b"\n") {
                drop(reader);
                self.quarantine_and_trim(&line, complete_len)?;
                break;
            }
            let json = std::str::from_utf8(&line[..line.len() - 1])
                .map_err(|_| RuntimeStoreError::Recovery)?;
            let record = parse_session_record_v1(json).map_err(|_| RuntimeStoreError::Recovery)?;
            if let Some(existing) = records_by_id.get(&record.record_id) {
                if existing != &record {
                    return Err(RuntimeStoreError::Recovery);
                }
            } else {
                records_by_id.insert(record.record_id.clone(), record.clone());
            }
            hash = hash_bytes(hash, &line);
            complete_len = complete_len
                .checked_add(line.len() as u64)
                .ok_or(RuntimeStoreError::Recovery)?;
            records.push(record);
        }

        self.file
            .seek(SeekFrom::End(0))
            .map_err(|_| RuntimeStoreError::Io)?;
        self.len = complete_len;
        self.record_count = records.len() as u64;
        self.hash = hash;
        self.record_ids = records_by_id.into_keys().collect();
        Ok(JournalLoad { records })
    }

    pub(crate) fn append(&mut self, record: &SessionRecordV1) -> Result<(), RuntimeStoreError> {
        if self.record_ids.contains(&record.record_id) {
            return Ok(());
        }
        let mut bytes = serde_json::to_vec(record).map_err(|_| RuntimeStoreError::Recovery)?;
        bytes.push(b'\n');
        if bytes.len() > MAX_RECORD_BYTES {
            return Err(RuntimeStoreError::RecordTooLarge);
        }
        self.file
            .seek(SeekFrom::End(0))
            .and_then(|_| self.file.write_all(&bytes))
            .and_then(|()| self.file.flush())
            .and_then(|()| self.file.sync_all())
            .map_err(|_| RuntimeStoreError::Io)?;
        self.len = self
            .len
            .checked_add(bytes.len() as u64)
            .ok_or(RuntimeStoreError::Recovery)?;
        self.record_count = self
            .record_count
            .checked_add(1)
            .ok_or(RuntimeStoreError::Recovery)?;
        self.hash = hash_bytes(self.hash, &bytes);
        self.record_ids.insert(record.record_id.clone());
        Ok(())
    }

    fn quarantine_and_trim(
        &mut self,
        fragment: &[u8],
        complete_len: u64,
    ) -> Result<(), RuntimeStoreError> {
        let runtime_dir = self.path.parent().ok_or(RuntimeStoreError::Recovery)?;
        let repair_dir = runtime_dir.join("repairs");
        std::fs::create_dir_all(&repair_dir).map_err(|_| RuntimeStoreError::Io)?;
        let repair_path = repair_dir.join(format!("final-fragment-{complete_len}.partial"));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&repair_path)
        {
            Ok(mut evidence) => {
                evidence
                    .write_all(fragment)
                    .and_then(|()| evidence.flush())
                    .and_then(|()| evidence.sync_all())
                    .map_err(|_| RuntimeStoreError::Io)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let existing = std::fs::read(&repair_path).map_err(|_| RuntimeStoreError::Io)?;
                if existing != fragment {
                    return Err(RuntimeStoreError::Recovery);
                }
            }
            Err(_) => return Err(RuntimeStoreError::Io),
        }
        self.file
            .set_len(complete_len)
            .and_then(|()| self.file.sync_all())
            .map_err(|_| RuntimeStoreError::Io)
    }

    #[must_use]
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub(crate) const fn len(&self) -> u64 {
        self.len
    }

    #[must_use]
    pub(crate) const fn record_count(&self) -> u64 {
        self.record_count
    }

    #[must_use]
    pub(crate) const fn hash(&self) -> u64 {
        self.hash
    }
}

pub(crate) fn stable_hash(bytes: &[u8]) -> u64 {
    hash_bytes(FNV_OFFSET, bytes)
}

fn hash_bytes(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}
