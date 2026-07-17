use std::collections::BTreeMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use minimax_core::{CancellationPort, ToolSandboxPolicy};
use minimax_protocol::{SchemaVersion, ToolInvocation, ToolResult, ToolTerminalStatus};
use serde::Deserialize;
use serde::Serialize;
use tokio::io::AsyncReadExt as _;
use tokio::process::{Child, ChildStderr, ChildStdout, Command};

use crate::WorkspaceRoot;
use crate::error::{ToolDenial, ToolDenialCode};
use crate::policy::Preflight;
use crate::sandbox::restricted_command;
#[cfg(test)]
use crate::sandbox::{bubblewrap_args, network_seccomp_program};
#[cfg(target_os = "linux")]
use crate::sandbox::{discover_bubblewrap, verify_bubblewrap_backend};

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

    #[cfg(target_os = "linux")]
    pub(crate) fn with_program(&self, program: String) -> Self {
        let mut request = self.clone();
        request.program = program;
        request
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
    fn spawn(
        &self,
        request: &ProcessRequest,
        sandbox_policy: ToolSandboxPolicy,
    ) -> Result<Box<dyn DirectChild>, ProcessLaunchError>;
}

#[derive(Debug)]
pub enum ProcessLaunchError {
    Io(std::io::Error),
    SandboxUnavailable(SandboxLaunchReceipt),
    SandboxDenied(SandboxLaunchReceipt),
}

impl ProcessLaunchError {
    #[must_use]
    pub const fn sandbox_unavailable(
        backend: &'static str,
        platform: &'static str,
        remediation: &'static str,
    ) -> Self {
        Self::SandboxUnavailable(SandboxLaunchReceipt {
            backend,
            platform,
            remediation,
        })
    }

    #[must_use]
    pub const fn sandbox_denied(
        backend: &'static str,
        platform: &'static str,
        remediation: &'static str,
    ) -> Self {
        Self::SandboxDenied(SandboxLaunchReceipt {
            backend,
            platform,
            remediation,
        })
    }
}

impl From<std::io::Error> for ProcessLaunchError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct SandboxLaunchReceipt {
    backend: &'static str,
    platform: &'static str,
    remediation: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SandboxCapabilityState {
    Enforced,
    Unavailable,
    Unsupported,
    DisabledByFullAccess,
}

impl SandboxCapabilityState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Enforced => "enforced",
            Self::Unavailable => "unavailable",
            Self::Unsupported => "unsupported",
            Self::DisabledByFullAccess => "disabled-by-full-access",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SandboxCapability {
    backend: &'static str,
    platform: &'static str,
    state: SandboxCapabilityState,
    detail: &'static str,
}

impl SandboxCapability {
    #[must_use]
    pub fn detect(workspace: &Path) -> Self {
        detect_sandbox_capability(workspace)
    }

    #[must_use]
    pub const fn backend(self) -> &'static str {
        self.backend
    }

    #[must_use]
    pub const fn platform(self) -> &'static str {
        self.platform
    }

    #[must_use]
    pub const fn state(self) -> SandboxCapabilityState {
        self.state
    }

    #[must_use]
    pub const fn detail(self) -> &'static str {
        self.detail
    }
}

#[cfg(target_os = "linux")]
fn detect_sandbox_capability(workspace: &Path) -> SandboxCapability {
    let Ok(bwrap) = discover_bubblewrap(workspace) else {
        return SandboxCapability {
            backend: "bubblewrap",
            platform: "linux",
            state: SandboxCapabilityState::Unavailable,
            detail: "Bubblewrap is missing; confirm-mode process execution fails closed",
        };
    };
    let Ok(home) = tempfile::tempdir() else {
        return SandboxCapability {
            backend: "bubblewrap",
            platform: "linux",
            state: SandboxCapabilityState::Unavailable,
            detail: "the private sandbox home cannot be created; confirm-mode process execution fails closed",
        };
    };
    let request = ProcessRequest::fixed("/bin/true", Vec::new(), workspace);
    if verify_bubblewrap_backend(&bwrap, &request, home.path(), &[]).is_ok() {
        SandboxCapability {
            backend: "bubblewrap+seccomp",
            platform: "linux",
            state: SandboxCapabilityState::Enforced,
            detail: "confirm-mode uses Bubblewrap plus a syscall filter for workspace-scoped writes and denied child network",
        }
    } else {
        SandboxCapability {
            backend: "bubblewrap",
            platform: "linux",
            state: SandboxCapabilityState::Unavailable,
            detail: "Bubblewrap cannot create the required namespaces or seccomp filter; on Ubuntu 24.04 check the AppArmor userns policy; confirm-mode process execution fails closed",
        }
    }
}

#[cfg(target_os = "windows")]
const fn detect_sandbox_capability(_workspace: &Path) -> SandboxCapability {
    SandboxCapability {
        backend: "windows_native",
        platform: "windows",
        state: SandboxCapabilityState::Unsupported,
        detail: "no native Windows backend is bundled; confirm-mode process execution fails closed",
    }
}

#[cfg(target_os = "macos")]
const fn detect_sandbox_capability(_workspace: &Path) -> SandboxCapability {
    SandboxCapability {
        backend: "seatbelt",
        platform: "macos",
        state: SandboxCapabilityState::Unsupported,
        detail: "macOS is a deferred platform; confirm-mode process tools fail closed",
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
const fn detect_sandbox_capability(_workspace: &Path) -> SandboxCapability {
    SandboxCapability {
        backend: "unsupported",
        platform: "unsupported",
        state: SandboxCapabilityState::Unsupported,
        detail: "this platform has no sandbox backend; confirm-mode process tools fail closed",
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TokioProcessLauncher;

impl ProcessLauncher for TokioProcessLauncher {
    fn spawn(
        &self,
        request: &ProcessRequest,
        sandbox_policy: ToolSandboxPolicy,
    ) -> Result<Box<dyn DirectChild>, ProcessLaunchError> {
        let (mut command, sandbox_home, sandbox_filter) = match sandbox_policy {
            ToolSandboxPolicy::Disabled => {
                let mut command = Command::new(&request.program);
                command.args(&request.args).env_clear().envs(&request.env);
                (command, None, None)
            }
            ToolSandboxPolicy::Restricted => restricted_command(request)?,
        };
        command
            .current_dir(&request.cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(unix)]
        command.process_group(0);
        #[cfg(windows)]
        command.creation_flags(0x0800_0000 | 0x0000_0200);
        let mut child = command.spawn().map_err(ProcessLaunchError::Io)?;
        drop(sandbox_filter);
        let process_id = child.id().ok_or_else(|| {
            ProcessLaunchError::Io(std::io::Error::other("missing child process id"))
        })?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| ProcessLaunchError::Io(std::io::Error::other("missing child stdout")))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| ProcessLaunchError::Io(std::io::Error::other("missing child stderr")))?;
        Ok(Box::new(TokioDirectChild {
            child,
            stdout,
            stderr,
            stdout_closed: false,
            stderr_closed: false,
            exit_emitted: false,
            process_id,
            _sandbox_home: sandbox_home,
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
    _sandbox_home: Option<tempfile::TempDir>,
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
        self.run_with_policy(request, ToolSandboxPolicy::Restricted, cancellation)
            .await
    }

    pub async fn run_with_policy(
        &self,
        request: &ProcessRequest,
        _sandbox_policy: ToolSandboxPolicy,
        cancellation: &dyn CancellationPort,
    ) -> ProcessCompletion {
        if cancellation.is_cancelled() {
            return ProcessCompletion::cancelled();
        }
        let mut child = match self.launcher.spawn(request, _sandbox_policy) {
            Ok(child) => child,
            Err(error) => return ProcessCompletion::launch_failed(error),
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
    error: Option<SandboxLaunchReceipt>,
}

impl ProcessCompletion {
    fn cancelled() -> Self {
        Self {
            status: ToolTerminalStatus::Cancelled,
            code: ToolDenialCode::Cancelled,
            exit_code: None,
            stdout: None,
            error: None,
        }
    }

    fn failed(code: ToolDenialCode, exit_code: Option<i32>, stdout: Option<String>) -> Self {
        Self {
            status: ToolTerminalStatus::Failed,
            code,
            exit_code,
            stdout,
            error: None,
        }
    }

    fn indeterminate() -> Self {
        Self {
            status: ToolTerminalStatus::Indeterminate,
            code: ToolDenialCode::CleanupUnknown,
            exit_code: None,
            stdout: None,
            error: None,
        }
    }

    fn succeeded(stdout: Option<String>) -> Self {
        Self {
            status: ToolTerminalStatus::Succeeded,
            code: ToolDenialCode::NonzeroExit,
            exit_code: Some(0),
            stdout,
            error: None,
        }
    }

    fn launch_failed(error: ProcessLaunchError) -> Self {
        match error {
            ProcessLaunchError::Io(_) => Self::failed(ToolDenialCode::SpawnFailed, None, None),
            ProcessLaunchError::SandboxUnavailable(error) => Self {
                status: ToolTerminalStatus::Failed,
                code: ToolDenialCode::SandboxUnavailable,
                exit_code: None,
                stdout: None,
                error: Some(error),
            },
            ProcessLaunchError::SandboxDenied(error) => Self {
                status: ToolTerminalStatus::Failed,
                code: ToolDenialCode::SandboxDenied,
                exit_code: None,
                stdout: None,
                error: Some(error),
            },
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
        let output = if self.exit_code.is_some() || self.stdout.is_some() || self.error.is_some() {
            let receipt = ProcessReceipt {
                exit_code: self.exit_code,
                stdout: self.stdout,
                error: self.error,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<SandboxLaunchReceipt>,
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
        let request = match prepare_diagnostic(workspace, invocation, cancellation) {
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

#[cfg(test)]
mod sandbox_tests {
    use super::*;

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_reports_unsupported_instead_of_claiming_partial_isolation() {
        let workspace = tempfile::tempdir().expect("workspace");
        let capability = SandboxCapability::detect(workspace.path());
        assert_eq!(capability.backend(), "windows_native");
        assert_eq!(capability.platform(), "windows");
        assert_eq!(capability.state(), SandboxCapabilityState::Unsupported);
        assert!(capability.detail().contains("fails closed"));
    }

    #[test]
    fn bubblewrap_plan_denies_network_and_exposes_only_the_workspace_as_writable() {
        let workspace = tempfile::tempdir().expect("workspace");
        std::fs::create_dir(workspace.path().join(".git")).expect("git metadata");
        std::fs::create_dir(workspace.path().join(".wiki-coding")).expect("runtime metadata");
        std::fs::create_dir(workspace.path().join(".obsidian")).expect("obsidian metadata");
        std::fs::create_dir(workspace.path().join(".minimax-runtime"))
            .expect("legacy runtime metadata");
        let home = tempfile::tempdir().expect("sandbox home");
        let request = ProcessRequest::fixed(
            "cargo",
            vec!["fmt".to_owned(), "--check".to_owned()],
            workspace.path(),
        );

        let args = bubblewrap_args(&request, home.path(), &[], Some(9));
        let rendered = args
            .iter()
            .map(|value| value.to_string_lossy().replace('\\', "/"))
            .collect::<Vec<_>>();
        for namespace_flag in [
            "--unshare-user",
            "--unshare-ipc",
            "--unshare-pid",
            "--unshare-net",
            "--unshare-uts",
            "--unshare-cgroup-try",
        ] {
            assert!(rendered.contains(&namespace_flag.into()));
        }
        assert!(!rendered.contains(&"--disable-userns".into()));
        assert!(!rendered.contains(&"--share-net".into()));
        assert!(
            rendered
                .windows(2)
                .any(|window| window == ["--seccomp", "9"]),
            "the network namespace must be backed by a syscall filter"
        );
        assert!(rendered.contains(&"--die-with-parent".into()));
        assert!(rendered.contains(&"--new-session".into()));
        assert!(rendered.windows(3).any(|window| {
            window[0] == "--bind"
                && window[1] == workspace.path().to_string_lossy().replace('\\', "/")
                && window[2] == "/workspace"
        }));
        assert!(rendered.windows(3).any(|window| {
            window[0] == "--ro-bind"
                && window[1]
                    == workspace
                        .path()
                        .join(".git")
                        .to_string_lossy()
                        .replace('\\', "/")
                && window[2] == "/workspace/.git"
        }));
        assert!(rendered.windows(3).any(|window| {
            window[0] == "--ro-bind"
                && window[1]
                    == workspace
                        .path()
                        .join(".wiki-coding")
                        .to_string_lossy()
                        .replace('\\', "/")
                && window[2] == "/workspace/.wiki-coding"
        }));
        for protected in [".obsidian", ".minimax-runtime"] {
            assert!(rendered.windows(3).any(|window| {
                window[0] == "--ro-bind"
                    && window[1]
                        == workspace
                            .path()
                            .join(protected)
                            .to_string_lossy()
                            .replace('\\', "/")
                    && window[2] == format!("/workspace/{protected}")
            }));
        }
        assert!(rendered.windows(3).any(|window| {
            window[0] == "--setenv" && window[1] == "HOME" && window[2] == "/tmp/wiki-coding-home"
        }));
        assert_eq!(
            rendered[rendered.len() - 4..],
            ["--", "cargo", "fmt", "--check"]
        );
    }

    #[test]
    fn bubblewrap_plan_mounts_rust_caches_without_cargo_credentials() {
        let workspace = tempfile::tempdir().expect("workspace");
        let home = tempfile::tempdir().expect("host home");
        let sandbox_home = tempfile::tempdir().expect("sandbox home");
        let cargo_home = home.path().join(".cargo");
        let rustup_home = home.path().join(".rustup");
        for relative in ["bin", "registry", "git"] {
            std::fs::create_dir_all(cargo_home.join(relative)).expect("cargo cache");
        }
        std::fs::create_dir_all(&rustup_home).expect("rustup home");
        std::fs::write(cargo_home.join("credentials.toml"), "token='secret'")
            .expect("credential fixture");
        let request = ProcessRequest::fixed("cargo", vec!["check".to_owned()], workspace.path());
        let runtime_mounts = vec![
            cargo_home.join("bin"),
            cargo_home.join("registry"),
            cargo_home.join("git"),
            rustup_home.clone(),
        ];

        let rendered = bubblewrap_args(&request, sandbox_home.path(), &runtime_mounts, Some(9))
            .iter()
            .map(|value| value.to_string_lossy().replace('\\', "/"))
            .collect::<Vec<_>>();
        let cargo_home = cargo_home.to_string_lossy().replace('\\', "/");
        let rustup_home = rustup_home.to_string_lossy().replace('\\', "/");
        assert!(rendered.windows(3).any(|window| {
            window[0] == "--setenv" && window[1] == "CARGO_HOME" && window[2] == cargo_home
        }));
        assert!(rendered.windows(3).any(|window| {
            window[0] == "--setenv" && window[1] == "RUSTUP_HOME" && window[2] == rustup_home
        }));
        assert!(rendered.iter().all(|value| !value.contains("credentials")));
        assert!(
            rendered
                .iter()
                .all(|value| !value.contains("token='secret'"))
        );
    }

    #[test]
    fn network_seccomp_filter_denies_sockets_keyrings_and_io_uring_on_x86_64() {
        let bytes = network_seccomp_program();
        assert_eq!(bytes.len(), 17 * 8);
        let instructions = bytes
            .chunks_exact(8)
            .map(|instruction| {
                (
                    u16::from_ne_bytes([instruction[0], instruction[1]]),
                    instruction[2],
                    instruction[3],
                    u32::from_ne_bytes([
                        instruction[4],
                        instruction[5],
                        instruction[6],
                        instruction[7],
                    ]),
                )
            })
            .collect::<Vec<_>>();
        let denied = instructions
            .iter()
            .skip(6)
            .step_by(2)
            .take(5)
            .map(|instruction| instruction.3)
            .collect::<Vec<_>>();
        assert_eq!(denied, [41, 248, 249, 250, 425]);
        assert_eq!(instructions[4].3, 0x4000_0000, "x32 ABI must be denied");
        assert_eq!(
            instructions.last().map(|instruction| instruction.3),
            Some(0x7fff_0000)
        );
    }

    #[cfg(unix)]
    #[test]
    fn bubblewrap_plan_does_not_follow_protected_metadata_symlinks() {
        use std::os::unix::fs::symlink;

        let workspace = tempfile::tempdir().expect("workspace");
        let host = tempfile::tempdir().expect("host fixture");
        let sandbox_home = tempfile::tempdir().expect("sandbox home");
        symlink(host.path(), workspace.path().join(".git")).expect("metadata symlink");
        let request = ProcessRequest::fixed("git", vec!["status".to_owned()], workspace.path());

        let rendered = bubblewrap_args(&request, sandbox_home.path(), &[], Some(9))
            .iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        let host_path = host.path().to_string_lossy().into_owned();
        assert!(
            rendered.iter().all(|value| value != &host_path),
            "protected overlays must never expose a symlink target from the host"
        );
    }
}
