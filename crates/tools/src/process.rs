use std::collections::BTreeMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use minimax_core::CancellationPort;
use minimax_protocol::{SchemaVersion, ToolInvocation, ToolResult, ToolTerminalStatus};
use serde::Deserialize;
use serde::Serialize;
use tokio::io::AsyncReadExt as _;
use tokio::process::{Child, ChildStderr, ChildStdout, Command};

use crate::WorkspaceRoot;
use crate::error::{ToolDenial, ToolDenialCode};
use crate::policy::Preflight;

const DEFAULT_PROCESS_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_PROCESS_OUTPUT_BYTES: usize = 64 * 1_024;
const READ_CHUNK_BYTES: usize = 8 * 1_024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessRequest {
    program: String,
    args: Vec<String>,
    cwd: PathBuf,
    env: BTreeMap<String, String>,
}

impl ProcessRequest {
    pub(crate) fn fixed(
        program: impl Into<String>,
        args: Vec<String>,
        workspace_root: &Path,
    ) -> Self {
        Self {
            program: program.into(),
            args,
            cwd: workspace_root.to_path_buf(),
            env: safe_environment(),
        }
    }

    #[must_use]
    pub fn program(&self) -> &str {
        &self.program
    }

    #[must_use]
    pub fn args(&self) -> &[String] {
        &self.args
    }

    #[must_use]
    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    #[must_use]
    pub fn env(&self) -> &BTreeMap<String, String> {
        &self.env
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessLimits {
    timeout: Duration,
    max_output_bytes: usize,
}

impl ProcessLimits {
    pub fn new(timeout: Duration, max_output_bytes: usize) -> Result<Self, ToolDenial> {
        if timeout.is_zero()
            || timeout > DEFAULT_PROCESS_TIMEOUT
            || max_output_bytes == 0
            || max_output_bytes > DEFAULT_PROCESS_OUTPUT_BYTES
        {
            return Err(ToolDenial::rejected(ToolDenialCode::InvalidArguments));
        }
        Ok(Self {
            timeout,
            max_output_bytes,
        })
    }
}

impl Default for ProcessLimits {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_PROCESS_TIMEOUT,
            max_output_bytes: DEFAULT_PROCESS_OUTPUT_BYTES,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ChildEvent {
    Stdout(Vec<u8>),
    Stderr(Vec<u8>),
    StdoutClosed,
    StderrClosed,
    Exited(Option<i32>),
}

pub type ChildEventFuture<'a> =
    Pin<Box<dyn Future<Output = std::io::Result<ChildEvent>> + Send + 'a>>;
pub type ChildStopFuture<'a> = Pin<Box<dyn Future<Output = std::io::Result<()>> + Send + 'a>>;

pub trait DirectChild: Send {
    fn next_event<'a>(&'a mut self) -> ChildEventFuture<'a>;
    fn terminate_and_wait<'a>(&'a mut self) -> ChildStopFuture<'a>;
}

pub trait ProcessLauncher: Send + Sync {
    fn spawn(&self, request: &ProcessRequest) -> std::io::Result<Box<dyn DirectChild>>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TokioProcessLauncher;

impl ProcessLauncher for TokioProcessLauncher {
    fn spawn(&self, request: &ProcessRequest) -> std::io::Result<Box<dyn DirectChild>> {
        let mut command = Command::new(&request.program);
        command
            .args(&request.args)
            .current_dir(&request.cwd)
            .env_clear()
            .envs(&request.env)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(unix)]
        command.process_group(0);
        #[cfg(windows)]
        command.creation_flags(0x0800_0000 | 0x0000_0200);
        let mut child = command.spawn()?;
        let process_id = child
            .id()
            .ok_or_else(|| std::io::Error::other("missing child process id"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| std::io::Error::other("missing child stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| std::io::Error::other("missing child stderr"))?;
        Ok(Box::new(TokioDirectChild {
            child,
            stdout,
            stderr,
            stdout_closed: false,
            stderr_closed: false,
            exit_emitted: false,
            process_id,
        }))
    }
}

struct TokioDirectChild {
    child: Child,
    stdout: ChildStdout,
    stderr: ChildStderr,
    stdout_closed: bool,
    stderr_closed: bool,
    exit_emitted: bool,
    process_id: u32,
}

impl DirectChild for TokioDirectChild {
    fn next_event<'a>(&'a mut self) -> ChildEventFuture<'a> {
        Box::pin(async move {
            let mut stdout_buffer = [0_u8; READ_CHUNK_BYTES];
            let mut stderr_buffer = [0_u8; READ_CHUNK_BYTES];
            let stdout_open = !self.stdout_closed;
            let stderr_open = !self.stderr_closed;
            let wait_open = !self.exit_emitted;
            tokio::select! {
                result = self.stdout.read(&mut stdout_buffer), if stdout_open => {
                    let bytes = result?;
                    if bytes == 0 {
                        self.stdout_closed = true;
                        Ok(ChildEvent::StdoutClosed)
                    } else {
                        Ok(ChildEvent::Stdout(stdout_buffer[..bytes].to_vec()))
                    }
                }
                result = self.stderr.read(&mut stderr_buffer), if stderr_open => {
                    let bytes = result?;
                    if bytes == 0 {
                        self.stderr_closed = true;
                        Ok(ChildEvent::StderrClosed)
                    } else {
                        Ok(ChildEvent::Stderr(stderr_buffer[..bytes].to_vec()))
                    }
                }
                result = self.child.wait(), if wait_open => {
                    let status = result?;
                    self.exit_emitted = true;
                    Ok(ChildEvent::Exited(status.code()))
                }
                else => Err(std::io::Error::other("child emitted no terminal event")),
            }
        })
    }

    fn terminate_and_wait<'a>(&'a mut self) -> ChildStopFuture<'a> {
        Box::pin(async move {
            let tree_result = terminate_process_tree(self.process_id).await;
            let _ = self.child.start_kill();
            let wait_result = self.child.wait().await;
            self.exit_emitted = true;
            tree_result?;
            wait_result?;
            Ok(())
        })
    }
}

#[cfg(unix)]
async fn terminate_process_tree(process_id: u32) -> std::io::Result<()> {
    let process_group = format!("-{process_id}");
    let terminate = Command::new(kill_program())
        .args(["-TERM", "--", &process_group])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .status();
    let status = tokio::time::timeout(Duration::from_secs(1), terminate)
        .await
        .map_err(|_| std::io::Error::other("process tree termination timed out"))??;
    if !status.success() {
        return Err(std::io::Error::other("process tree termination failed"));
    }
    tokio::time::sleep(Duration::from_millis(50)).await;
    let force = Command::new(kill_program())
        .args(["-KILL", "--", &process_group])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .status();
    let _ = tokio::time::timeout(Duration::from_secs(1), force).await;
    Ok(())
}

#[cfg(unix)]
fn kill_program() -> &'static str {
    if Path::new("/bin/kill").is_file() {
        "/bin/kill"
    } else {
        "/usr/bin/kill"
    }
}

#[cfg(windows)]
async fn terminate_process_tree(process_id: u32) -> std::io::Result<()> {
    let system_root = std::env::var_os("SystemRoot")
        .ok_or_else(|| std::io::Error::other("missing SystemRoot"))?;
    let taskkill = PathBuf::from(system_root)
        .join("System32")
        .join("taskkill.exe");
    let status = tokio::time::timeout(
        Duration::from_secs(2),
        Command::new(taskkill)
            .args(["/PID", &process_id.to_string(), "/T", "/F"])
            .env_clear()
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(0x0800_0000)
            .kill_on_drop(true)
            .status(),
    )
    .await
    .map_err(|_| std::io::Error::other("process tree termination timed out"))??;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other("process tree termination failed"))
    }
}

#[derive(Clone)]
pub struct BoundedProcess {
    launcher: Arc<dyn ProcessLauncher>,
    limits: ProcessLimits,
}

impl BoundedProcess {
    #[must_use]
    pub fn new(launcher: Arc<dyn ProcessLauncher>, limits: ProcessLimits) -> Self {
        Self { launcher, limits }
    }

    #[must_use]
    pub fn production() -> Self {
        Self::new(Arc::new(TokioProcessLauncher), ProcessLimits::default())
    }

    pub async fn run(
        &self,
        request: &ProcessRequest,
        cancellation: &dyn CancellationPort,
    ) -> ProcessCompletion {
        if cancellation.is_cancelled() {
            return ProcessCompletion::cancelled();
        }
        let Ok(mut child) = self.launcher.spawn(request) else {
            return ProcessCompletion::failed(ToolDenialCode::SpawnFailed, None, None);
        };
        let timeout = tokio::time::sleep(self.limits.timeout);
        tokio::pin!(timeout);
        let mut combined_bytes = 0_usize;
        let mut stdout = Vec::new();
        let mut stdout_closed = false;
        let mut stderr_closed = false;
        let mut exit_seen = false;
        let mut exit_code = None;

        loop {
            if stdout_closed && stderr_closed && exit_seen {
                return complete_process(exit_code, stdout);
            }
            let event = tokio::select! {
                biased;
                () = cancellation.cancelled() => {
                    if child.terminate_and_wait().await.is_err() {
                        return ProcessCompletion::indeterminate();
                    }
                    return ProcessCompletion::cancelled();
                }
                () = &mut timeout => {
                    if child.terminate_and_wait().await.is_err() {
                        return ProcessCompletion::indeterminate();
                    }
                    return ProcessCompletion::failed(ToolDenialCode::TimedOut, None, None);
                }
                event = child.next_event() => event,
            };
            let Ok(event) = event else {
                if child.terminate_and_wait().await.is_err() {
                    return ProcessCompletion::indeterminate();
                }
                return ProcessCompletion::failed(ToolDenialCode::ProcessIo, None, None);
            };
            match event {
                ChildEvent::Stdout(bytes) => {
                    if exceeds_output_limit(
                        &mut combined_bytes,
                        bytes.len(),
                        self.limits.max_output_bytes,
                    ) {
                        if child.terminate_and_wait().await.is_err() {
                            return ProcessCompletion::indeterminate();
                        }
                        return ProcessCompletion::failed(ToolDenialCode::OutputLimit, None, None);
                    }
                    stdout.extend_from_slice(&bytes);
                }
                ChildEvent::Stderr(bytes) => {
                    if exceeds_output_limit(
                        &mut combined_bytes,
                        bytes.len(),
                        self.limits.max_output_bytes,
                    ) {
                        if child.terminate_and_wait().await.is_err() {
                            return ProcessCompletion::indeterminate();
                        }
                        return ProcessCompletion::failed(ToolDenialCode::OutputLimit, None, None);
                    }
                }
                ChildEvent::StdoutClosed => stdout_closed = true,
                ChildEvent::StderrClosed => stderr_closed = true,
                ChildEvent::Exited(code) => {
                    exit_seen = true;
                    exit_code = code;
                }
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessCompletion {
    status: ToolTerminalStatus,
    code: ToolDenialCode,
    exit_code: Option<i32>,
    stdout: Option<String>,
}

impl ProcessCompletion {
    fn cancelled() -> Self {
        Self {
            status: ToolTerminalStatus::Cancelled,
            code: ToolDenialCode::Cancelled,
            exit_code: None,
            stdout: None,
        }
    }

    fn failed(code: ToolDenialCode, exit_code: Option<i32>, stdout: Option<String>) -> Self {
        Self {
            status: ToolTerminalStatus::Failed,
            code,
            exit_code,
            stdout,
        }
    }

    fn indeterminate() -> Self {
        Self {
            status: ToolTerminalStatus::Indeterminate,
            code: ToolDenialCode::CleanupUnknown,
            exit_code: None,
            stdout: None,
        }
    }

    fn succeeded(stdout: Option<String>) -> Self {
        Self {
            status: ToolTerminalStatus::Succeeded,
            code: ToolDenialCode::NonzeroExit,
            exit_code: Some(0),
            stdout,
        }
    }

    #[must_use]
    pub fn into_tool_result(self, invocation: &ToolInvocation) -> ToolResult {
        if let Some(stdout) = self.stdout.as_deref()
            && let Err(error) = Preflight::ensure_safe_output(stdout)
        {
            return ToolDenial::failed(error.code()).into_result(invocation);
        }
        let code = if self.status == ToolTerminalStatus::Succeeded {
            "ok"
        } else {
            self.code.as_str()
        };
        let output = if self.exit_code.is_some() || self.stdout.is_some() {
            let receipt = ProcessReceipt {
                exit_code: self.exit_code,
                stdout: self.stdout,
            };
            match serde_json::to_string(&receipt) {
                Ok(output) => match Preflight::ensure_safe_output(&output) {
                    Ok(()) => Some(output),
                    Err(error) => {
                        return ToolDenial::failed(error.code()).into_result(invocation);
                    }
                },
                Err(_) => {
                    return ToolDenial::failed(ToolDenialCode::ProcessIo).into_result(invocation);
                }
            }
        } else {
            None
        };
        ToolResult {
            schema_version: SchemaVersion,
            call_id: invocation.call.call_id.clone(),
            tool_name: invocation.call.name.clone(),
            status: self.status,
            code: code.to_owned(),
            output,
        }
    }
}

#[derive(Serialize)]
struct ProcessReceipt {
    #[serde(skip_serializing_if = "Option::is_none")]
    exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stdout: Option<String>,
}

fn complete_process(exit_code: Option<i32>, stdout: Vec<u8>) -> ProcessCompletion {
    let Ok(stdout) = String::from_utf8(stdout) else {
        return ProcessCompletion::failed(ToolDenialCode::BinaryFile, exit_code, None);
    };
    let stdout = sanitize_process_output(&stdout);
    let stdout = (!stdout.is_empty()).then_some(stdout);
    if exit_code == Some(0) {
        ProcessCompletion::succeeded(stdout)
    } else {
        ProcessCompletion::failed(ToolDenialCode::NonzeroExit, exit_code, stdout)
    }
}

fn exceeds_output_limit(total: &mut usize, next: usize, limit: usize) -> bool {
    let Some(updated) = total.checked_add(next) else {
        return true;
    };
    if updated > limit {
        true
    } else {
        *total = updated;
        false
    }
}

fn sanitize_process_output(output: &str) -> String {
    output
        .chars()
        .filter(|character| !character.is_control() || matches!(character, '\n' | '\r' | '\t'))
        .collect()
}

fn safe_environment() -> BTreeMap<String, String> {
    let mut environment = BTreeMap::new();
    for key in [
        "PATH",
        "Path",
        "SystemRoot",
        "ComSpec",
        "PATHEXT",
        "TEMP",
        "TMP",
        "HOME",
        "USERPROFILE",
        "APPDATA",
        "LOCALAPPDATA",
    ] {
        if let Ok(value) = std::env::var(key) {
            environment.insert(key.to_owned(), value);
        }
    }
    for (key, value) in [
        ("CI", "1"),
        ("NO_COLOR", "1"),
        ("GIT_TERMINAL_PROMPT", "0"),
        ("GIT_CONFIG_NOSYSTEM", "1"),
        ("GIT_ATTR_NOSYSTEM", "1"),
        ("GIT_CONFIG_GLOBAL", disabled_config_path()),
        ("GIT_PAGER", "cat"),
        ("PAGER", "cat"),
        ("CARGO_NET_OFFLINE", "true"),
        ("npm_config_audit", "false"),
        ("npm_config_fund", "false"),
        ("npm_config_offline", "true"),
        ("npm_config_userconfig", disabled_config_path()),
        ("npm_config_update_notifier", "false"),
    ] {
        environment.insert(key.to_owned(), value.to_owned());
    }
    environment
}

#[cfg(windows)]
fn disabled_config_path() -> &'static str {
    "NUL"
}

#[cfg(not(windows))]
fn disabled_config_path() -> &'static str {
    "/dev/null"
}

#[derive(Clone)]
pub struct RunDiagnosticTool {
    process: BoundedProcess,
}

impl RunDiagnosticTool {
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
        let request = match prepare_diagnostic(workspace, invocation, cancellation) {
            Ok(request) => request,
            Err(error) => return error.into_result(invocation),
        };
        self.process
            .run(&request, cancellation)
            .await
            .into_tool_result(invocation)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DiagnosticArguments {
    action: DiagnosticAction,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    pattern: Option<String>,
    #[serde(default)]
    max_results: Option<u16>,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum DiagnosticAction {
    CargoCheck,
    CargoTest,
    CargoClippy,
    CargoFmtCheck,
    NodeCheck,
    RgSearch,
}

fn prepare_diagnostic(
    workspace: &WorkspaceRoot,
    invocation: &ToolInvocation,
    cancellation: &dyn CancellationPort,
) -> Result<ProcessRequest, ToolDenial> {
    Preflight::check(invocation, cancellation)?;
    let arguments: DiagnosticArguments = serde_json::from_str(&invocation.call.arguments_json)
        .map_err(|_| ToolDenial::rejected(ToolDenialCode::InvalidArguments))?;
    let (program, args) = match arguments.action {
        DiagnosticAction::CargoCheck => {
            require_no_diagnostic_options(&arguments)?;
            (
                "cargo",
                strings(&["check", "--workspace", "--locked", "--offline"]),
            )
        }
        DiagnosticAction::CargoTest => {
            require_no_diagnostic_options(&arguments)?;
            (
                "cargo",
                strings(&["test", "--workspace", "--locked", "--offline"]),
            )
        }
        DiagnosticAction::CargoClippy => {
            require_no_diagnostic_options(&arguments)?;
            (
                "cargo",
                strings(&[
                    "clippy",
                    "--workspace",
                    "--all-targets",
                    "--locked",
                    "--offline",
                    "--",
                    "-D",
                    "warnings",
                ]),
            )
        }
        DiagnosticAction::CargoFmtCheck => {
            require_no_diagnostic_options(&arguments)?;
            ("cargo", strings(&["fmt", "--all", "--", "--check"]))
        }
        DiagnosticAction::NodeCheck => {
            if arguments.pattern.is_some() || arguments.max_results.is_some() {
                return Err(ToolDenial::rejected(ToolDenialCode::InvalidArguments));
            }
            let path = arguments
                .path
                .as_deref()
                .ok_or_else(|| ToolDenial::rejected(ToolDenialCode::InvalidArguments))?;
            reject_command_token(path)?;
            let target = workspace.resolve_existing(path)?;
            if !target.absolute().is_file()
                || !matches!(
                    target
                        .absolute()
                        .extension()
                        .and_then(|value| value.to_str()),
                    Some("js" | "mjs" | "cjs")
                )
            {
                return Err(ToolDenial::rejected(ToolDenialCode::WrongFileType));
            }
            (
                "node",
                vec![
                    "--check".to_owned(),
                    "--".to_owned(),
                    normalized_relative(target.relative()),
                ],
            )
        }
        DiagnosticAction::RgSearch => {
            let path = arguments
                .path
                .as_deref()
                .ok_or_else(|| ToolDenial::rejected(ToolDenialCode::InvalidArguments))?;
            let pattern = arguments
                .pattern
                .as_deref()
                .ok_or_else(|| ToolDenial::rejected(ToolDenialCode::InvalidArguments))?;
            reject_command_token(path)?;
            reject_search_pattern(pattern)?;
            let target = workspace.resolve_existing(path)?;
            let max_results = arguments.max_results.unwrap_or(100);
            if max_results == 0 || max_results > 500 {
                return Err(ToolDenial::rejected(ToolDenialCode::InvalidArguments));
            }
            (
                "rg",
                vec![
                    "--fixed-strings".to_owned(),
                    "--line-number".to_owned(),
                    "--no-heading".to_owned(),
                    "--color".to_owned(),
                    "never".to_owned(),
                    "--max-count".to_owned(),
                    max_results.to_string(),
                    "--".to_owned(),
                    pattern.to_owned(),
                    normalized_relative(target.relative()),
                ],
            )
        }
    };
    Ok(ProcessRequest::fixed(program, args, workspace.as_path()))
}

fn require_no_diagnostic_options(arguments: &DiagnosticArguments) -> Result<(), ToolDenial> {
    if arguments.path.is_some() || arguments.pattern.is_some() || arguments.max_results.is_some() {
        Err(ToolDenial::rejected(ToolDenialCode::InvalidArguments))
    } else {
        Ok(())
    }
}

pub(crate) fn reject_command_token(value: &str) -> Result<(), ToolDenial> {
    if value.starts_with(['-', '@'])
        || value.contains('\0')
        || value.contains(['\r', '\n'])
        || value.contains("://")
        || looks_like_environment_assignment(value)
    {
        Err(ToolDenial::rejected(ToolDenialCode::InvalidArguments))
    } else {
        Ok(())
    }
}

fn looks_like_environment_assignment(value: &str) -> bool {
    value.split_once('=').is_some_and(|(name, _)| {
        !name.is_empty()
            && name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    })
}

fn reject_search_pattern(pattern: &str) -> Result<(), ToolDenial> {
    reject_command_token(pattern)?;
    if pattern.contains(';')
        || pattern.contains("&&")
        || pattern.contains("||")
        || pattern.contains('`')
        || pattern.contains("$(")
    {
        Err(ToolDenial::rejected(ToolDenialCode::InvalidArguments))
    } else {
        Ok(())
    }
}

pub(crate) fn normalized_relative(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}
