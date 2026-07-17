use minimax_core::{CancellationPort, ToolSandboxPolicy};
use minimax_protocol::{ToolInvocation, ToolResult};
use serde::Deserialize;

use crate::WorkspaceRoot;
use crate::error::{ToolDenial, ToolDenialCode};
use crate::policy::Preflight;
use crate::process::{BoundedProcess, ProcessRequest, normalized_relative, reject_command_token};

#[derive(Clone)]
pub struct GitStatusTool {
    process: BoundedProcess,
}

impl GitStatusTool {
    #[must_use]
    pub fn new(process: BoundedProcess) -> Self {
        Self { process }
    }

    #[must_use]
    pub fn production() -> Self {
        Self::new(BoundedProcess::production())
    }

    pub async fn execute(
        &self,
        workspace: &WorkspaceRoot,
        invocation: &ToolInvocation,
        cancellation: &dyn CancellationPort,
    ) -> ToolResult {
        self.execute_with_policy(
            workspace,
            invocation,
            ToolSandboxPolicy::Restricted,
            cancellation,
        )
        .await
    }

    pub async fn execute_with_policy(
        &self,
        workspace: &WorkspaceRoot,
        invocation: &ToolInvocation,
        sandbox_policy: ToolSandboxPolicy,
        cancellation: &dyn CancellationPort,
    ) -> ToolResult {
        let request = match prepare_status(workspace, invocation, cancellation) {
            Ok(request) => request,
            Err(error) => return error.into_result(invocation),
        };
        self.process
            .run_with_policy(&request, sandbox_policy, cancellation)
            .await
            .into_tool_result(invocation)
    }
}

#[derive(Clone)]
pub struct GitDiffTool {
    process: BoundedProcess,
}

impl GitDiffTool {
    #[must_use]
    pub fn new(process: BoundedProcess) -> Self {
        Self { process }
    }

    #[must_use]
    pub fn production() -> Self {
        Self::new(BoundedProcess::production())
    }

    pub async fn execute(
        &self,
        workspace: &WorkspaceRoot,
        invocation: &ToolInvocation,
        cancellation: &dyn CancellationPort,
    ) -> ToolResult {
        self.execute_with_policy(
            workspace,
            invocation,
            ToolSandboxPolicy::Restricted,
            cancellation,
        )
        .await
    }

    pub async fn execute_with_policy(
        &self,
        workspace: &WorkspaceRoot,
        invocation: &ToolInvocation,
        sandbox_policy: ToolSandboxPolicy,
        cancellation: &dyn CancellationPort,
    ) -> ToolResult {
        let request = match prepare_diff(workspace, invocation, cancellation) {
            Ok(request) => request,
            Err(error) => return error.into_result(invocation),
        };
        self.process
            .run_with_policy(&request, sandbox_policy, cancellation)
            .await
            .into_tool_result(invocation)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GitStatusArguments {
    #[serde(default)]
    path: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GitDiffArguments {
    #[serde(default)]
    cached: bool,
    #[serde(default)]
    path: Option<String>,
}

fn prepare_status(
    workspace: &WorkspaceRoot,
    invocation: &ToolInvocation,
    cancellation: &dyn CancellationPort,
) -> Result<ProcessRequest, ToolDenial> {
    Preflight::check(invocation, cancellation)?;
    let arguments: GitStatusArguments = parse_arguments(invocation)?;
    let mut args = git_prefix();
    args.extend([
        "-c".to_owned(),
        "pager.status=false".to_owned(),
        "status".to_owned(),
        "--short".to_owned(),
        "--untracked-files=all".to_owned(),
        "--".to_owned(),
    ]);
    if let Some(path) = arguments.path.as_deref() {
        args.push(resolve_optional_path(workspace, path)?);
    }
    Ok(ProcessRequest::fixed("git", args, workspace.as_path()))
}

fn prepare_diff(
    workspace: &WorkspaceRoot,
    invocation: &ToolInvocation,
    cancellation: &dyn CancellationPort,
) -> Result<ProcessRequest, ToolDenial> {
    Preflight::check(invocation, cancellation)?;
    let arguments: GitDiffArguments = parse_arguments(invocation)?;
    let mut args = git_prefix();
    args.extend([
        "-c".to_owned(),
        "diff.external=".to_owned(),
        "diff".to_owned(),
        "--no-color".to_owned(),
        "--no-ext-diff".to_owned(),
        "--no-textconv".to_owned(),
    ]);
    if arguments.cached {
        args.push("--cached".to_owned());
    }
    args.push("--".to_owned());
    if let Some(path) = arguments.path.as_deref() {
        args.push(resolve_optional_path(workspace, path)?);
    }
    Ok(ProcessRequest::fixed("git", args, workspace.as_path()))
}

fn git_prefix() -> Vec<String> {
    vec![
        "-c".to_owned(),
        format!("core.hooksPath={}", disabled_hooks_path()),
        "-c".to_owned(),
        "core.pager=cat".to_owned(),
        "-c".to_owned(),
        "color.ui=false".to_owned(),
        "-c".to_owned(),
        "core.quotepath=false".to_owned(),
    ]
}

#[cfg(windows)]
fn disabled_hooks_path() -> &'static str {
    "NUL"
}

#[cfg(not(windows))]
fn disabled_hooks_path() -> &'static str {
    "/dev/null"
}

fn resolve_optional_path(workspace: &WorkspaceRoot, path: &str) -> Result<String, ToolDenial> {
    reject_command_token(path)?;
    let target = workspace.resolve_write(path)?;
    Ok(normalized_relative(target.relative()))
}

fn parse_arguments<T: for<'de> Deserialize<'de>>(
    invocation: &ToolInvocation,
) -> Result<T, ToolDenial> {
    serde_json::from_str(&invocation.call.arguments_json)
        .map_err(|_| ToolDenial::rejected(ToolDenialCode::InvalidArguments))
}
