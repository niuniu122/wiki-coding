use std::path::{Path, PathBuf};
use std::time::Duration;

use minimax_core::CancellationPort;
use minimax_protocol::{
    MAX_SHELL_COMMAND_BYTES, MAX_SHELL_CWD_BYTES, MAX_SHELL_OUTPUT_BYTES, SchemaVersion,
    ShellReceipt, ShellSessionState, ToolInvocation, ToolResult, ToolTerminalStatus,
};
use serde::Deserialize;

use crate::error::{ToolDenial, ToolDenialCode, io_denial};
use crate::path::WorkspaceRoot;
use crate::{DEFAULT_COMMAND_YIELD, ShellCommandRequest, ShellManagerError, ShellSessionManager};

pub const SHELL_RUNNING: &str = "shell_running";
pub const SHELL_EXITED: &str = "shell_exited";
pub const SHELL_NONZERO_EXIT: &str = "shell_nonzero_exit";
pub const SHELL_STOPPED: &str = "shell_stopped";

pub(crate) const DEFAULT_TOOL_OUTPUT_BYTES: usize = 16 * 1_024;

#[derive(Clone)]
pub struct ShellCommandTool {
    manager: ShellSessionManager,
}

impl ShellCommandTool {
    #[must_use]
    pub const fn new(manager: ShellSessionManager) -> Self {
        Self { manager }
    }

    pub async fn execute(
        &self,
        workspace: &WorkspaceRoot,
        invocation: &ToolInvocation,
        cancellation: &dyn CancellationPort,
    ) -> ToolResult {
        let arguments = match parse_arguments(workspace, invocation) {
            Ok(arguments) => arguments,
            Err(error) => return error.into_result(invocation),
        };
        match self.manager.start(arguments, cancellation).await {
            Ok(receipt) => receipt_result(invocation, receipt),
            Err(error) => manager_error_result(invocation, error),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ShellCommandArguments {
    command: String,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    tty: bool,
    #[serde(default)]
    yield_time_ms: Option<u64>,
    #[serde(default)]
    max_output_bytes: Option<usize>,
}

fn parse_arguments(
    workspace: &WorkspaceRoot,
    invocation: &ToolInvocation,
) -> Result<ShellCommandRequest, ToolDenial> {
    let arguments: ShellCommandArguments = serde_json::from_str(&invocation.call.arguments_json)
        .map_err(|_| ToolDenial::rejected(ToolDenialCode::InvalidArguments))?;
    if arguments.command.trim().is_empty() {
        return Err(ToolDenial::rejected(ToolDenialCode::InvalidArguments));
    }
    if arguments.command.len() > MAX_SHELL_COMMAND_BYTES
        || arguments
            .cwd
            .as_ref()
            .is_some_and(|cwd| cwd.len() > MAX_SHELL_CWD_BYTES)
    {
        return Err(ToolDenial::rejected(ToolDenialCode::InputLimit));
    }
    let yield_time = arguments
        .yield_time_ms
        .map_or(DEFAULT_COMMAND_YIELD, Duration::from_millis);
    if !(Duration::from_millis(250)..=Duration::from_secs(60)).contains(&yield_time) {
        return Err(ToolDenial::rejected(ToolDenialCode::InvalidArguments));
    }
    let max_output_bytes = arguments
        .max_output_bytes
        .unwrap_or(DEFAULT_TOOL_OUTPUT_BYTES);
    if !(1024..=MAX_SHELL_OUTPUT_BYTES).contains(&max_output_bytes) {
        return Err(ToolDenial::rejected(ToolDenialCode::InvalidArguments));
    }

    Ok(ShellCommandRequest {
        command: arguments.command,
        cwd: resolve_cwd(workspace, arguments.cwd.as_deref())?,
        tty: arguments.tty,
        yield_time,
        max_output_bytes,
    })
}

fn resolve_cwd(workspace: &WorkspaceRoot, cwd: Option<&str>) -> Result<PathBuf, ToolDenial> {
    let candidate = match cwd {
        None => workspace.as_path().to_owned(),
        Some("") => return Err(ToolDenial::rejected(ToolDenialCode::InvalidArguments)),
        Some(cwd) => {
            let requested = Path::new(cwd);
            if requested.is_absolute() {
                requested.to_owned()
            } else {
                workspace.as_path().join(requested)
            }
        }
    };
    let canonical = std::fs::canonicalize(candidate).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            ToolDenial::rejected(ToolDenialCode::PathNotFound)
        } else {
            io_denial(&error)
        }
    })?;
    if !canonical.is_dir() {
        return Err(ToolDenial::rejected(ToolDenialCode::WrongFileType));
    }
    Ok(canonical)
}

pub(crate) fn receipt_result(invocation: &ToolInvocation, receipt: ShellReceipt) -> ToolResult {
    let (status, code) = match (receipt.state, receipt.exit_code) {
        (ShellSessionState::Running, _) => (ToolTerminalStatus::Succeeded, SHELL_RUNNING),
        (ShellSessionState::Exited, Some(0)) => (ToolTerminalStatus::Succeeded, SHELL_EXITED),
        (ShellSessionState::Exited, _) => (ToolTerminalStatus::Failed, SHELL_NONZERO_EXIT),
        (ShellSessionState::Stopped, _) => (ToolTerminalStatus::Succeeded, SHELL_STOPPED),
        (ShellSessionState::Failed, _) => (
            ToolTerminalStatus::Failed,
            ToolDenialCode::ShellLaunchFailed.as_str(),
        ),
    };
    let Ok(output) = serde_json::to_string(&receipt) else {
        return ToolDenial::failed(ToolDenialCode::OutputLimit).into_result(invocation);
    };
    let result = ToolResult {
        schema_version: SchemaVersion,
        call_id: invocation.call.call_id.clone(),
        tool_name: invocation.call.name.clone(),
        status,
        code: code.to_owned(),
        output: Some(output),
    };
    if result.clone().validate().is_err() {
        ToolDenial::failed(ToolDenialCode::OutputLimit).into_result(invocation)
    } else {
        result
    }
}

pub(crate) fn manager_error_result(
    invocation: &ToolInvocation,
    error: ShellManagerError,
) -> ToolResult {
    let denial = match error {
        ShellManagerError::Disabled => {
            ToolDenial::rejected(ToolDenialCode::ShellRequiresFullAccess)
        }
        ShellManagerError::SessionNotFound => {
            ToolDenial::rejected(ToolDenialCode::ShellSessionNotFound)
        }
        ShellManagerError::SessionLimit => ToolDenial::rejected(ToolDenialCode::ShellSessionLimit),
        ShellManagerError::InvalidArguments => {
            ToolDenial::rejected(ToolDenialCode::InvalidArguments)
        }
        ShellManagerError::Launch | ShellManagerError::Io | ShellManagerError::Identifier => {
            ToolDenial::failed(ToolDenialCode::ShellLaunchFailed)
        }
        ShellManagerError::Cancelled => ToolDenial::cancelled(),
        ShellManagerError::Indeterminate => {
            ToolDenial::indeterminate(ToolDenialCode::ShellStopIndeterminate)
        }
    };
    denial.into_result(invocation)
}
