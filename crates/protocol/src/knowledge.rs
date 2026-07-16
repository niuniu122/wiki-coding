use std::collections::BTreeSet;
use std::fmt;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};

use crate::{ContentHash, ModelBinding, SchemaVersion, Usage, validate_vault_relative_path};

pub const MAX_KNOWLEDGE_BODY_BYTES: usize = 128 * 1_024;
pub const MAX_KNOWLEDGE_OPERATIONS: usize = 32;
pub const MAX_KNOWLEDGE_SOURCES: usize = 64;
const MAX_TITLE_BYTES: usize = 512;
const MAX_ID_BYTES: usize = 256;
const MAX_CODE_BYTES: usize = 64;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeValidationError {
    InvalidId,
    InvalidCode,
    InvalidPath,
    InvalidTitle,
    EmptyBody,
    BodyTooLarge,
    TooManyOperations,
    TooManySources,
    DuplicateOperation,
    DuplicateSource,
    InvalidSupersession,
    EmptyPatch,
}

impl fmt::Display for KnowledgeValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = serde_json::to_value(self)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_else(|| "knowledge_validation_error".to_owned());
        formatter.write_str(&value)
    }
}

impl std::error::Error for KnowledgeValidationError {}

macro_rules! knowledge_id {
    ($name:ident) => {
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, KnowledgeValidationError> {
                let value = value.into();
                if value.trim().is_empty()
                    || value.len() > MAX_ID_BYTES
                    || value.chars().any(char::is_control)
                {
                    return Err(KnowledgeValidationError::InvalidId);
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
                Self::new(value).map_err(|_| D::Error::custom("knowledge identifier is invalid"))
            }
        }
    };
}

knowledge_id!(EvidenceId);
knowledge_id!(KnowledgeJobId);
knowledge_id!(PageId);
knowledge_id!(TopicId);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceCitation {
    pub source_id: EvidenceId,
    pub source_hash: ContentHash,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgePageStatus {
    Current,
    Superseded,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgePage {
    pub schema_version: SchemaVersion,
    pub page_id: PageId,
    pub topic_id: TopicId,
    pub relative_path: String,
    pub title: String,
    pub status: KnowledgePageStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<PageId>,
    pub sources: Vec<SourceCitation>,
    pub body: String,
}

impl KnowledgePage {
    pub fn validate(self) -> Result<Self, KnowledgeValidationError> {
        validate_vault_relative_path(&self.relative_path)
            .map_err(|_| KnowledgeValidationError::InvalidPath)?;
        if !self.relative_path.starts_with("wiki/") || !self.relative_path.ends_with(".md") {
            return Err(KnowledgeValidationError::InvalidPath);
        }
        if self.title.trim().is_empty()
            || self.title.len() > MAX_TITLE_BYTES
            || self.title.chars().any(char::is_control)
        {
            return Err(KnowledgeValidationError::InvalidTitle);
        }
        if self.body.trim().is_empty() {
            return Err(KnowledgeValidationError::EmptyBody);
        }
        if self.body.len() > MAX_KNOWLEDGE_BODY_BYTES {
            return Err(KnowledgeValidationError::BodyTooLarge);
        }
        if self.sources.is_empty() || self.sources.len() > MAX_KNOWLEDGE_SOURCES {
            return Err(KnowledgeValidationError::TooManySources);
        }
        let mut source_ids = BTreeSet::new();
        if self
            .sources
            .iter()
            .any(|source| !source_ids.insert(source.source_id.clone()))
        {
            return Err(KnowledgeValidationError::DuplicateSource);
        }
        if self
            .sources
            .windows(2)
            .any(|pair| pair[0].source_id >= pair[1].source_id)
        {
            return Err(KnowledgeValidationError::DuplicateSource);
        }
        match (self.status, &self.superseded_by) {
            (KnowledgePageStatus::Current, None) | (KnowledgePageStatus::Superseded, Some(_)) => {}
            _ => return Err(KnowledgeValidationError::InvalidSupersession),
        }
        Ok(self)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum KnowledgeOperation {
    Create {
        page: KnowledgePage,
    },
    Replace {
        page: KnowledgePage,
        expected_hash: ContentHash,
    },
    Remove {
        page_id: PageId,
        relative_path: String,
        expected_hash: ContentHash,
    },
}

impl KnowledgeOperation {
    pub fn validate(self) -> Result<Self, KnowledgeValidationError> {
        match &self {
            Self::Create { page } | Self::Replace { page, .. } => {
                page.clone().validate()?;
            }
            Self::Remove { relative_path, .. } => {
                validate_vault_relative_path(relative_path)
                    .map_err(|_| KnowledgeValidationError::InvalidPath)?;
                if !relative_path.starts_with("wiki/") || !relative_path.ends_with(".md") {
                    return Err(KnowledgeValidationError::InvalidPath);
                }
            }
        }
        Ok(self)
    }

    #[must_use]
    pub fn page_id(&self) -> &PageId {
        match self {
            Self::Create { page } | Self::Replace { page, .. } => &page.page_id,
            Self::Remove { page_id, .. } => page_id,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgePatch {
    pub schema_version: SchemaVersion,
    pub job_id: KnowledgeJobId,
    pub operations: Vec<KnowledgeOperation>,
}

impl KnowledgePatch {
    pub fn validate(self) -> Result<Self, KnowledgeValidationError> {
        if self.operations.is_empty() {
            return Err(KnowledgeValidationError::EmptyPatch);
        }
        if self.operations.len() > MAX_KNOWLEDGE_OPERATIONS {
            return Err(KnowledgeValidationError::TooManyOperations);
        }
        let mut page_ids = BTreeSet::new();
        for operation in &self.operations {
            operation.clone().validate()?;
            if !page_ids.insert(operation.page_id().clone()) {
                return Err(KnowledgeValidationError::DuplicateOperation);
            }
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WikiWorkflowState {
    EvaluationPending,
    NoOp,
    SynthesisPending,
    Generating,
    Validating,
    Committing,
    Synthesized,
    Pending,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WikiWorkflowUsage {
    pub model_binding: ModelBinding,
    pub usage: Usage,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WikiWorkflowEvent {
    pub schema_version: SchemaVersion,
    pub job_id: KnowledgeJobId,
    pub state: WikiWorkflowState,
    pub code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<WikiWorkflowUsage>,
}

impl WikiWorkflowEvent {
    pub fn validate(self) -> Result<Self, KnowledgeValidationError> {
        validate_code(&self.code)?;
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeReceiptOutcome {
    NoOp,
    Pending,
    Synthesized,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeReceipt {
    pub schema_version: SchemaVersion,
    pub job_id: KnowledgeJobId,
    pub source_id: EvidenceId,
    pub source_hash: ContentHash,
    pub outcome: KnowledgeReceiptOutcome,
    pub code: String,
    pub model_binding: ModelBinding,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patch_hash: Option<ContentHash>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transaction_id: Option<crate::TransactionId>,
}

impl KnowledgeReceipt {
    pub fn validate(self) -> Result<Self, KnowledgeValidationError> {
        validate_code(&self.code)?;
        Ok(self)
    }
}

fn validate_code(code: &str) -> Result<(), KnowledgeValidationError> {
    if code.is_empty()
        || code.len() > MAX_CODE_BYTES
        || !code
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(KnowledgeValidationError::InvalidCode);
    }
    Ok(())
}
