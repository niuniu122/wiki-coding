use std::time::Duration;

use minimax_core::CancellationPort;
use minimax_protocol::{
    MAX_SHELL_INPUT_BYTES, MAX_SHELL_OUTPUT_BYTES, ShellSessionId, ToolInvocation, ToolResult,
};
use serde::Deserialize;

use crate::error::{ToolDenial, ToolDenialCode};
use crate::shell_command::{DEFAULT_TOOL_OUTPUT_BYTES, manager_error_result, receipt_result};
use crate::{
    DEFAULT_POLL_YIELD, DEFAULT_WRITE_YIELD, ShellPollRequest, ShellSessionManager,
    ShellWriteRequest,
};

#[derive(Clone)]
pub struct ShellSessionTool {
    manager: ShellSessionManager,
}

impl ShellSessionTool {
    #[must_use]
    pub const fn new(manager: ShellSessionManager) -> Self {
        Self { manager }
    }

    pub async fn execute(
        &self,
        invocation: &ToolInvocation,
        cancellation: &dyn CancellationPort,
    ) -> ToolResult {
        let arguments = match parse_arguments(invocation) {
            Ok(arguments) => arguments,
            Err(error) => return error.into_result(invocation),
        };
        if cancellation.is_cancelled() {
            return ToolDenial::cancelled().into_result(invocation);
        }
        let result = match arguments {
            ParsedAction::Poll(request) => self.manager.poll(request, cancellation).await,
            ParsedAction::Write(request) => self.manager.write(request, cancellation).await,
            ParsedAction::Stop(session_id, max_output_bytes) => {
                self.manager.stop(&session_id, max_output_bytes).await
            }
        };
        match result {
            Ok(receipt) => receipt_result(invocation, receipt),
            Err(error) => manager_error_result(invocation, error),
        }
    }
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ShellAction {
    Poll,
    Write,
    Stop,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ShellSessionArguments {
    session_id: String,
    action: ShellAction,
    #[serde(default)]
    input: Option<String>,
    #[serde(default)]
    submit: Option<bool>,
    #[serde(default)]
    yield_time_ms: Option<u64>,
    #[serde(default)]
    max_output_bytes: Option<usize>,
}

enum ParsedAction {
    Poll(ShellPollRequest),
    Write(ShellWriteRequest),
    Stop(ShellSessionId, usize),
}

fn parse_arguments(invocation: &ToolInvocation) -> Result<ParsedAction, ToolDenial> {
    let arguments: ShellSessionArguments = serde_json::from_str(&invocation.call.arguments_json)
        .map_err(|_| ToolDenial::rejected(ToolDenialCode::InvalidArguments))?;
    let session_id = ShellSessionId::new(arguments.session_id)
        .map_err(|_| ToolDenial::rejected(ToolDenialCode::InvalidArguments))?;
    if arguments
        .input
        .as_ref()
        .is_some_and(|input| input.len() > MAX_SHELL_INPUT_BYTES)
    {
        return Err(ToolDenial::rejected(ToolDenialCode::InputLimit));
    }
    let max_output_bytes = arguments
        .max_output_bytes
        .unwrap_or(DEFAULT_TOOL_OUTPUT_BYTES);
    if !(1024..=MAX_SHELL_OUTPUT_BYTES).contains(&max_output_bytes)
        || arguments.yield_time_ms.is_some_and(|value| value > 60_000)
    {
        return Err(ToolDenial::rejected(ToolDenialCode::InvalidArguments));
    }

    match arguments.action {
        ShellAction::Poll => {
            if arguments.input.is_some() || arguments.submit.is_some() {
                return Err(ToolDenial::rejected(ToolDenialCode::InvalidArguments));
            }
            Ok(ParsedAction::Poll(ShellPollRequest {
                session_id,
                yield_time: arguments
                    .yield_time_ms
                    .map_or(DEFAULT_POLL_YIELD, Duration::from_millis),
                max_output_bytes,
            }))
        }
        ShellAction::Write => {
            let input = arguments.input.unwrap_or_default();
            let submit = arguments.submit.unwrap_or(false);
            if input.is_empty() && !submit {
                return Err(ToolDenial::rejected(ToolDenialCode::InvalidArguments));
            }
            Ok(ParsedAction::Write(ShellWriteRequest {
                session_id,
                input,
                submit,
                yield_time: arguments
                    .yield_time_ms
                    .map_or(DEFAULT_WRITE_YIELD, Duration::from_millis),
                max_output_bytes,
            }))
        }
        ShellAction::Stop => {
            if arguments.input.is_some()
                || arguments.submit.is_some()
                || arguments.yield_time_ms.unwrap_or(0) != 0
            {
                return Err(ToolDenial::rejected(ToolDenialCode::InvalidArguments));
            }
            Ok(ParsedAction::Stop(session_id, max_output_bytes))
        }
    }
}
