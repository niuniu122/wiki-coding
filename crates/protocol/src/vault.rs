use std::fmt;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};

use crate::{EvidenceId, SchemaVersion, SessionId};

const MAX_ID_BYTES: usize = 256;
const SHA256_HEX_BYTES: usize = 64;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VaultValidationError {
    InvalidId,
    InvalidHash,
    InvalidPath,
    InvalidTargetOrder,
    EmptyTargets,
    DuplicateTarget,
    InvalidState,
}

impl fmt::Display for VaultValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = serde_json::to_value(self)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_else(|| "vault_validation_error".to_owned());
        formatter.write_str(&value)
    }
}

impl std::error::Error for VaultValidationError {}

macro_rules! vault_id {
    ($name:ident) => {
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, VaultValidationError> {
                let value = value.into();
                if value.trim().is_empty()
                    || value.len() > MAX_ID_BYTES
                    || value.chars().any(char::is_control)
                {
                    return Err(VaultValidationError::InvalidId);
                }
                Ok(Self(value))
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::new(value).map_err(|_| D::Error::custom("vault identifier is invalid"))
            }
        }
    };
}

vault_id!(ProjectId);
vault_id!(TransactionId);
vault_id!(GcId);
vault_id!(ForgetId);

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ContentHash(String);

impl ContentHash {
    pub fn new(value: impl Into<String>) -> Result<Self, VaultValidationError> {
        let value = value.into();
        if value.len() != SHA256_HEX_BYTES
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(VaultValidationError::InvalidHash);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for ContentHash {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(|_| D::Error::custom("content hash must be lowercase SHA-256"))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VaultManifest {
    pub schema_version: SchemaVersion,
    pub project_id: ProjectId,
    pub project_fingerprint: ContentHash,
    pub created_at_unix_ms: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VaultOwnership {
    Human,
    Agent,
    Internal,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RawEvidenceKind {
    Session,
    Import,
    Asset,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RawEvidenceManifest {
    pub schema_version: SchemaVersion,
    pub evidence_id: EvidenceId,
    pub kind: RawEvidenceKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
    pub relative_path: String,
    pub content_hash: ContentHash,
    pub bytes: u64,
    pub finalized_at_unix_ms: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InboxImportStatus {
    ImportedSourceRetained,
    CompiledSourceRemoved,
    EvidenceOnly,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InboxImportReceipt {
    pub schema_version: SchemaVersion,
    pub evidence_id: EvidenceId,
    pub kind: RawEvidenceKind,
    pub content_hash: ContentHash,
    pub bytes: u64,
    pub origin_relative_path: String,
    pub imported_relative_path: String,
    pub imported_at_unix_ms: u64,
    pub status: InboxImportStatus,
    pub code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transaction_id: Option<TransactionId>,
}

impl InboxImportReceipt {
    pub fn validate(self) -> Result<Self, VaultValidationError> {
        validate_vault_relative_path(&self.origin_relative_path)?;
        validate_vault_relative_path(&self.imported_relative_path)?;
        if !self.origin_relative_path.starts_with("inbox/")
            || !self.imported_relative_path.starts_with("raw/")
            || self.code.is_empty()
            || self.code.len() > 64
            || !self
                .code
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        {
            return Err(VaultValidationError::InvalidState);
        }
        match (self.kind, self.status, &self.transaction_id) {
            (RawEvidenceKind::Import, InboxImportStatus::ImportedSourceRetained, None)
            | (RawEvidenceKind::Import, InboxImportStatus::CompiledSourceRemoved, Some(_))
            | (RawEvidenceKind::Asset, InboxImportStatus::EvidenceOnly, None) => Ok(self),
            _ => Err(VaultValidationError::InvalidState),
        }
    }
}

impl RawEvidenceManifest {
    pub fn validate(self) -> Result<Self, VaultValidationError> {
        validate_vault_relative_path(&self.relative_path)?;
        if !self.relative_path.starts_with("raw/") {
            return Err(VaultValidationError::InvalidPath);
        }
        match (self.kind, &self.session_id) {
            (RawEvidenceKind::Session, Some(_))
            | (RawEvidenceKind::Import | RawEvidenceKind::Asset, None) => Ok(self),
            _ => Err(VaultValidationError::InvalidState),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TransactionState {
    Preparing,
    Prepared,
    Applying,
    Committed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransactionTarget {
    pub relative_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_hash: Option<ContentHash>,
    pub expected_hash: ContentHash,
    pub staged_relative_path: String,
    pub order: u32,
}

impl TransactionTarget {
    pub fn validate(self) -> Result<Self, VaultValidationError> {
        validate_vault_relative_path(&self.relative_path)?;
        validate_vault_relative_path(&self.staged_relative_path)?;
        if !(self.relative_path.starts_with("wiki/")
            || self.relative_path == "log.md"
            || self.relative_path.starts_with(".minimax/"))
            || !self
                .staged_relative_path
                .starts_with(".minimax/transactions/")
        {
            return Err(VaultValidationError::InvalidPath);
        }
        Ok(self)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransactionManifest {
    pub schema_version: SchemaVersion,
    pub transaction_id: TransactionId,
    pub state: TransactionState,
    pub targets: Vec<TransactionTarget>,
    pub created_at_unix_ms: u64,
}

impl TransactionManifest {
    pub fn validate(self) -> Result<Self, VaultValidationError> {
        if self.targets.is_empty() {
            return Err(VaultValidationError::EmptyTargets);
        }
        let mut paths = std::collections::BTreeSet::new();
        for (index, target) in self.targets.iter().enumerate() {
            target.clone().validate()?;
            if target.order != u32::try_from(index).unwrap_or(u32::MAX) {
                return Err(VaultValidationError::InvalidTargetOrder);
            }
            if !paths.insert(target.relative_path.clone()) {
                return Err(VaultValidationError::DuplicateTarget);
            }
        }
        Ok(self)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VaultReceipt {
    pub schema_version: SchemaVersion,
    pub operation_id: String,
    pub code: String,
    pub recorded_at_unix_ms: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VaultIssueCode {
    ManifestInvalid,
    OwnedPathMissing,
    RawMetadataInvalid,
    RawContentMissing,
    RawHashMismatch,
    WikiPageInvalid,
    WikiPageIdDuplicate,
    WikiCurrentTopicDuplicate,
    WikiSourceMissing,
    WikiSourceHashMismatch,
    WorkflowJournalIncomplete,
    WorkflowRecordInvalid,
    TransactionManifestInvalid,
    TransactionRecoveryRequired,
    TransactionTargetConflict,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VaultIssue {
    pub code: VaultIssueCode,
    pub relative_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub related_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VaultLintReport {
    pub schema_version: SchemaVersion,
    pub project_id: ProjectId,
    pub issues: Vec<VaultIssue>,
}

impl VaultLintReport {
    #[must_use]
    pub const fn is_clean(&self) -> bool {
        self.issues.is_empty()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VaultRepairReceipt {
    pub schema_version: SchemaVersion,
    pub operation_id: String,
    pub recovered_transactions: Vec<TransactionId>,
    pub quarantined_fragments: Vec<String>,
    pub remaining_issues: Vec<VaultIssue>,
    pub recorded_at_unix_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RebuildReceipt {
    pub schema_version: SchemaVersion,
    pub operation_id: String,
    pub raw_snapshot_hash: ContentHash,
    pub raw_object_count: u32,
    pub page_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transaction_id: Option<TransactionId>,
    pub code: String,
    pub recorded_at_unix_ms: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GcClass {
    Permanent,
    Referenced,
    Rebuildable,
    Collectable,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GcCandidate {
    pub relative_path: String,
    pub content_hash: ContentHash,
    pub bytes: u64,
    pub class: GcClass,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eligible_at_unix_ms: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GcPlan {
    pub schema_version: SchemaVersion,
    pub gc_id: GcId,
    pub plan_hash: ContentHash,
    pub candidates: Vec<GcCandidate>,
    pub created_at_unix_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrashEntry {
    pub original_relative_path: String,
    pub trash_relative_path: String,
    pub content_hash: ContentHash,
    pub bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrashManifest {
    pub schema_version: SchemaVersion,
    pub gc_id: GcId,
    pub plan_hash: ContentHash,
    pub entries: Vec<TrashEntry>,
    pub applied_at_unix_ms: u64,
    pub expires_at_unix_ms: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GcReceiptAction {
    Applied,
    Undone,
    Purged,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GcReceipt {
    pub schema_version: SchemaVersion,
    pub gc_id: GcId,
    pub plan_hash: ContentHash,
    pub action: GcReceiptAction,
    pub object_count: u32,
    pub bytes: u64,
    pub recorded_at_unix_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ForgetPlan {
    pub schema_version: SchemaVersion,
    pub forget_id: ForgetId,
    pub evidence_id: EvidenceId,
    pub expected_hash: ContentHash,
    pub affected_page_paths: Vec<String>,
    pub plan_hash: ContentHash,
    pub created_at_unix_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ForgetReceipt {
    pub schema_version: SchemaVersion,
    pub forget_id: ForgetId,
    pub evidence_id: EvidenceId,
    pub evidence_hash: ContentHash,
    pub transaction_id: TransactionId,
    pub tombstone_relative_path: String,
    pub code: String,
    pub recorded_at_unix_ms: u64,
}

pub fn validate_vault_relative_path(path: &str) -> Result<(), VaultValidationError> {
    if path.is_empty()
        || path.len() > 1_024
        || path.contains('\0')
        || path.contains('\\')
        || path.starts_with('/')
        || path.ends_with('/')
        || path.split('/').any(|segment| {
            segment.is_empty()
                || segment == "."
                || segment == ".."
                || segment.ends_with('.')
                || segment.chars().any(char::is_control)
        })
        || path.as_bytes().get(1) == Some(&b':')
    {
        return Err(VaultValidationError::InvalidPath);
    }
    Ok(())
}
