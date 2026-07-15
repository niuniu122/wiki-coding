use std::fmt;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};

/// The first stable Rust protocol schema.
pub const SCHEMA_VERSION: u16 = 1;

/// A schema marker that can only contain the supported version.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SchemaVersion;

impl Serialize for SchemaVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u16(SCHEMA_VERSION)
    }
}

impl<'de> Deserialize<'de> for SchemaVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let version = u16::deserialize(deserializer)?;
        if version == SCHEMA_VERSION {
            Ok(Self)
        } else {
            Err(D::Error::custom("unsupported protocol schema version"))
        }
    }
}

macro_rules! validated_id {
    ($name:ident) => {
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, ProtocolErrorCode> {
                let value = value.into();
                if value.trim().is_empty() || value.len() > 256 {
                    return Err(ProtocolErrorCode::MalformedJson);
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
                Self::new(value).map_err(|_| D::Error::custom("identifier must not be empty"))
            }
        }
    };
}

validated_id!(SessionId);
validated_id!(TurnId);
validated_id!(ToolCallId);

/// The two Provider wire families supported by the compatibility baseline.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderProtocolKind {
    Responses,
    ChatCompletions,
}

/// Safe, stable protocol failure classifications.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolErrorCode {
    MalformedJson,
    #[serde(rename = "missing_call_id", alias = "missing_tool_call_id")]
    MissingToolCallId,
    PrematureEof,
    DuplicateTerminal,
    EventAfterTerminal,
    UnknownEvent,
    DuplicateToolCallId,
    InvalidToolArguments,
    ToolArgumentsTooLarge,
}

impl fmt::Display for ProtocolErrorCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = serde_json::to_value(self)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_else(|| "protocol_error".to_owned());
        formatter.write_str(&value)
    }
}

impl std::error::Error for ProtocolErrorCode {}

/// Provider-neutral token accounting.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Usage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<u64>,
}

/// A provider-neutral fragment of a native tool call.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ToolCallFragment {
    pub call_id: ToolCallId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments_delta: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub arguments_complete: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index: Option<u32>,
}

/// Exactly one terminal outcome closes a stream.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum TerminalOutcome {
    Completed,
    Failed { code: ProtocolErrorCode },
    Interrupted,
    Stopped,
}

/// Provider-neutral events accepted by the core sequence reducer.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum StreamEvent {
    ReasoningFiltered,
    VisibleTextDelta { delta: String },
    ToolCallFragments { fragments: Vec<ToolCallFragment> },
    Usage { usage: Usage },
    Terminal { outcome: TerminalOutcome },
}

impl StreamEvent {
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Terminal { .. })
    }
}

/// Strict versioned envelope for serialized stream events.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StreamEventV1 {
    pub schema_version: SchemaVersion,
    pub event: StreamEvent,
}

impl StreamEventV1 {
    #[must_use]
    pub const fn new(event: StreamEvent) -> Self {
        Self {
            schema_version: SchemaVersion,
            event,
        }
    }
}

/// Parse a v1 event without exposing untrusted JSON in the returned error.
pub fn parse_stream_event_v1(raw: &str) -> Result<StreamEventV1, ProtocolErrorCode> {
    let value: serde_json::Value =
        serde_json::from_str(raw).map_err(|_| ProtocolErrorCode::MalformedJson)?;
    let event_type = value
        .get("event")
        .and_then(|event| event.get("type"))
        .and_then(serde_json::Value::as_str)
        .ok_or(ProtocolErrorCode::MalformedJson)?;

    if !matches!(
        event_type,
        "reasoning_filtered" | "visible_text_delta" | "tool_call_fragments" | "usage" | "terminal"
    ) {
        return Err(ProtocolErrorCode::UnknownEvent);
    }

    serde_json::from_value(value).map_err(|_| ProtocolErrorCode::MalformedJson)
}
