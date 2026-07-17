use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read as _;

use minimax_core::{CancellationPort, ToolSandboxPolicy};
use minimax_protocol::{ToolInvocation, ToolResult};
use serde::Deserialize;
use serde_json::Value;

use crate::WorkspaceRoot;
use crate::error::{ToolDenial, ToolDenialCode, io_denial};
use crate::policy::Preflight;
use crate::process::{BoundedProcess, ProcessRequest, reject_command_token};

const MAX_PACKAGE_BYTES: usize = 64 * 1_024;
const MAX_SCRIPT_BODY_BYTES: usize = 1_024;

#[derive(Clone)]
pub struct NpmDiagnosticTool {
    process: BoundedProcess,
}

impl NpmDiagnosticTool {
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
        let request = match prepare_npm(workspace, invocation, cancellation) {
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
struct NpmArguments {
    script: String,
}

fn prepare_npm(
    workspace: &WorkspaceRoot,
    invocation: &ToolInvocation,
    cancellation: &dyn CancellationPort,
) -> Result<ProcessRequest, ToolDenial> {
    Preflight::check(invocation, cancellation)?;
    let arguments: NpmArguments = serde_json::from_str(&invocation.call.arguments_json)
        .map_err(|_| ToolDenial::rejected(ToolDenialCode::InvalidArguments))?;
    validate_script_name(&arguments.script)?;
    let scripts = load_scripts(workspace)?;
    let body = scripts
        .get(&arguments.script)
        .ok_or_else(|| ToolDenial::rejected(ToolDenialCode::InvalidArguments))?;
    validate_script_body(body)?;
    if scripts.contains_key(&format!("pre{}", arguments.script))
        || scripts.contains_key(&format!("post{}", arguments.script))
    {
        return Err(ToolDenial::rejected(ToolDenialCode::UnsafeScript));
    }
    let (program, mut args) =
        npm_runtime().ok_or_else(|| ToolDenial::failed(ToolDenialCode::SpawnFailed))?;
    args.extend([
        "run".to_owned(),
        arguments.script,
        "--".to_owned(),
        "--no-color".to_owned(),
    ]);
    Ok(ProcessRequest::fixed(program, args, workspace.as_path()))
}

fn load_scripts(workspace: &WorkspaceRoot) -> Result<BTreeMap<String, String>, ToolDenial> {
    let target = workspace.resolve_existing("package.json")?;
    let mut file = File::open(target.absolute()).map_err(|error| io_denial(&error))?;
    let metadata = file.metadata().map_err(|error| io_denial(&error))?;
    if !metadata.is_file() || metadata.len() > MAX_PACKAGE_BYTES as u64 {
        return Err(ToolDenial::rejected(ToolDenialCode::InputLimit));
    }
    let mut bytes = Vec::new();
    file.by_ref()
        .take((MAX_PACKAGE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| io_denial(&error))?;
    if bytes.len() > MAX_PACKAGE_BYTES {
        return Err(ToolDenial::rejected(ToolDenialCode::InputLimit));
    }
    let text =
        String::from_utf8(bytes).map_err(|_| ToolDenial::rejected(ToolDenialCode::BinaryFile))?;
    if text.contains('\0') {
        return Err(ToolDenial::rejected(ToolDenialCode::BinaryFile));
    }
    let package: Value = serde_json::from_str(&text)
        .map_err(|_| ToolDenial::rejected(ToolDenialCode::InvalidArguments))?;
    package
        .get("scripts")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|_| ToolDenial::rejected(ToolDenialCode::InvalidArguments))?
        .ok_or_else(|| ToolDenial::rejected(ToolDenialCode::InvalidArguments))
}

fn validate_script_name(script: &str) -> Result<(), ToolDenial> {
    let lower = script.to_ascii_lowercase();
    if script.is_empty()
        || script.len() > 64
        || script.starts_with(['-', '@'])
        || !script
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'_' | b'-'))
        || !["check", "test", "lint", "verify", "typecheck", "format"]
            .iter()
            .any(|marker| lower.contains(marker))
    {
        Err(ToolDenial::rejected(ToolDenialCode::InvalidArguments))
    } else {
        Ok(())
    }
}

fn validate_script_body(body: &str) -> Result<(), ToolDenial> {
    reject_command_token(body).map_err(|_| ToolDenial::rejected(ToolDenialCode::UnsafeScript))?;
    let lower = body.to_ascii_lowercase();
    let first_token = lower.split_ascii_whitespace().next().unwrap_or_default();
    let forbidden = [
        " install",
        "uninstall",
        "publish",
        "npm ",
        "npx ",
        "pnpm ",
        "yarn ",
        "bun ",
        "exec ",
        "curl ",
        "wget ",
        "powershell",
        "pwsh ",
        "invoke-webrequest",
        "start-process",
        "git push",
        "git fetch",
        "git clone",
        " rm ",
        "del ",
        "remove-item",
        "rmdir",
    ];
    if body.is_empty()
        || body.len() > MAX_SCRIPT_BODY_BYTES
        || body.contains([';', '|', '&', '>', '<', '`'])
        || body.contains("$(")
        || matches!(
            first_token,
            "install"
                | "uninstall"
                | "publish"
                | "npm"
                | "npx"
                | "pnpm"
                | "yarn"
                | "bun"
                | "exec"
                | "curl"
                | "wget"
                | "powershell"
                | "pwsh"
                | "rm"
                | "del"
                | "rmdir"
        )
        || forbidden.iter().any(|token| lower.contains(token))
    {
        return Err(ToolDenial::rejected(ToolDenialCode::UnsafeScript));
    }
    Preflight::ensure_safe_output(body)
        .map_err(|_| ToolDenial::rejected(ToolDenialCode::UnsafeScript))
}

#[cfg(not(windows))]
fn npm_runtime() -> Option<(String, Vec<String>)> {
    Some(("npm".to_owned(), Vec::new()))
}

#[cfg(windows)]
fn npm_runtime() -> Option<(String, Vec<String>)> {
    if let Some(path) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&path) {
            let node = directory.join("node.exe");
            let cli = directory.join("node_modules/npm/bin/npm-cli.js");
            if node.is_file()
                && cli.is_file()
                && let (Some(node), Some(cli)) = (node.to_str(), cli.to_str())
            {
                return Some((node.to_owned(), vec![cli.to_owned()]));
            }
        }
    }
    None
}
