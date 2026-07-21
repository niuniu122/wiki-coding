use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{SchemaVersion, ToolCallId};

pub const MAX_TOOL_NAME_BYTES: usize = 64;
pub const MAX_TOOL_DESCRIPTION_BYTES: usize = 1_024;
pub const MAX_TOOL_ARGUMENT_BYTES: usize = 64 * 1_024;
pub const MAX_TOOL_RESULT_BYTES: usize = 64 * 1_024;
pub const MAX_TOOL_CODE_BYTES: usize = 64;
pub const V1_TOOL_NAMES: [&str; 8] = [
    "read_file",
    "list_directory",
    "apply_patch",
    "write_file",
    "run_diagnostic",
    "git_status",
    "git_diff",
    "npm_diagnostic",
];

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolValidationError {
    EmptyName,
    InvalidName,
    DescriptionTooLarge,
    InvalidParametersSchema,
    ArgumentsTooLarge,
    ArgumentsNotObject,
    UnknownArgument,
    MissingArgument,
    InvalidCode,
    ResultTooLarge,
    InvalidShellReceipt,
}

impl fmt::Display for ToolValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = serde_json::to_value(self)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_else(|| "tool_validation_error".to_owned());
        formatter.write_str(&value)
    }
}

impl std::error::Error for ToolValidationError {}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ToolDefinition {
    pub schema_version: SchemaVersion,
    pub name: String,
    pub description: String,
    pub parameters: Value,
    pub strict: bool,
}

impl ToolDefinition {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters: Value,
    ) -> Result<Self, ToolValidationError> {
        Self {
            schema_version: SchemaVersion,
            name: name.into(),
            description: description.into(),
            parameters,
            strict: true,
        }
        .validate()
    }

    pub fn validate(self) -> Result<Self, ToolValidationError> {
        validate_tool_name(&self.name)?;
        if self.description.len() > MAX_TOOL_DESCRIPTION_BYTES {
            return Err(ToolValidationError::DescriptionTooLarge);
        }
        let schema = self
            .parameters
            .as_object()
            .ok_or(ToolValidationError::InvalidParametersSchema)?;
        if !self.strict
            || schema.get("type").and_then(Value::as_str) != Some("object")
            || schema.get("additionalProperties").and_then(Value::as_bool) != Some(false)
            || !schema
                .get("properties")
                .is_some_and(serde_json::Value::is_object)
        {
            return Err(ToolValidationError::InvalidParametersSchema);
        }
        if let Some(required) = schema.get("required") {
            let Some(required) = required.as_array() else {
                return Err(ToolValidationError::InvalidParametersSchema);
            };
            if required.iter().any(|value| !value.is_string()) {
                return Err(ToolValidationError::InvalidParametersSchema);
            }
        }
        Ok(self)
    }

    pub fn validate_call(&self, call: &ToolCall) -> Result<(), ToolValidationError> {
        if call.name != self.name {
            return Err(ToolValidationError::InvalidName);
        }
        let arguments = call.arguments_value()?;
        let properties = self
            .parameters
            .get("properties")
            .and_then(Value::as_object)
            .ok_or(ToolValidationError::InvalidParametersSchema)?;
        if arguments.keys().any(|key| !properties.contains_key(key)) {
            return Err(ToolValidationError::UnknownArgument);
        }
        if let Some(required) = self.parameters.get("required").and_then(Value::as_array) {
            for key in required.iter().filter_map(Value::as_str) {
                if !arguments.contains_key(key) {
                    return Err(ToolValidationError::MissingArgument);
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ToolCall {
    pub schema_version: SchemaVersion,
    pub call_id: ToolCallId,
    pub name: String,
    pub arguments_json: String,
}

impl ToolCall {
    pub fn new(
        call_id: ToolCallId,
        name: impl Into<String>,
        arguments_json: impl Into<String>,
    ) -> Result<Self, ToolValidationError> {
        let call = Self {
            schema_version: SchemaVersion,
            call_id,
            name: name.into(),
            arguments_json: arguments_json.into(),
        };
        call.validate()
    }

    pub fn validate(self) -> Result<Self, ToolValidationError> {
        validate_tool_name(&self.name)?;
        self.arguments_value()?;
        Ok(self)
    }

    pub fn arguments_value(&self) -> Result<serde_json::Map<String, Value>, ToolValidationError> {
        if self.arguments_json.len() > MAX_TOOL_ARGUMENT_BYTES {
            return Err(ToolValidationError::ArgumentsTooLarge);
        }
        let value: Value = serde_json::from_str(&self.arguments_json)
            .map_err(|_| ToolValidationError::ArgumentsNotObject)?;
        value
            .as_object()
            .cloned()
            .ok_or(ToolValidationError::ArgumentsNotObject)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolEffect {
    Read,
    Write,
    Process,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ToolInvocation {
    pub schema_version: SchemaVersion,
    pub call: ToolCall,
    pub effect: ToolEffect,
}

impl ToolInvocation {
    pub fn new(call: ToolCall, effect: ToolEffect) -> Result<Self, ToolValidationError> {
        call.arguments_value()?;
        Ok(Self {
            schema_version: SchemaVersion,
            call,
            effect,
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolDecisionKind {
    Approved,
    Rejected,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ToolDecision {
    pub schema_version: SchemaVersion,
    pub call_id: ToolCallId,
    pub decision: ToolDecisionKind,
    pub code: String,
}

impl ToolDecision {
    pub fn validate(self) -> Result<Self, ToolValidationError> {
        validate_code(&self.code)?;
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolTerminalStatus {
    Succeeded,
    Failed,
    Rejected,
    Cancelled,
    Indeterminate,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ToolResult {
    pub schema_version: SchemaVersion,
    pub call_id: ToolCallId,
    pub tool_name: String,
    pub status: ToolTerminalStatus,
    pub code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
}

impl ToolResult {
    pub fn validate(self) -> Result<Self, ToolValidationError> {
        validate_tool_name(&self.tool_name)?;
        validate_code(&self.code)?;
        if self
            .output
            .as_ref()
            .is_some_and(|output| output.len() > MAX_TOOL_RESULT_BYTES)
        {
            return Err(ToolValidationError::ResultTooLarge);
        }
        Ok(self)
    }
}

pub fn validate_unique_call_ids<'a>(
    calls: impl IntoIterator<Item = &'a ToolCall>,
) -> Result<(), ToolCallId> {
    let mut seen = BTreeSet::new();
    for call in calls {
        if !seen.insert(call.call_id.clone()) {
            return Err(call.call_id.clone());
        }
    }
    Ok(())
}

fn validate_tool_name(name: &str) -> Result<(), ToolValidationError> {
    if name.is_empty() {
        return Err(ToolValidationError::EmptyName);
    }
    if name.len() > MAX_TOOL_NAME_BYTES
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(ToolValidationError::InvalidName);
    }
    Ok(())
}

fn validate_code(code: &str) -> Result<(), ToolValidationError> {
    if code.is_empty()
        || code.len() > MAX_TOOL_CODE_BYTES
        || !code
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(ToolValidationError::InvalidCode);
    }
    Ok(())
}
