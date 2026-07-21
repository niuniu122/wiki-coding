use std::io;
#[cfg(windows)]
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(target_os = "linux")]
use std::time::Duration;

use minimax_protocol::ShellSessionId;
use portable_pty::{CommandBuilder, MasterPty, PtySize};
#[cfg(windows)]
use win32job::{ExtendedLimitInfo, Job};

use super::{
    PtyBackend, PtyChild, PtyTerminateFuture, ShellManagerError, ShellSessionIdSource,
    ShellSpawnRequest, SpawnedPty,
};

const PROCESS_NONCE_BYTES: usize = 8;
#[cfg(windows)]
const WINDOWS_GATE_COMMAND_ENV: &str = "MINIMAX_SHELL_GATED_COMMAND";
#[cfg(windows)]
const WINDOWS_GATE_PATH_ENV: &str = "MINIMAX_SHELL_GATE_PATH";
#[cfg(windows)]
const WINDOWS_GATE_TOKEN_ENV: &str = "MINIMAX_SHELL_GATE_TOKEN";
#[cfg(windows)]
const WINDOWS_GATE_TIMEOUT_ENV: &str = "MINIMAX_SHELL_GATE_TIMEOUT_MS";
#[cfg(windows)]
const WINDOWS_GATE_TIMEOUT_MS: &str = "30000";
#[cfg(windows)]
const WINDOWS_GATE_WRAPPER: &str = r#"
$__minimax_shell_gate_71f5650d_path = [Environment]::GetEnvironmentVariable('MINIMAX_SHELL_GATE_PATH', 'Process')
$__minimax_shell_gate_71f5650d_token = [Environment]::GetEnvironmentVariable('MINIMAX_SHELL_GATE_TOKEN', 'Process')
$__minimax_shell_gate_71f5650d_timeout_text = [Environment]::GetEnvironmentVariable('MINIMAX_SHELL_GATE_TIMEOUT_MS', 'Process')
$__minimax_shell_gate_71f5650d_timeout_ms = 0
if ([String]::IsNullOrEmpty($__minimax_shell_gate_71f5650d_path) -or [String]::IsNullOrEmpty($__minimax_shell_gate_71f5650d_token)) { exit 125 }
if (-not [Int32]::TryParse($__minimax_shell_gate_71f5650d_timeout_text, [ref]$__minimax_shell_gate_71f5650d_timeout_ms) -or $__minimax_shell_gate_71f5650d_timeout_ms -lt 1 -or $__minimax_shell_gate_71f5650d_timeout_ms -gt 300000) { exit 125 }
$__minimax_shell_gate_71f5650d_wait = [Diagnostics.Stopwatch]::StartNew()
while (-not [IO.File]::Exists($__minimax_shell_gate_71f5650d_path)) {
    if ($__minimax_shell_gate_71f5650d_wait.ElapsedMilliseconds -ge $__minimax_shell_gate_71f5650d_timeout_ms) { exit 125 }
    [Threading.Thread]::Sleep(10)
}
try { $__minimax_shell_gate_71f5650d_observed = [IO.File]::ReadAllText($__minimax_shell_gate_71f5650d_path) } catch { exit 125 }
if ($__minimax_shell_gate_71f5650d_observed -cne $__minimax_shell_gate_71f5650d_token) { exit 125 }
$__minimax_shell_gate_71f5650d_command = [Environment]::GetEnvironmentVariable('MINIMAX_SHELL_GATED_COMMAND', 'Process')
[Environment]::SetEnvironmentVariable('MINIMAX_SHELL_GATED_COMMAND', $null, 'Process')
[Environment]::SetEnvironmentVariable('MINIMAX_SHELL_GATE_PATH', $null, 'Process')
[Environment]::SetEnvironmentVariable('MINIMAX_SHELL_GATE_TOKEN', $null, 'Process')
[Environment]::SetEnvironmentVariable('MINIMAX_SHELL_GATE_TIMEOUT_MS', $null, 'Process')
try { [IO.File]::Delete($__minimax_shell_gate_71f5650d_path) } catch { exit 125 }
if ($null -eq $__minimax_shell_gate_71f5650d_command) { exit 125 }
$__minimax_shell_gate_71f5650d_script_9c8e36a2 = [ScriptBlock]::Create($__minimax_shell_gate_71f5650d_command)
Remove-Variable -Name '__minimax_shell_gate_71f5650d_path','__minimax_shell_gate_71f5650d_token','__minimax_shell_gate_71f5650d_timeout_text','__minimax_shell_gate_71f5650d_timeout_ms','__minimax_shell_gate_71f5650d_wait','__minimax_shell_gate_71f5650d_observed','__minimax_shell_gate_71f5650d_command' -ErrorAction SilentlyContinue
& $__minimax_shell_gate_71f5650d_script_9c8e36a2
"#;

#[derive(Clone, Copy, Debug, Default)]
pub struct NativePtyBackend;

impl PtyBackend for NativePtyBackend {
    fn requires_cursor_handshake(&self) -> bool {
        cfg!(windows)
    }

    fn spawn(&self, request: &ShellSpawnRequest) -> io::Result<SpawnedPty> {
        let resolved = resolve_native_shell(&request.command)?;
        #[cfg(windows)]
        let mut windows_startup = WindowsStartupGate::new()?;
        let pair = portable_pty::native_pty_system()
            .openpty(PtySize {
                rows: request.rows,
                cols: request.cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(pty_error)?;
        let mut command = CommandBuilder::new(&resolved.program);
        #[cfg(windows)]
        windows_startup.prepare_command(&mut command, &request.command);
        #[cfg(not(windows))]
        command.args(&resolved.args);
        command.cwd(&request.cwd);
        let (mut child, reader, writer) = acquire_handles_then_spawn(
            || pair.master.try_clone_reader().map_err(pty_error),
            || pair.master.take_writer().map_err(pty_error),
            || pair.slave.spawn_command(command).map_err(pty_error),
        )?;
        drop(pair.slave);

        let process_id = process_id_or_cleanup(child.as_mut())?;
        #[cfg(target_os = "linux")]
        let process_group = match pair
            .master
            .process_group_leader()
            .and_then(rustix::process::Pid::from_raw)
        {
            Some(process_group)
                if u32::try_from(process_group.as_raw_pid()).ok() == Some(process_id) =>
            {
                process_group
            }
            Some(process_group) => {
                let cleanup = kill_and_wait_child(child.as_mut());
                return Err(io::Error::other(format!(
                    "PTY process-group leader {} did not match child {process_id}; {cleanup}",
                    process_group.as_raw_pid()
                )));
            }
            None => {
                let cleanup = kill_and_wait_child(child.as_mut());
                return Err(io::Error::other(format!(
                    "PTY did not expose a Linux process-group leader; {cleanup}"
                )));
            }
        };
        #[cfg(windows)]
        if let Err(error) = windows_startup.assign_and_release(child.as_ref()) {
            let cleanup = kill_and_wait_child(child.as_mut());
            return Err(io::Error::other(format!(
                "Windows Job startup gate failed: {error}; gated child cleanup: {cleanup}"
            )));
        }

        Ok(SpawnedPty {
            child: Box::new(NativePtyChild {
                child,
                process_id,
                #[cfg(target_os = "linux")]
                observed_exit: None,
                #[cfg(target_os = "linux")]
                reaped: false,
            }),
            reader,
            writer,
            guard: Box::new(NativePtyGuard {
                master: Some(pair.master),
                #[cfg(windows)]
                job: windows_startup.job.take(),
                #[cfg(windows)]
                gate_dir: windows_startup.gate_dir.take(),
                #[cfg(not(any(windows, target_os = "linux")))]
                process_id,
                #[cfg(target_os = "linux")]
                process_group,
                #[cfg(target_os = "linux")]
                destructive_complete: false,
                armed: true,
            }),
        })
    }
}

#[cfg(windows)]
struct WindowsStartupGate {
    job: Option<Job>,
    gate_dir: Option<tempfile::TempDir>,
    gate_path: PathBuf,
    gate_token: String,
}

#[cfg(windows)]
impl WindowsStartupGate {
    fn new() -> io::Result<Self> {
        let mut limits = ExtendedLimitInfo::new();
        limits.limit_kill_on_job_close();
        let job = Job::create_with_limit_info(&limits)
            .map_err(|error| io::Error::other(format!("create Windows Job: {error}")))?;
        let gate_dir = tempfile::Builder::new()
            .prefix("minimax-shell-gate-")
            .tempdir()?;
        let gate_path = gate_dir.path().join("ready");
        let mut token = [0_u8; 16];
        getrandom::fill(&mut token).map_err(|error| {
            io::Error::other(format!("generate Windows startup gate token: {error}"))
        })?;
        let gate_token = encode_lower_hex(&token);
        Ok(Self {
            job: Some(job),
            gate_dir: Some(gate_dir),
            gate_path,
            gate_token,
        })
    }

    fn prepare_command(&self, command: &mut CommandBuilder, user_command: &str) {
        command.args(["-NoLogo", "-NoProfile", "-Command", WINDOWS_GATE_WRAPPER]);
        command.env(WINDOWS_GATE_COMMAND_ENV, user_command);
        command.env(WINDOWS_GATE_PATH_ENV, &self.gate_path);
        command.env(WINDOWS_GATE_TOKEN_ENV, &self.gate_token);
        command.env(WINDOWS_GATE_TIMEOUT_ENV, WINDOWS_GATE_TIMEOUT_MS);
    }

    fn assign_and_release(
        &self,
        child: &(dyn portable_pty::Child + Send + Sync),
    ) -> io::Result<()> {
        let process_handle = child
            .as_raw_handle()
            .ok_or_else(|| io::Error::other("PTY child did not expose a raw process handle"))?;
        self.assign_and_release_with(process_handle as isize, |job, handle| {
            job.assign_process(handle).map_err(|error| {
                io::Error::other(format!("assign PTY child to Windows Job: {error}"))
            })
        })
    }

    fn assign_and_release_with(
        &self,
        process_handle: isize,
        assign: impl FnOnce(&Job, isize) -> io::Result<()>,
    ) -> io::Result<()> {
        assign(
            self.job
                .as_ref()
                .ok_or_else(|| io::Error::other("Windows Job is missing before assignment"))?,
            process_handle,
        )?;
        self.release()
    }

    fn release(&self) -> io::Result<()> {
        let pending_path = self
            .gate_dir
            .as_ref()
            .ok_or_else(|| io::Error::other("Windows startup gate directory is missing"))?
            .path()
            .join("ready.pending");
        let mut pending = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&pending_path)?;
        pending.write_all(self.gate_token.as_bytes())?;
        pending.sync_all()?;
        drop(pending);
        std::fs::rename(pending_path, &self.gate_path)
    }
}

#[cfg(windows)]
fn encode_lower_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

struct NativePtyChild {
    child: Box<dyn portable_pty::Child + Send + Sync>,
    process_id: u32,
    #[cfg(target_os = "linux")]
    observed_exit: Option<i32>,
    #[cfg(target_os = "linux")]
    reaped: bool,
}

impl PtyChild for NativePtyChild {
    fn process_id(&self) -> u32 {
        self.process_id
    }

    fn try_wait(&mut self) -> io::Result<Option<i32>> {
        #[cfg(target_os = "linux")]
        {
            if self.observed_exit.is_none() {
                self.observed_exit = linux_observe_exit(self.process_id)?;
            }
            Ok(self.observed_exit)
        }

        #[cfg(not(target_os = "linux"))]
        self.child
            .try_wait()
            .map(|status| status.map(|status| status.exit_code() as i32))
    }

    fn kill(&mut self) -> io::Result<()> {
        self.child.kill()
    }

    fn reap(&mut self) -> io::Result<()> {
        #[cfg(target_os = "linux")]
        {
            if !self.reaped {
                let status = self.child.wait()?;
                self.reaped = true;
                if self.observed_exit.is_none() {
                    self.observed_exit = Some(status.exit_code() as i32);
                }
            }
        }
        Ok(())
    }
}

#[cfg(target_os = "linux")]
impl Drop for NativePtyChild {
    fn drop(&mut self) {
        if self.reaped {
            return;
        }
        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) | Err(_) => {
                    self.reaped = true;
                    break;
                }
                Ok(None) if std::time::Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Ok(None) => break,
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn linux_observe_exit(process_id: u32) -> io::Result<Option<i32>> {
    let raw_pid = i32::try_from(process_id)
        .map_err(|_| io::Error::other("Linux child process ID exceeds i32"))?;
    let pid = rustix::process::Pid::from_raw(raw_pid)
        .ok_or_else(|| io::Error::other("Linux child process ID must be positive"))?;
    let status = rustix::process::waitid(
        rustix::process::WaitId::Pid(pid),
        rustix::process::WaitIdOptions::NOHANG
            | rustix::process::WaitIdOptions::NOWAIT
            | rustix::process::WaitIdOptions::EXITED,
    )
    .map_err(io::Error::from)?;
    Ok(status.map(|status| {
        status
            .exit_status()
            .or_else(|| status.terminating_signal().map(|signal| 128 + signal))
            .unwrap_or(1)
    }))
}

struct NativePtyGuard {
    master: Option<Box<dyn MasterPty + Send>>,
    #[cfg(windows)]
    job: Option<Job>,
    #[cfg(windows)]
    gate_dir: Option<tempfile::TempDir>,
    #[cfg(not(any(windows, target_os = "linux")))]
    process_id: u32,
    #[cfg(target_os = "linux")]
    process_group: rustix::process::Pid,
    #[cfg(target_os = "linux")]
    destructive_complete: bool,
    armed: bool,
}

impl super::backend::PtyGuard for NativePtyGuard {
    fn terminate<'a>(&'a mut self) -> PtyTerminateFuture<'a> {
        #[cfg(windows)]
        return Box::pin(async move {
            // This Job is unique and non-inherited. Windows' KILL_ON_JOB_CLOSE contract
            // terminates every associated process when this final Job handle closes.
            drop(self.job.take());
            self.armed = false;
            Ok(())
        });

        #[cfg(target_os = "linux")]
        return Box::pin(async move {
            linux_terminate_process_group(self.process_group, &mut self.destructive_complete).await
        });

        #[cfg(not(any(windows, target_os = "linux")))]
        Box::pin(async {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "native PTY containment is supported only on Windows and Linux",
            ))
        })
    }

    fn confirm<'a>(&'a mut self) -> PtyTerminateFuture<'a> {
        #[cfg(target_os = "linux")]
        return Box::pin(async move {
            if !self.destructive_complete {
                return Err(io::Error::other(
                    "Linux process-group confirmation preceded destructive cleanup",
                ));
            }
            linux_confirm_process_group_absent(self.process_group).await
        });

        #[cfg(windows)]
        return Box::pin(async {
            // Closing the final KILL_ON_JOB_CLOSE handle is the Windows containment
            // confirmation boundary; there is no reusable process-group ID to probe.
            Ok(())
        });

        #[cfg(not(any(windows, target_os = "linux")))]
        Box::pin(async { Ok(()) })
    }

    fn close_io(&mut self) {
        drop(self.master.take());
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for NativePtyGuard {
    fn drop(&mut self) {
        if self.armed {
            #[cfg(windows)]
            {
                drop(self.job.take());
            }
            #[cfg(target_os = "linux")]
            if !self.destructive_complete {
                linux_terminate_process_group_sync(self.process_group);
                self.destructive_complete = true;
            }
            #[cfg(not(any(windows, target_os = "linux")))]
            let _ = self.process_id;
        }
        #[cfg(windows)]
        drop(self.gate_dir.take());
    }
}

#[cfg(target_os = "linux")]
async fn linux_terminate_process_group(
    process_group: rustix::process::Pid,
    destructive_complete: &mut bool,
) -> io::Result<()> {
    if *destructive_complete {
        return Ok(());
    }
    match rustix::process::kill_process_group(process_group, rustix::process::Signal::TERM) {
        Ok(()) => {}
        Err(rustix::io::Errno::SRCH) => {
            *destructive_complete = true;
            return Ok(());
        }
        Err(error) => return Err(io::Error::from(error)),
    }
    tokio::time::sleep(Duration::from_millis(50)).await;
    match rustix::process::test_kill_process_group(process_group) {
        Err(rustix::io::Errno::SRCH) => {
            *destructive_complete = true;
            Ok(())
        }
        Ok(()) => {
            match rustix::process::kill_process_group(process_group, rustix::process::Signal::KILL)
            {
                Ok(()) | Err(rustix::io::Errno::SRCH) => {
                    *destructive_complete = true;
                    Ok(())
                }
                Err(error) => Err(io::Error::from(error)),
            }
        }
        Err(error) => Err(io::Error::from(error)),
    }
}

#[cfg(target_os = "linux")]
async fn linux_confirm_process_group_absent(process_group: rustix::process::Pid) -> io::Result<()> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
    loop {
        match rustix::process::test_kill_process_group(process_group) {
            Err(rustix::io::Errno::SRCH) => return Ok(()),
            Ok(()) if tokio::time::Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            Ok(()) => {
                return Err(io::Error::other(
                    "Linux process group remained observable after SIGKILL",
                ));
            }
            Err(error) => return Err(io::Error::from(error)),
        }
    }
}

#[cfg(target_os = "linux")]
fn linux_terminate_process_group_sync(process_group: rustix::process::Pid) {
    match rustix::process::kill_process_group(process_group, rustix::process::Signal::TERM) {
        Ok(()) => std::thread::sleep(Duration::from_millis(50)),
        Err(rustix::io::Errno::SRCH) => return,
        Err(_) => {}
    }
    let _ = rustix::process::kill_process_group(process_group, rustix::process::Signal::KILL);
}

#[derive(Debug)]
pub struct ProcessShellSessionIds {
    nonce: String,
    counter: AtomicU64,
}

impl ProcessShellSessionIds {
    pub fn new() -> Result<Self, ShellManagerError> {
        let mut nonce = [0_u8; PROCESS_NONCE_BYTES];
        getrandom::fill(&mut nonce).map_err(|_| ShellManagerError::Identifier)?;
        Ok(Self::from_nonce_and_counter(nonce, 0))
    }

    fn from_nonce_and_counter(nonce: [u8; PROCESS_NONCE_BYTES], counter: u64) -> Self {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut encoded = String::with_capacity(PROCESS_NONCE_BYTES * 2);
        for byte in nonce {
            encoded.push(char::from(HEX[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
        Self {
            nonce: encoded,
            counter: AtomicU64::new(counter),
        }
    }
}

impl ShellSessionIdSource for ProcessShellSessionIds {
    fn next_session_id(&self) -> Result<ShellSessionId, ShellManagerError> {
        let previous = self
            .counter
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |counter| {
                counter.checked_add(1)
            })
            .map_err(|_| ShellManagerError::Identifier)?;
        let counter = previous
            .checked_add(1)
            .ok_or(ShellManagerError::Identifier)?;
        ShellSessionId::new(format!("shell-{}-{counter:016x}", self.nonce))
            .map_err(|_| ShellManagerError::Identifier)
    }
}

#[derive(Debug, Eq, PartialEq)]
struct ResolvedShell {
    program: PathBuf,
    args: Vec<String>,
}

#[cfg(any(windows, test))]
fn resolve_windows_shell(
    command: &str,
    pwsh_candidates: &[PathBuf],
    powershell_candidate: &Path,
    is_executable: impl Fn(&Path) -> bool,
) -> io::Result<ResolvedShell> {
    let program = pwsh_candidates
        .iter()
        .find(|candidate| is_executable(candidate))
        .cloned()
        .or_else(|| is_executable(powershell_candidate).then(|| powershell_candidate.to_owned()))
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "PowerShell executable not found")
        })?;
    Ok(ResolvedShell {
        program,
        args: vec![
            "-NoLogo".to_owned(),
            "-NoProfile".to_owned(),
            "-Command".to_owned(),
            command.to_owned(),
        ],
    })
}

#[cfg(any(target_os = "linux", test))]
fn resolve_linux_shell(
    command: &str,
    requested_shell: Option<&Path>,
    bash_candidate: &Path,
    sh_candidate: &Path,
    is_executable: impl Fn(&Path) -> bool,
) -> io::Result<ResolvedShell> {
    let requested_shell = requested_shell
        .filter(|candidate| is_posix_absolute(candidate) && is_executable(candidate));
    let program = requested_shell
        .or_else(|| is_executable(bash_candidate).then_some(bash_candidate))
        .or_else(|| is_executable(sh_candidate).then_some(sh_candidate))
        .map(Path::to_owned)
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "POSIX shell executable not found")
        })?;
    Ok(ResolvedShell {
        program,
        args: vec!["-lc".to_owned(), command.to_owned()],
    })
}

#[cfg(any(target_os = "linux", test))]
fn is_posix_absolute(path: &Path) -> bool {
    path.as_os_str().as_encoded_bytes().first() == Some(&b'/')
}

#[cfg(windows)]
fn resolve_native_shell(command: &str) -> io::Result<ResolvedShell> {
    let pwsh_candidates = std::env::var_os("PATH")
        .map(|path| {
            std::env::split_paths(&path)
                .map(|directory| directory.join("pwsh.exe"))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let powershell = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .map(|root| {
            root.join("System32")
                .join("WindowsPowerShell")
                .join("v1.0")
                .join("powershell.exe")
        })
        .unwrap_or_default();
    resolve_windows_shell(command, &pwsh_candidates, &powershell, Path::is_file)
}

#[cfg(target_os = "linux")]
fn resolve_native_shell(command: &str) -> io::Result<ResolvedShell> {
    let requested_shell = std::env::var_os("SHELL").map(PathBuf::from);
    resolve_linux_shell(
        command,
        requested_shell.as_deref(),
        Path::new("/bin/bash"),
        Path::new("/bin/sh"),
        is_executable_for_current_process,
    )
}

#[cfg(target_os = "linux")]
fn is_executable_for_current_process(path: &Path) -> bool {
    is_executable_with_access_check(path, |candidate| {
        rustix::fs::access(candidate, rustix::fs::Access::EXEC_OK).is_ok()
    })
}

#[cfg(any(target_os = "linux", test))]
fn is_executable_with_access_check(path: &Path, check_x_ok: impl FnOnce(&Path) -> bool) -> bool {
    path.is_file() && check_x_ok(path)
}

#[cfg(not(any(windows, target_os = "linux")))]
fn resolve_native_shell(_command: &str) -> io::Result<ResolvedShell> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "native PTY shell is supported only on Windows and Linux",
    ))
}

fn pty_error(error: impl std::fmt::Display) -> io::Error {
    io::Error::other(error.to_string())
}

fn acquire_handles_then_spawn<Child, Reader, Writer>(
    acquire_reader: impl FnOnce() -> io::Result<Reader>,
    acquire_writer: impl FnOnce() -> io::Result<Writer>,
    spawn: impl FnOnce() -> io::Result<Child>,
) -> io::Result<(Child, Reader, Writer)> {
    let reader = acquire_reader()?;
    let writer = acquire_writer()?;
    let child = spawn()?;
    Ok((child, reader, writer))
}

fn process_id_or_cleanup(child: &mut (dyn portable_pty::Child + Send + Sync)) -> io::Result<u32> {
    if let Some(process_id) = child.process_id() {
        return Ok(process_id);
    }

    let cleanup = kill_and_wait_child(child);
    Err(io::Error::other(format!(
        "PTY child did not expose a process ID; {cleanup}"
    )))
}

fn kill_and_wait_child(child: &mut (dyn portable_pty::Child + Send + Sync)) -> String {
    let kill_error = child.kill().err();
    let wait_error = child.wait().err();
    match (kill_error, wait_error) {
        (None, None) => "direct kill and wait completed".to_owned(),
        (Some(kill), None) => format!("direct kill failed: {kill}; wait completed"),
        (None, Some(wait)) => format!("direct kill completed; wait failed: {wait}"),
        (Some(kill), Some(wait)) => {
            format!("direct kill failed: {kill}; wait failed: {wait}")
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::io::{self, Cursor};
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::{
        NativePtyBackend, ProcessShellSessionIds, acquire_handles_then_spawn,
        is_executable_with_access_check, process_id_or_cleanup, resolve_linux_shell,
        resolve_windows_shell,
    };
    #[cfg(windows)]
    use super::{
        WINDOWS_GATE_COMMAND_ENV, WINDOWS_GATE_PATH_ENV, WINDOWS_GATE_TIMEOUT_ENV,
        WINDOWS_GATE_TOKEN_ENV, WINDOWS_GATE_WRAPPER, WindowsStartupGate, resolve_native_shell,
    };
    use crate::shell::{PtyBackend, ShellManagerError, ShellSessionIdSource};

    #[test]
    fn native_backend_requires_startup_cursor_handshake_only_on_windows() {
        assert_eq!(NativePtyBackend.requires_cursor_handshake(), cfg!(windows));
    }

    #[test]
    fn native_startup_acquires_both_fallible_master_handles_before_spawn() {
        type HandleResult = io::Result<((), Cursor<Vec<u8>>, Cursor<Vec<u8>>)>;

        let spawn_count = AtomicUsize::new(0);
        let reader_failure: HandleResult = acquire_handles_then_spawn(
            || Err(io::Error::other("reader acquisition failed")),
            || Ok(Cursor::new(Vec::new())),
            || {
                spawn_count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
        );
        assert_eq!(
            reader_failure
                .expect_err("reader acquisition must fail")
                .kind(),
            io::ErrorKind::Other
        );

        let writer_failure: HandleResult = acquire_handles_then_spawn(
            || Ok(Cursor::new(Vec::new())),
            || Err(io::Error::other("writer acquisition failed")),
            || {
                spawn_count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
        );
        assert_eq!(
            writer_failure
                .expect_err("writer acquisition must fail")
                .kind(),
            io::ErrorKind::Other
        );
        assert_eq!(
            spawn_count.load(Ordering::SeqCst),
            0,
            "no child may be spawned until both fallible master handles exist"
        );
    }

    #[derive(Debug)]
    struct MissingPidChild {
        kills: Arc<AtomicUsize>,
        waits: Arc<AtomicUsize>,
    }

    impl portable_pty::ChildKiller for MissingPidChild {
        fn kill(&mut self) -> io::Result<()> {
            self.kills.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn clone_killer(&self) -> Box<dyn portable_pty::ChildKiller + Send + Sync> {
            Box::new(Self {
                kills: Arc::clone(&self.kills),
                waits: Arc::clone(&self.waits),
            })
        }
    }

    impl portable_pty::Child for MissingPidChild {
        fn try_wait(&mut self) -> io::Result<Option<portable_pty::ExitStatus>> {
            Ok(None)
        }

        fn wait(&mut self) -> io::Result<portable_pty::ExitStatus> {
            self.waits.fetch_add(1, Ordering::SeqCst);
            Ok(portable_pty::ExitStatus::with_exit_code(1))
        }

        fn process_id(&self) -> Option<u32> {
            None
        }

        #[cfg(windows)]
        fn as_raw_handle(&self) -> Option<std::os::windows::io::RawHandle> {
            None
        }
    }

    #[test]
    fn native_startup_missing_process_id_directly_kills_and_waits() {
        let kills = Arc::new(AtomicUsize::new(0));
        let waits = Arc::new(AtomicUsize::new(0));
        let mut child = MissingPidChild {
            kills: Arc::clone(&kills),
            waits: Arc::clone(&waits),
        };

        let error = process_id_or_cleanup(&mut child).expect_err("missing PID must fail startup");

        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert_eq!(kills.load(Ordering::SeqCst), 1);
        assert_eq!(waits.load(Ordering::SeqCst), 1);
    }

    #[cfg(windows)]
    #[test]
    fn windows_startup_gate_wrapper_times_out_without_a_marker() {
        use std::os::windows::process::CommandExt as _;
        use std::process::{Command, Stdio};
        use std::time::{Duration, Instant};

        let fixture = tempfile::tempdir().expect("gate timeout fixture");
        let gate_path = fixture.path().join("never-ready");
        let side_effect = fixture.path().join("must-not-run.txt");
        let escaped_side_effect = side_effect.to_string_lossy().replace('\'', "''");
        let user_command =
            format!("[IO.File]::WriteAllText('{escaped_side_effect}', 'unexpected execution')");
        let shell = resolve_native_shell("unused").expect("native PowerShell resolves");
        let mut child = Command::new(shell.program)
            .args(["-NoLogo", "-NoProfile", "-Command", WINDOWS_GATE_WRAPPER])
            .env(WINDOWS_GATE_COMMAND_ENV, user_command)
            .env(WINDOWS_GATE_PATH_ENV, &gate_path)
            .env(WINDOWS_GATE_TOKEN_ENV, "never-released")
            .env(WINDOWS_GATE_TIMEOUT_ENV, "200")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(0x0800_0000)
            .spawn()
            .expect("gated PowerShell starts");
        let deadline = Instant::now() + Duration::from_secs(4);
        let status = loop {
            if let Some(status) = child.try_wait().expect("gated PowerShell status") {
                break status;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("startup gate did not self-terminate without a marker");
            }
            std::thread::sleep(Duration::from_millis(10));
        };

        assert_eq!(status.code(), Some(125));
        assert!(!side_effect.exists(), "gated user command must not execute");
    }

    #[cfg(windows)]
    #[test]
    fn windows_assignment_failure_keeps_the_user_command_gate_closed() {
        let gate = WindowsStartupGate::new().expect("Windows startup gate");

        let error = gate
            .assign_and_release_with(0, |_job, _handle| {
                Err(io::Error::other("scripted assignment failure"))
            })
            .expect_err("assignment must fail");

        assert!(error.to_string().contains("scripted assignment failure"));
        assert!(
            !gate.gate_path.exists(),
            "the marker must remain absent, so the user command cannot run"
        );
    }

    #[test]
    fn linux_x_ok_result_controls_production_executability_instead_of_mode_bits() {
        let fixture = tempfile::tempdir().expect("x-ok fixture");
        let candidate = fixture.path().join("candidate-shell");
        std::fs::write(&candidate, []).expect("candidate fixture");
        let calls = AtomicUsize::new(0);

        let executable = is_executable_with_access_check(&candidate, |path| {
            calls.fetch_add(1, Ordering::SeqCst);
            assert_eq!(path, candidate);
            false
        });

        assert!(!executable, "a denied X_OK check must remain denied");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_x_ok_rejects_a_file_without_execute_permission() {
        use std::os::unix::fs::PermissionsExt as _;

        let fixture = tempfile::tempdir().expect("x-ok fixture");
        let candidate = fixture.path().join("candidate-shell");
        std::fs::write(&candidate, b"#!/bin/sh\nexit 0\n").expect("candidate fixture");
        let mut permissions = std::fs::metadata(&candidate)
            .expect("candidate metadata")
            .permissions();
        permissions.set_mode(0o000);
        std::fs::set_permissions(&candidate, permissions).expect("candidate permissions");
        assert_eq!(
            std::fs::metadata(&candidate)
                .expect("candidate metadata")
                .permissions()
                .mode()
                & 0o111,
            0,
            "the fixture must not have execute permission"
        );
        assert!(!super::is_executable_for_current_process(&candidate));
    }

    #[test]
    fn native_shell_resolution_windows_prefers_pwsh_then_powershell_and_never_cmd() {
        let fixture = tempfile::tempdir().expect("shell fixture");
        let pwsh = fixture.path().join("pwsh.exe");
        let powershell = fixture.path().join("powershell.exe");
        let cmd = fixture.path().join("cmd.exe");
        for executable in [&pwsh, &powershell, &cmd] {
            std::fs::write(executable, []).expect("shell executable fixture");
        }

        let resolved = resolve_windows_shell(
            "Write-Output ok",
            std::slice::from_ref(&pwsh),
            &powershell,
            Path::is_file,
        )
        .expect("pwsh resolution");
        assert_eq!(resolved.program, pwsh);
        assert_eq!(
            resolved.args,
            ["-NoLogo", "-NoProfile", "-Command", "Write-Output ok"]
        );

        std::fs::remove_file(&resolved.program).expect("remove pwsh fixture");
        let resolved = resolve_windows_shell(
            "Write-Output fallback",
            std::slice::from_ref(&resolved.program),
            &powershell,
            Path::is_file,
        )
        .expect("Windows PowerShell resolution");
        assert_eq!(resolved.program, powershell);
        assert_eq!(
            resolved.args,
            ["-NoLogo", "-NoProfile", "-Command", "Write-Output fallback"]
        );

        std::fs::remove_file(&resolved.program).expect("remove powershell fixture");
        let error = resolve_windows_shell(
            "echo must-not-use-cmd",
            &[fixture.path().join("pwsh.exe")],
            &fixture.path().join("powershell.exe"),
            Path::is_file,
        )
        .expect_err("cmd.exe must never be selected");
        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
        assert!(
            cmd.is_file(),
            "cmd fixture proves it was deliberately ignored"
        );
    }

    #[test]
    fn native_shell_resolution_linux_prefers_absolute_executable_shell_then_bash_then_sh() {
        let requested = PathBuf::from("/opt/user/bin/zsh");
        let relative = PathBuf::from("opt/user/bin/fish");
        let bash = PathBuf::from("/bin/bash");
        let sh = PathBuf::from("/bin/sh");

        let executable = HashSet::from([
            requested.clone(),
            relative.clone(),
            bash.clone(),
            sh.clone(),
        ]);
        let resolved = resolve_linux_shell(
            "printf ok",
            Some(requested.as_path()),
            &bash,
            &sh,
            |candidate| executable.contains(candidate),
        )
        .expect("absolute executable SHELL");
        assert_eq!(resolved.program, requested);
        assert_eq!(resolved.args, ["-lc", "printf ok"]);

        let resolved = resolve_linux_shell(
            "printf bash",
            Some(relative.as_path()),
            &bash,
            &sh,
            |candidate| executable.contains(candidate),
        )
        .expect("relative SHELL must be ignored");
        assert_eq!(resolved.program, bash);
        assert_eq!(resolved.args, ["-lc", "printf bash"]);

        let only_sh = HashSet::from([sh.clone()]);
        let resolved = resolve_linux_shell(
            "printf sh",
            Some(Path::new("/missing/shell")),
            Path::new("/missing/bash"),
            &sh,
            |candidate| only_sh.contains(candidate),
        )
        .expect("sh fallback");
        assert_eq!(resolved.program, sh);
        assert_eq!(resolved.args, ["-lc", "printf sh"]);
    }

    #[test]
    fn process_shell_session_ids_report_identifier_when_the_counter_is_exhausted() {
        let ids = ProcessShellSessionIds::from_nonce_and_counter([0xab; 8], u64::MAX);
        assert!(matches!(
            ids.next_session_id(),
            Err(ShellManagerError::Identifier)
        ));
    }
}
