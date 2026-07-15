use std::collections::BTreeMap;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};

use crate::{
    MessageRole, ModelId, ProviderId, ProviderProtocolKind, RequestId, RuntimeErrorCode,
    RuntimeTerminalOutcome, SchemaVersion, SessionId, ToolCallId, ToolDecision, ToolInvocation,
    ToolResult, TurnId, TurnReceipt, Usage,
};

macro_rules! session_id_type {
    ($name:ident) => {
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, RuntimeErrorCode> {
                let value = value.into();
                if value.trim().is_empty() || value.len() > 256 {
                    return Err(RuntimeErrorCode::Recovery);
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
                Self::new(value).map_err(|_| D::Error::custom("session record ID is invalid"))
            }
        }
    };
}

session_id_type!(RecordId);
session_id_type!(CompactionId);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelBinding {
    pub provider_id: ProviderId,
    pub model_id: ModelId,
    pub protocol: ProviderProtocolKind,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Active,
    Archived,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnStatus {
    Running,
    Completed,
    Failed,
    Interrupted,
    Stopped,
}

impl TurnStatus {
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        !matches!(self, Self::Running)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VisibleMessage {
    pub role: MessageRole,
    pub content: String,
    #[serde(default)]
    pub partial: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TurnRecord {
    pub turn_id: TurnId,
    pub request_id: RequestId,
    pub started_at_unix_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at_unix_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_of: Option<TurnId>,
    pub status: TurnStatus,
    pub user_message: VisibleMessage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assistant_message: Option<VisibleMessage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt: Option<TurnReceipt>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_invocations: Vec<ToolInvocationRecord>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolInvocationRecord {
    pub invocation: ToolInvocation,
    pub requested_at_unix_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision: Option<ToolDecision>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_at_unix_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at_unix_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_result: Option<ToolResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_at_unix_ms: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionRecord {
    pub session_id: SessionId,
    pub created_at_unix_ms: u64,
    pub updated_at_unix_ms: u64,
    pub status: SessionStatus,
    pub binding: ModelBinding,
    pub turns: Vec<TurnRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compaction: Option<CompactionPointer>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompactionPointer {
    pub compaction_id: CompactionId,
    pub covered_through_turn_id: TurnId,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompactionRecentTurn {
    pub turn_id: TurnId,
    pub user: String,
    pub assistant: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompactionRecord {
    pub compaction_id: CompactionId,
    pub covered_through_turn_id: TurnId,
    pub goal: Vec<String>,
    pub constraints: Vec<String>,
    pub decisions: Vec<String>,
    pub open_items: Vec<String>,
    pub retained_recent_turns: Vec<CompactionRecentTurn>,
    pub before_estimated_tokens: u64,
    pub after_estimated_tokens: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceCode {
    TurnStarted,
    ProviderConnected,
    ProviderFailed,
    TurnInterrupted,
    TurnRecovered,
    CompactionCompleted,
    CommandRejected,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TraceEntry {
    pub recorded_at_unix_ms: u64,
    pub code: TraceCode,
    pub facts: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum JournalRecord {
    SessionCreated {
        session: SessionRecord,
    },
    SessionActivated {
        session_id: SessionId,
        activated_at_unix_ms: u64,
    },
    TurnStarted {
        session_id: SessionId,
        binding: ModelBinding,
        turn: Box<TurnRecord>,
    },
    TurnDelta {
        session_id: SessionId,
        turn_id: TurnId,
        delta: String,
        recorded_at_unix_ms: u64,
    },
    TurnTerminal {
        session_id: SessionId,
        receipt: TurnReceipt,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        assistant_message: Option<VisibleMessage>,
        completed_at_unix_ms: u64,
    },
    ToolRequested {
        session_id: SessionId,
        turn_id: TurnId,
        invocation: ToolInvocation,
        requested_at_unix_ms: u64,
    },
    ToolDecisionRecorded {
        session_id: SessionId,
        turn_id: TurnId,
        decision: ToolDecision,
        recorded_at_unix_ms: u64,
    },
    ToolStarted {
        session_id: SessionId,
        turn_id: TurnId,
        call_id: ToolCallId,
        started_at_unix_ms: u64,
    },
    ToolTerminal {
        session_id: SessionId,
        turn_id: TurnId,
        result: ToolResult,
        completed_at_unix_ms: u64,
    },
    RecoveryApplied {
        session_id: SessionId,
        receipt: TurnReceipt,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        partial_assistant_message: Option<VisibleMessage>,
        recovered_at_unix_ms: u64,
    },
    CompactionStored {
        session_id: SessionId,
        compaction: Box<CompactionRecord>,
        stored_at_unix_ms: u64,
    },
    TraceStored {
        session_id: SessionId,
        entry: TraceEntry,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionRecordV1 {
    pub schema_version: SchemaVersion,
    pub record_id: RecordId,
    pub record: JournalRecord,
}

impl SessionRecordV1 {
    #[must_use]
    pub const fn new(record_id: RecordId, record: JournalRecord) -> Self {
        Self {
            schema_version: SchemaVersion,
            record_id,
            record,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecoveryRecord {
    pub session_id: SessionId,
    pub turn_id: TurnId,
    pub recovered_at_unix_ms: u64,
    pub outcome: RuntimeTerminalOutcome,
}

pub fn parse_session_record_v1(raw: &str) -> Result<SessionRecordV1, RuntimeErrorCode> {
    serde_json::from_str(raw).map_err(|_| RuntimeErrorCode::Recovery)
}
