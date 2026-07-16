use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use minimax_protocol::{
    KnowledgeEvaluationJob, KnowledgePatch, KnowledgeReceipt, KnowledgeReceiptOutcome,
    ModelBinding, SchemaVersion, Usage, WikiWorkflowEvent,
};
use serde::{Deserialize, Serialize};

use crate::bootstrap::{ProjectVault, VaultError};
use crate::path::{atomic_create_or_same, sha256_hex};
use crate::raw::FinalizedSessionEvidence;

const MAX_WORKFLOW_RECORD_BYTES: usize = 512 * 1024;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum StoredGeneration {
    Started {
        attempt: u8,
        model_binding: ModelBinding,
    },
    Accepted {
        attempt: u8,
        model_binding: ModelBinding,
        patch: KnowledgePatch,
        usage: Usage,
    },
    SchemaRejected {
        attempt: u8,
        model_binding: ModelBinding,
        code: String,
        usage: Usage,
    },
    UnsafeRejected {
        attempt: u8,
        model_binding: ModelBinding,
        code: String,
        usage: Usage,
    },
    Unavailable {
        attempt: u8,
        model_binding: ModelBinding,
        code: String,
    },
}

impl StoredGeneration {
    #[must_use]
    pub const fn attempt(&self) -> u8 {
        match self {
            Self::Started { attempt, .. }
            | Self::Accepted { attempt, .. }
            | Self::SchemaRejected { attempt, .. }
            | Self::UnsafeRejected { attempt, .. }
            | Self::Unavailable { attempt, .. } => *attempt,
        }
    }

    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        !matches!(self, Self::Started { .. })
    }

    #[must_use]
    pub const fn model_binding(&self) -> &ModelBinding {
        match self {
            Self::Started { model_binding, .. }
            | Self::Accepted { model_binding, .. }
            | Self::SchemaRejected { model_binding, .. }
            | Self::UnsafeRejected { model_binding, .. }
            | Self::Unavailable { model_binding, .. } => model_binding,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum WorkflowRecord {
    Event { event: WikiWorkflowEvent },
    Generation { generation: StoredGeneration },
    Receipt { receipt: KnowledgeReceipt },
    Rebind { model_binding: ModelBinding },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WorkflowJournalRecord {
    schema_version: SchemaVersion,
    sequence: u32,
    record: WorkflowRecord,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct KnowledgeWorkflowHistory {
    records: Vec<WorkflowRecord>,
}

impl KnowledgeWorkflowHistory {
    #[must_use]
    pub fn receipt(&self) -> Option<&KnowledgeReceipt> {
        self.records.iter().rev().find_map(|record| match record {
            WorkflowRecord::Receipt { receipt } => Some(receipt),
            _ => None,
        })
    }

    #[must_use]
    pub fn terminal_generation(
        &self,
        attempt: u8,
        model_binding: &ModelBinding,
    ) -> Option<&StoredGeneration> {
        self.records.iter().rev().find_map(|record| match record {
            WorkflowRecord::Generation { generation }
                if generation.attempt() == attempt
                    && generation.model_binding() == model_binding
                    && generation.is_terminal() =>
            {
                Some(generation)
            }
            _ => None,
        })
    }

    #[must_use]
    pub fn generation_started(&self, attempt: u8, model_binding: &ModelBinding) -> bool {
        self.records.iter().any(|record| {
            matches!(
                record,
                WorkflowRecord::Generation {
                    generation: StoredGeneration::Started {
                        attempt: stored,
                        model_binding: stored_binding,
                    }
                } if *stored == attempt && stored_binding == model_binding
            )
        })
    }

    #[must_use]
    pub fn latest_rebind(&self) -> Option<&ModelBinding> {
        self.records.iter().rev().find_map(|record| match record {
            WorkflowRecord::Rebind { model_binding } => Some(model_binding),
            _ => None,
        })
    }
}

pub struct KnowledgeWorkflowStore<'a> {
    vault: &'a ProjectVault,
    job: KnowledgeEvaluationJob,
    directory: PathBuf,
    history: KnowledgeWorkflowHistory,
}

impl<'a> KnowledgeWorkflowStore<'a> {
    pub fn open(vault: &'a ProjectVault, job: KnowledgeEvaluationJob) -> Result<Self, VaultError> {
        let job = job.validate().map_err(|_| VaultError::RecoveryRequired)?;
        let directory = workflow_directory(vault.root(), &job);
        std::fs::create_dir_all(&directory).map_err(|_| VaultError::Io)?;
        atomic_create_or_same(&directory.join("job.json"), &encode_pretty(&job)?)?;
        let history = load_history(&directory.join("journal.jsonl"))?;
        if history.records.iter().any(|record| match record {
            WorkflowRecord::Event { event } => event.job_id != job.job_id,
            WorkflowRecord::Generation {
                generation: StoredGeneration::Accepted { patch, .. },
            } => patch.job_id != job.job_id,
            WorkflowRecord::Receipt { receipt } => receipt.job_id != job.job_id,
            WorkflowRecord::Generation { .. } | WorkflowRecord::Rebind { .. } => false,
        }) {
            return Err(VaultError::RecoveryRequired);
        }
        Ok(Self {
            vault,
            job,
            directory,
            history,
        })
    }

    #[must_use]
    pub const fn job(&self) -> &KnowledgeEvaluationJob {
        &self.job
    }

    #[must_use]
    pub const fn history(&self) -> &KnowledgeWorkflowHistory {
        &self.history
    }

    pub fn append_event(&mut self, event: WikiWorkflowEvent) -> Result<(), VaultError> {
        if event.job_id != self.job.job_id {
            return Err(VaultError::RecoveryRequired);
        }
        self.append(WorkflowRecord::Event { event })
    }

    pub fn append_generation(&mut self, generation: StoredGeneration) -> Result<(), VaultError> {
        if generation.attempt() == 0 || generation.attempt() > 2 {
            return Err(VaultError::RecoveryRequired);
        }
        if let StoredGeneration::Accepted { patch, .. } = &generation
            && patch.job_id != self.job.job_id
        {
            return Err(VaultError::RecoveryRequired);
        }
        if let Some(existing) = self.history.records.iter().find_map(|record| match record {
            WorkflowRecord::Generation {
                generation: existing,
            } if existing.attempt() == generation.attempt()
                && existing.model_binding() == generation.model_binding()
                && existing.is_terminal()
                && generation.is_terminal() =>
            {
                Some(existing)
            }
            _ => None,
        }) {
            return if existing == &generation {
                Ok(())
            } else {
                Err(VaultError::Conflict)
            };
        }
        self.append(WorkflowRecord::Generation { generation })
    }

    pub fn append_receipt(&mut self, receipt: KnowledgeReceipt) -> Result<(), VaultError> {
        if receipt.job_id != self.job.job_id
            || receipt.source_id != self.job.source_id
            || receipt.source_hash != self.job.source_hash
        {
            return Err(VaultError::RecoveryRequired);
        }
        if let Some(existing) = self.history.receipt() {
            if existing == &receipt {
                return Ok(());
            }
            if existing.outcome != KnowledgeReceiptOutcome::Pending {
                return Err(VaultError::Conflict);
            }
            crate::path::atomic_replace(
                &workflow_receipt_path(self.vault.root(), &self.job),
                &encode_pretty(&receipt)?,
            )?;
            return self.append(WorkflowRecord::Receipt { receipt });
        }
        atomic_create_or_same(
            &workflow_receipt_path(self.vault.root(), &self.job),
            &encode_pretty(&receipt)?,
        )?;
        self.append(WorkflowRecord::Receipt { receipt })
    }

    pub fn append_rebind(&mut self, model_binding: ModelBinding) -> Result<(), VaultError> {
        if self.history.latest_rebind() == Some(&model_binding) {
            return Ok(());
        }
        if self.history.latest_rebind().is_some() {
            return Err(VaultError::Conflict);
        }
        self.append(WorkflowRecord::Rebind { model_binding })
    }

    fn append(&mut self, record: WorkflowRecord) -> Result<(), VaultError> {
        if self.history.records.contains(&record) {
            return Ok(());
        }
        let envelope = WorkflowJournalRecord {
            schema_version: SchemaVersion,
            sequence: u32::try_from(self.history.records.len())
                .map_err(|_| VaultError::RecoveryRequired)?,
            record: record.clone(),
        };
        let mut bytes = serde_json::to_vec(&envelope).map_err(|_| VaultError::RecoveryRequired)?;
        bytes.push(b'\n');
        if bytes.len() > MAX_WORKFLOW_RECORD_BYTES {
            return Err(VaultError::RecoveryRequired);
        }
        let journal_path = self.directory.join("journal.jsonl");
        let mut journal = OpenOptions::new()
            .create(true)
            .append(true)
            .open(journal_path)
            .map_err(|_| VaultError::Io)?;
        journal.write_all(&bytes).map_err(|_| VaultError::Io)?;
        journal.flush().map_err(|_| VaultError::Io)?;
        journal.sync_all().map_err(|_| VaultError::Io)?;
        self.history.records.push(record);
        Ok(())
    }
}

pub fn knowledge_job_for_session(
    evidence: &FinalizedSessionEvidence,
) -> Result<KnowledgeEvaluationJob, VaultError> {
    let identity = serde_json::to_vec(&(
        evidence.evidence_id.as_str(),
        evidence.events_hash.as_str(),
        evidence.binding.provider_id.as_str(),
        evidence.binding.model_id.as_str(),
        evidence.binding.protocol,
        1_u16,
        1_u16,
    ))
    .map_err(|_| VaultError::RecoveryRequired)?;
    KnowledgeEvaluationJob {
        schema_version: SchemaVersion,
        job_id: minimax_protocol::KnowledgeJobId::new(format!(
            "wiki:{}",
            &sha256_hex(&identity)[..24]
        ))
        .map_err(|_| VaultError::RecoveryRequired)?,
        source_id: evidence.evidence_id.clone(),
        source_hash: evidence.events_hash.clone(),
        model_binding: evidence.binding.clone(),
        prompt_version: 1,
        patch_schema_version: 1,
        max_evidence_bytes: 256 * 1024,
        max_output_tokens: 2_048,
    }
    .validate()
    .map_err(|_| VaultError::RecoveryRequired)
}

pub fn ensure_knowledge_job(
    vault: &ProjectVault,
    evidence: &FinalizedSessionEvidence,
) -> Result<KnowledgeEvaluationJob, VaultError> {
    let job = knowledge_job_for_session(evidence)?;
    KnowledgeWorkflowStore::open(vault, job.clone())?;
    Ok(job)
}

pub fn find_evaluation_missing(
    vault: &ProjectVault,
) -> Result<Vec<FinalizedSessionEvidence>, VaultError> {
    let mut entries = std::fs::read_dir(vault.root().join("raw/sessions"))
        .map_err(|_| VaultError::Io)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| VaultError::Io)?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    let mut missing = Vec::new();
    for entry in entries {
        if !entry.file_type().map_err(|_| VaultError::Io)?.is_dir() {
            continue;
        }
        let evidence: FinalizedSessionEvidence = serde_json::from_slice(
            &std::fs::read(entry.path().join("session.json")).map_err(|_| VaultError::Io)?,
        )
        .map_err(|_| VaultError::RecoveryRequired)?;
        let job = knowledge_job_for_session(&evidence)?;
        if !workflow_directory(vault.root(), &job)
            .join("job.json")
            .exists()
        {
            missing.push(evidence);
        }
    }
    Ok(missing)
}

fn workflow_directory(root: &Path, job: &KnowledgeEvaluationJob) -> PathBuf {
    root.join(".minimax/pending")
        .join(sha256_hex(job.job_id.as_str().as_bytes()))
}

fn workflow_receipt_path(root: &Path, job: &KnowledgeEvaluationJob) -> PathBuf {
    root.join(".minimax/receipts").join(format!(
        "wiki-{}.json",
        sha256_hex(job.job_id.as_str().as_bytes())
    ))
}

fn load_history(path: &Path) -> Result<KnowledgeWorkflowHistory, VaultError> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Default::default()),
        Err(_) => return Err(VaultError::Io),
    };
    if !bytes.is_empty() && !bytes.ends_with(b"\n") {
        return Err(VaultError::RecoveryRequired);
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| VaultError::RecoveryRequired)?;
    let mut records = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.len() > MAX_WORKFLOW_RECORD_BYTES {
            return Err(VaultError::RecoveryRequired);
        }
        let envelope: WorkflowJournalRecord =
            serde_json::from_str(line).map_err(|_| VaultError::RecoveryRequired)?;
        if envelope.sequence != u32::try_from(index).map_err(|_| VaultError::RecoveryRequired)? {
            return Err(VaultError::RecoveryRequired);
        }
        records.push(envelope.record);
    }
    Ok(KnowledgeWorkflowHistory { records })
}

fn encode_pretty<T: Serialize>(value: &T) -> Result<Vec<u8>, VaultError> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|_| VaultError::RecoveryRequired)?;
    bytes.push(b'\n');
    Ok(bytes)
}
