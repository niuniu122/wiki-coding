use std::fmt;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};

use crate::{
    ProtocolErrorCode, ProviderProtocolKind, SchemaVersion, SessionId, ToolCallId, TurnId, Usage,
};

macro_rules! validated_runtime_id {
    ($name:ident) => {
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, RuntimeErrorCode> {
                let value = value.into();
                if value.trim().is_empty() || value.len() > 256 {
                    return Err(RuntimeErrorCode::Configuration);
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
                Self::new(value).map_err(|_| D::Error::custom("identifier is invalid"))
            }
        }
    };
}

validated_runtime_id!(ProviderId);
validated_runtime_id!(ModelId);
validated_runtime_id!(RequestId);

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    System,
    User,
    Assistant,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModelMessage {
    pub role: MessageRole,
    pub content: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OutputSettings {
    pub max_output_tokens: u32,
}

impl OutputSettings {
    pub const MAX_OUTPUT_TOKENS: u32 = 1_048_576;

    pub fn new(max_output_tokens: u32) -> Result<Self, RuntimeErrorCode> {
        if max_output_tokens == 0 || max_output_tokens > Self::MAX_OUTPUT_TOKENS {
            return Err(RuntimeErrorCode::Configuration);
        }
        Ok(Self { max_output_tokens })
    }

    pub fn validate(self) -> Result<Self, RuntimeErrorCode> {
        Self::new(self.max_output_tokens)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TurnRequest {
    pub session_id: SessionId,
    pub turn_id: TurnId,
    pub request_id: RequestId,
    pub provider_id: ProviderId,
    pub model_id: ModelId,
    pub protocol: ProviderProtocolKind,
    pub messages: Vec<ModelMessage>,
    pub output: OutputSettings,
}

impl TurnRequest {
    pub const MAX_MESSAGES: usize = 4_096;
    pub const MAX_MESSAGE_BYTES: usize = 1_048_576;

    pub fn validate(self) -> Result<Self, RuntimeErrorCode> {
        if self.messages.is_empty()
            || self.messages.len() > Self::MAX_MESSAGES
            || self
                .messages
                .iter()
                .any(|message| message.content.len() > Self::MAX_MESSAGE_BYTES)
        {
            return Err(RuntimeErrorCode::Configuration);
        }
        self.output.validate()?;
        Ok(self)
    }
}

/// Redacted runtime failure classifications shared by adapters and presentation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeErrorCode {
    Configuration,
    CredentialMissing,
    TransportTimeout,
    TransportNetwork,
    HttpStatus,
    ProtocolMalformedJson,
    ProtocolPrematureEof,
    ProtocolDuplicateTerminal,
    ProtocolEventAfterTerminal,
    ProtocolUnknownEvent,
    Interrupted,
    WorkspaceBusy,
    Recovery,
    ToolUnavailable,
}

impl fmt::Display for RuntimeErrorCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = serde_json::to_value(self)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_else(|| "runtime_error".to_owned());
        formatter.write_str(&value)
    }
}

impl std::error::Error for RuntimeErrorCode {}

impl From<ProtocolErrorCode> for RuntimeErrorCode {
    fn from(value: ProtocolErrorCode) -> Self {
        match value {
            ProtocolErrorCode::MalformedJson | ProtocolErrorCode::MissingToolCallId => {
                Self::ProtocolMalformedJson
            }
            ProtocolErrorCode::PrematureEof => Self::ProtocolPrematureEof,
            ProtocolErrorCode::DuplicateTerminal => Self::ProtocolDuplicateTerminal,
            ProtocolErrorCode::EventAfterTerminal => Self::ProtocolEventAfterTerminal,
            ProtocolErrorCode::UnknownEvent => Self::ProtocolUnknownEvent,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeFailure {
    pub code: RuntimeErrorCode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http_status: Option<u16>,
}

impl RuntimeFailure {
    #[must_use]
    pub const fn new(code: RuntimeErrorCode) -> Self {
        Self {
            code,
            http_status: None,
        }
    }

    pub fn http(status: u16) -> Result<Self, RuntimeErrorCode> {
        if !(100..=599).contains(&status) {
            return Err(RuntimeErrorCode::Configuration);
        }
        Ok(Self {
            code: RuntimeErrorCode::HttpStatus,
            http_status: Some(status),
        })
    }
}

impl fmt::Display for RuntimeFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.http_status {
            Some(status) => write!(formatter, "{}:{status}", self.code),
            None => self.code.fmt(formatter),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum RuntimeTerminalOutcome {
    Completed,
    Failed { failure: RuntimeFailure },
    Interrupted,
    Stopped,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticCode {
    ProviderConnected,
    ProviderDisconnected,
    RecoveryApplied,
    NotAvailable,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum RuntimeEvent {
    TurnStarted {
        session_id: SessionId,
        turn_id: TurnId,
        request_id: RequestId,
    },
    VisibleTextDelta {
        delta: String,
    },
    ReasoningFiltered,
    ToolCallObserved {
        call_id: ToolCallId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    Usage {
        usage: Usage,
    },
    Diagnostic {
        code: DiagnosticCode,
    },
    Terminal {
        outcome: RuntimeTerminalOutcome,
    },
}

impl RuntimeEvent {
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Terminal { .. })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeEventV1 {
    pub schema_version: SchemaVersion,
    pub event: RuntimeEvent,
}

impl RuntimeEventV1 {
    #[must_use]
    pub const fn new(event: RuntimeEvent) -> Self {
        Self {
            schema_version: SchemaVersion,
            event,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TurnReceipt {
    pub session_id: SessionId,
    pub turn_id: TurnId,
    pub request_id: RequestId,
    pub outcome: RuntimeTerminalOutcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
}

pub fn parse_runtime_event_v1(raw: &str) -> Result<RuntimeEventV1, RuntimeErrorCode> {
    let value: serde_json::Value =
        serde_json::from_str(raw).map_err(|_| RuntimeErrorCode::ProtocolMalformedJson)?;
    let event_type = value
        .get("event")
        .and_then(|event| event.get("type"))
        .and_then(serde_json::Value::as_str)
        .ok_or(RuntimeErrorCode::ProtocolMalformedJson)?;
    if !matches!(
        event_type,
        "turn_started"
            | "visible_text_delta"
            | "reasoning_filtered"
            | "tool_call_observed"
            | "usage"
            | "diagnostic"
            | "terminal"
    ) {
        return Err(RuntimeErrorCode::ProtocolUnknownEvent);
    }
    serde_json::from_value(value).map_err(|_| RuntimeErrorCode::ProtocolMalformedJson)
}
