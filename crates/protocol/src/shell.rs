use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, de};

use crate::ToolValidationError;

pub const MAX_SHELL_COMMAND_BYTES: usize = 32 * 1_024;
pub const MAX_SHELL_CWD_BYTES: usize = 4 * 1_024;
pub const MAX_SHELL_INPUT_BYTES: usize = 16 * 1_024;
pub const MAX_SHELL_OUTPUT_BYTES: usize = 49_152;
pub const MAX_SHELL_SESSION_ID_BYTES: usize = 128;
pub const MAX_SHELL_UNREAD_BYTES: usize = 1_024 * 1_024;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ShellSessionId(String);

impl ShellSessionId {
    pub fn new(value: impl Into<String>) -> Result<Self, ToolValidationError> {
        let value = value.into();
        if value.len() <= "shell-".len()
            || value.len() > MAX_SHELL_SESSION_ID_BYTES
            || !value.starts_with("shell-")
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err(ToolValidationError::InvalidShellReceipt);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for ShellSessionId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(String::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

impl fmt::Display for ShellSessionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShellSessionState {
    Running,
    Exited,
    Stopped,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ShellReceipt {
    pub session_id: ShellSessionId,
    pub state: ShellSessionState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    pub output: String,
    pub output_truncated: bool,
}

impl ShellReceipt {
    pub fn new(
        session_id: ShellSessionId,
        state: ShellSessionState,
        exit_code: Option<i32>,
        output: String,
        output_truncated: bool,
    ) -> Result<Self, ToolValidationError> {
        if output.len() > MAX_SHELL_OUTPUT_BYTES || output.contains('\0') {
            return Err(ToolValidationError::InvalidShellReceipt);
        }
        Ok(Self {
            session_id,
            state,
            exit_code,
            output,
            output_truncated,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ShellReceiptWire {
    session_id: ShellSessionId,
    state: ShellSessionState,
    #[serde(default)]
    exit_code: Option<i32>,
    output: String,
    output_truncated: bool,
}

impl<'de> Deserialize<'de> for ShellReceipt {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = ShellReceiptWire::deserialize(deserializer)?;
        Self::new(
            wire.session_id,
            wire.state,
            wire.exit_code,
            wire.output,
            wire.output_truncated,
        )
        .map_err(de::Error::custom)
    }
}
