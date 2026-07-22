use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(any(windows, target_os = "linux"))]
use std::time::Duration;

use minimax_protocol::ShellSessionId;
use portable_pty::{CommandBuilder, MasterPty, PtySize};
#[cfg(windows)]
use std::os::windows::io::AsRawHandle;
#[cfg(windows)]
use win32job::{ExtendedLimitInfo, Job};

use super::{
    ShellBackend, ShellChild, ShellIoMode, ShellManagerError, ShellSessionIdSource,
    ShellSpawnRequest, ShellTerminateFuture, SpawnedShell,
};

const PROCESS_NONCE_BYTES: usize = 8;
#[cfg(any(windows, target_os = "linux"))]
const HOST_STARTUP_TIMEOUT: Duration = Duration::from_secs(30);
#[cfg(target_os = "linux")]
const HOST_CLEANUP_TIMEOUT: Duration = Duration::from_secs(1);

#[allow(dead_code)]
fn build_host_command(
    trusted_host: &Path,
    bootstrap: &super::host::HostBootstrap,
    cwd: &Path,
) -> CommandBuilder {
    build_host_command_with_environment(trusted_host, bootstrap, cwd, std::env::vars_os())
}

fn build_host_command_with_environment(
    trusted_host: &Path,
    bootstrap: &super::host::HostBootstrap,
    cwd: &Path,
    environment: impl IntoIterator<Item = (OsString, OsString)>,
) -> CommandBuilder {
    let mut command = CommandBuilder::new(trusted_host);
    command.args(bootstrap.arguments());
    for (key, value) in environment {
        command.env(key, value);
    }
    for (key, value) in bootstrap.environment() {
        command.env(key, value);
    }
    command.cwd(cwd);
    command
}

#[cfg(any(windows, target_os = "linux"))]
fn build_host_process_command(
    trusted_host: &Path,
    bootstrap: &super::host::HostBootstrap,
    cwd: &Path,
) -> Command {
    let mut command = Command::new(trusted_host);
    command.args(bootstrap.arguments());
    for (key, value) in bootstrap.environment() {
        command.env(key, value);
    }
    command.current_dir(cwd);
    command
}

#[derive(Clone, Debug)]
enum HostExecutable {
    CurrentExecutable,
    Fixed(PathBuf),
}

#[derive(Clone)]
pub struct NativeShellBackend {
    host_executable: HostExecutable,
    startup_observer: Option<Arc<dyn Fn(&'static str) + Send + Sync>>,
}

impl std::fmt::Debug for NativeShellBackend {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NativeShellBackend")
            .field("host_executable", &self.host_executable)
            .field("startup_observer", &self.startup_observer.is_some())
            .finish()
    }
}

impl Default for NativeShellBackend {
    fn default() -> Self {
        Self {
            host_executable: HostExecutable::CurrentExecutable,
            startup_observer: None,
        }
    }
}

impl NativeShellBackend {
    /// Overrides the trusted internal-host executable for integration tests.
    #[doc(hidden)]
    pub fn with_host_executable(path: PathBuf) -> io::Result<Self> {
        if !path.is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "trusted shell host path must be absolute",
            ));
        }
        Ok(Self {
            host_executable: HostExecutable::Fixed(path),
            startup_observer: None,
        })
    }

    /// Installs a secret-free Windows startup observer for integration tests.
    #[doc(hidden)]
    #[must_use]
    pub fn with_startup_observer(
        mut self,
        observer: Arc<dyn Fn(&'static str) + Send + Sync>,
    ) -> Self {
        self.startup_observer = Some(observer);
        self
    }

    fn observe_startup(&self, stage: &'static str) {
        if let Some(observer) = &self.startup_observer {
            observer(stage);
        }
    }

    #[cfg(any(windows, target_os = "linux"))]
    fn resolve_host_executable(&self) -> io::Result<PathBuf> {
        let path = match &self.host_executable {
            HostExecutable::CurrentExecutable => std::env::current_exe()?,
            HostExecutable::Fixed(path) => path.clone(),
        };
        if !path.is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "trusted shell host path must be absolute",
            ));
        }
        Ok(path)
    }

    #[cfg(windows)]
    fn spawn_terminal_windows(
        &self,
        request: &ShellSpawnRequest,
        cols: u16,
        rows: u16,
    ) -> io::Result<SpawnedShell> {
        let trusted_host = self.resolve_host_executable()?;
        let (listener, bootstrap) =
            super::host::HostListener::bind(HOST_STARTUP_TIMEOUT).map_err(io::Error::from)?;
        self.observe_startup("listener_bound");
        let mut job = WindowsJobBoundary::new()?;
        let pair = portable_pty::native_pty_system()
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(pty_error)?;
        let command = build_host_command(&trusted_host, &bootstrap, &request.cwd);
        let (mut child, reader, mut writer) = acquire_handles_then_spawn(
            || pair.master.try_clone_reader().map_err(pty_error),
            || pair.master.take_writer().map_err(pty_error),
            || pair.slave.spawn_command(command).map_err(pty_error),
        )?;
        drop(pair.slave);
        self.observe_startup("host_spawned");

        let process_id = process_id_or_cleanup(child.as_mut())?;
        let protocol = assign_job_before_host_protocol(
            child.as_mut(),
            |child| {
                self.observe_startup("assign_begin");
                job.assign_terminal(child)?;
                self.observe_startup("assigned");
                Ok(())
            },
            |child| {
                // portable-pty creates ConPTY with INHERIT_CURSOR. The attached
                // host cannot reach main() until its cursor query is answered,
                // so complete that fixed console handshake only after the Job
                // boundary exists and before the authenticated host protocol.
                writer.write_all(b"\x1b[1;1R")?;
                writer.flush()?;
                let mut command_sent = false;
                start_host_protocol(
                    listener,
                    &request.command,
                    self,
                    || {
                        child
                            .try_wait()
                            .map(|status| status.map(|status| status.exit_code()))
                    },
                    &mut command_sent,
                )
            },
        );
        let parent_channel = match protocol {
            Ok(parent_channel) => parent_channel,
            Err(error) => {
                self.observe_startup("error_cleanup");
                drop(job.take());
                drop(reader);
                drop(writer);
                drop(pair.master);
                let cleanup = kill_and_poll_child(child.as_mut());
                return Err(io::Error::other(format!(
                    "Windows trusted-host startup failed: {error}; host cleanup: {cleanup}"
                )));
            }
        };

        Ok(SpawnedShell {
            child: Box::new(NativeShellChild { child, process_id }),
            reader,
            writer,
            guard: Box::new(NativeShellGuard {
                master: Some(pair.master),
                job: job.take(),
                parent_channel: Some(parent_channel),
                armed: true,
            }),
        })
    }

    #[cfg(target_os = "linux")]
    fn spawn_terminal_linux(
        &self,
        request: &ShellSpawnRequest,
        cols: u16,
        rows: u16,
    ) -> io::Result<SpawnedShell> {
        let trusted_host = self.resolve_host_executable()?;
        let (listener, bootstrap) =
            super::host::HostListener::bind(HOST_STARTUP_TIMEOUT).map_err(io::Error::from)?;
        self.observe_startup("listener_bound");
        let pair = portable_pty::native_pty_system()
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(pty_error)?;
        let command = build_host_command(&trusted_host, &bootstrap, &request.cwd);
        let (mut child, reader, writer) = acquire_handles_then_spawn(
            || pair.master.try_clone_reader().map_err(pty_error),
            || pair.master.take_writer().map_err(pty_error),
            || pair.slave.spawn_command(command).map_err(pty_error),
        )?;
        drop(pair.slave);
        self.observe_startup("host_spawned");

        let process_id = process_id_or_cleanup(child.as_mut())?;
        let mut command_sent = false;
        let parent_channel = match start_host_protocol(
            listener,
            &request.command,
            self,
            || {
                child
                    .try_wait()
                    .map(|status| status.map(|status| status.exit_code()))
            },
            &mut command_sent,
        ) {
            Ok(parent_channel) => parent_channel,
            Err(error) => {
                self.observe_startup("error_cleanup");
                drop(reader);
                drop(writer);
                drop(pair.master);
                let cleanup = if command_sent {
                    wait_for_host_cleanup(
                        || {
                            child.try_wait().map(|status| {
                                status.map(|status| i32::try_from(status.exit_code()).unwrap_or(1))
                            })
                        },
                        HOST_CLEANUP_TIMEOUT,
                    )
                } else {
                    kill_and_wait_child(child.as_mut())
                };
                return Err(io::Error::other(format!(
                    "Linux trusted-host startup failed: {error}; host cleanup: {cleanup}"
                )));
            }
        };

        Ok(SpawnedShell {
            child: Box::new(NativeShellChild {
                child,
                process_id,
                observed_exit: None,
                reaped: false,
            }),
            reader,
            writer,
            guard: Box::new(NativeShellGuard {
                master: Some(pair.master),
                parent_channel: Some(parent_channel),
                cleanup_confirmed: false,
                armed: true,
            }),
        })
    }

    #[cfg(any(windows, target_os = "linux"))]
    fn spawn_pipe(&self, request: &ShellSpawnRequest) -> io::Result<SpawnedShell> {
        let trusted_host = self.resolve_host_executable()?;
        let (listener, bootstrap) =
            super::host::HostListener::bind(HOST_STARTUP_TIMEOUT).map_err(io::Error::from)?;
        self.observe_startup("listener_bound");
        #[cfg(windows)]
        let mut job = WindowsJobBoundary::new()?;
        let filedescriptor::Pipe {
            read: input_read,
            write: input_write,
        } = filedescriptor::Pipe::new().map_err(descriptor_error)?;
        let filedescriptor::Pipe {
            read: output_read,
            write: output_write,
        } = filedescriptor::Pipe::new().map_err(descriptor_error)?;
        let mut command = build_host_process_command(&trusted_host, &bootstrap, &request.cwd);
        command
            .stdin(input_read.as_stdio().map_err(descriptor_error)?)
            .stdout(output_write.as_stdio().map_err(descriptor_error)?)
            .stderr(output_write.as_stdio().map_err(descriptor_error)?);
        let mut child = command.spawn()?;
        drop(command);
        drop(input_read);
        drop(output_write);
        self.observe_startup("host_spawned");

        let process_id = child.id();
        let mut command_sent = false;
        #[cfg(windows)]
        let protocol = assign_job_before_host_protocol(
            &mut child,
            |child| {
                self.observe_startup("assign_begin");
                job.assign_process(child)?;
                self.observe_startup("assigned");
                Ok(())
            },
            |child| {
                start_host_protocol(
                    listener,
                    &request.command,
                    self,
                    || try_wait_host_process(child),
                    &mut command_sent,
                )
            },
        );
        #[cfg(target_os = "linux")]
        let protocol = start_host_protocol(
            listener,
            &request.command,
            self,
            || try_wait_host_process(&mut child),
            &mut command_sent,
        );
        let parent_channel = match protocol {
            Ok(parent_channel) => parent_channel,
            Err(error) => {
                self.observe_startup("error_cleanup");
                #[cfg(windows)]
                let cleanup = {
                    drop(job.take());
                    kill_and_poll_process(&mut child)
                };
                #[cfg(target_os = "linux")]
                let cleanup = if command_sent {
                    wait_for_host_cleanup(|| try_wait_process(&mut child), HOST_CLEANUP_TIMEOUT)
                } else {
                    kill_and_wait_process(&mut child)
                };
                drop(output_read);
                drop(input_write);
                return Err(io::Error::other(format!(
                    "trusted-host pipe startup failed: {error}; host cleanup: {cleanup}"
                )));
            }
        };

        Ok(SpawnedShell {
            child: Box::new(NativePipeChild {
                child,
                process_id,
                #[cfg(target_os = "linux")]
                observed_exit: None,
                #[cfg(target_os = "linux")]
                reaped: false,
            }),
            reader: Box::new(output_read),
            writer: Box::new(input_write),
            guard: Box::new(NativeShellGuard {
                master: None,
                #[cfg(windows)]
                job: job.take(),
                parent_channel: Some(parent_channel),
                #[cfg(target_os = "linux")]
                cleanup_confirmed: false,
                armed: true,
            }),
        })
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    fn spawn_terminal(
        &self,
        request: &ShellSpawnRequest,
        cols: u16,
        rows: u16,
    ) -> io::Result<SpawnedShell> {
        let resolved = resolve_native_shell(&request.command)?;
        let pair = portable_pty::native_pty_system()
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(pty_error)?;
        let mut command = CommandBuilder::new(&resolved.program);
        command.args(&resolved.args);
        command.cwd(&request.cwd);
        let (mut child, reader, writer) = acquire_handles_then_spawn(
            || pair.master.try_clone_reader().map_err(pty_error),
            || pair.master.take_writer().map_err(pty_error),
            || pair.slave.spawn_command(command).map_err(pty_error),
        )?;
        drop(pair.slave);

        let process_id = process_id_or_cleanup(child.as_mut())?;
        Ok(SpawnedShell {
            child: Box::new(NativeShellChild { child, process_id }),
            reader,
            writer,
            guard: Box::new(NativeShellGuard {
                master: Some(pair.master),
                process_id,
                armed: true,
            }),
        })
    }
}

impl ShellBackend for NativeShellBackend {
    fn spawn(&self, request: &ShellSpawnRequest) -> io::Result<SpawnedShell> {
        match request.io_mode {
            ShellIoMode::Pipe => {
                #[cfg(any(windows, target_os = "linux"))]
                return self.spawn_pipe(request);
                #[cfg(not(any(windows, target_os = "linux")))]
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "native pipe shell is supported only on Windows and Linux",
                ));
            }
            ShellIoMode::Terminal { cols, rows } => {
                #[cfg(windows)]
                return self.spawn_terminal_windows(request, cols, rows);
                #[cfg(target_os = "linux")]
                return self.spawn_terminal_linux(request, cols, rows);
                #[cfg(not(any(windows, target_os = "linux")))]
                return self.spawn_terminal(request, cols, rows);
            }
        }
    }
}

#[cfg(windows)]
struct WindowsJobBoundary {
    job: Option<Job>,
}

#[cfg(windows)]
impl WindowsJobBoundary {
    fn new() -> io::Result<Self> {
        let mut limits = ExtendedLimitInfo::new();
        limits.limit_kill_on_job_close();
        let job = Job::create_with_limit_info(&limits)
            .map_err(|error| io::Error::other(format!("create Windows Job: {error}")))?;
        Ok(Self { job: Some(job) })
    }

    fn assign_terminal(&self, child: &(dyn portable_pty::Child + Send + Sync)) -> io::Result<()> {
        let process_handle = child
            .as_raw_handle()
            .ok_or_else(|| io::Error::other("PTY child did not expose a raw process handle"))?;
        self.assign_handle(process_handle)
    }

    fn assign_process(&self, child: &std::process::Child) -> io::Result<()> {
        self.assign_handle(child.as_raw_handle())
    }

    fn assign_handle(&self, process_handle: std::os::windows::io::RawHandle) -> io::Result<()> {
        self.job
            .as_ref()
            .ok_or_else(|| io::Error::other("Windows Job is missing before assignment"))?
            .assign_process(process_handle as isize)
            .map_err(|error| {
                io::Error::other(format!("assign trusted host to Windows Job: {error}"))
            })
    }

    fn take(&mut self) -> Option<Job> {
        self.job.take()
    }
}

#[cfg(windows)]
fn assign_job_before_host_protocol<T, Child: ?Sized>(
    child: &mut Child,
    assign: impl FnOnce(&Child) -> io::Result<()>,
    start_protocol: impl FnOnce(&mut Child) -> io::Result<T>,
) -> io::Result<T> {
    assign(child)?;
    start_protocol(child)
}

#[cfg(any(windows, target_os = "linux"))]
fn start_host_protocol(
    listener: super::host::HostListener,
    command: &str,
    backend: &NativeShellBackend,
    mut try_wait: impl FnMut() -> io::Result<Option<u32>>,
    command_sent: &mut bool,
) -> io::Result<super::host::ParentChannel> {
    let mut parent = listener
        .accept_with_probe(|| {
            try_wait().map(|status| {
                status.inspect(|_| {
                    backend.observe_startup("host_exited_before_auth");
                })
            })
        })
        .map_err(io::Error::from)?;
    backend.observe_startup("authenticated");
    parent.send_activate().map_err(io::Error::from)?;
    backend.observe_startup("activated");
    if parent.recv_event().map_err(io::Error::from)? != super::host::HostEvent::Contained {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "internal shell host did not confirm containment",
        ));
    }
    backend.observe_startup("contained");
    *command_sent = true;
    parent.send_command(command).map_err(io::Error::from)?;
    backend.observe_startup("command_sent");
    if parent.recv_event().map_err(io::Error::from)? != super::host::HostEvent::Ready {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "internal shell host did not become ready",
        ));
    }
    backend.observe_startup("ready");
    Ok(parent)
}

#[cfg(target_os = "linux")]
fn wait_for_host_cleanup(
    mut try_wait: impl FnMut() -> io::Result<Option<i32>>,
    timeout: Duration,
) -> String {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match try_wait() {
            Ok(Some(exit_code)) => {
                return format!(
                    "control channel closed and host cleanup exited with exit_code={}",
                    exit_code
                );
            }
            Err(error) => return format!("host cleanup poll failed: {error}"),
            Ok(None) if std::time::Instant::now() >= deadline => {
                return format!(
                    "host retained cleanup ownership after {}ms",
                    timeout.as_millis()
                );
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(5)),
        }
    }
}

struct NativeShellChild {
    child: Box<dyn portable_pty::Child + Send + Sync>,
    process_id: u32,
    #[cfg(target_os = "linux")]
    observed_exit: Option<i32>,
    #[cfg(target_os = "linux")]
    reaped: bool,
}

impl ShellChild for NativeShellChild {
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
        #[cfg(target_os = "linux")]
        {
            if self.try_wait()?.is_some() {
                return Ok(());
            }
            Err(io::Error::other(
                "Linux trusted host retains exclusive descendant-cleanup ownership",
            ))
        }

        #[cfg(not(target_os = "linux"))]
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
impl Drop for NativeShellChild {
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

struct NativePipeChild {
    child: std::process::Child,
    process_id: u32,
    #[cfg(target_os = "linux")]
    observed_exit: Option<i32>,
    #[cfg(target_os = "linux")]
    reaped: bool,
}

impl ShellChild for NativePipeChild {
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
        try_wait_process(&mut self.child)
    }

    fn kill(&mut self) -> io::Result<()> {
        #[cfg(target_os = "linux")]
        {
            if self.try_wait()?.is_some() {
                return Ok(());
            }
            Err(io::Error::other(
                "Linux trusted host retains exclusive descendant-cleanup ownership",
            ))
        }

        #[cfg(not(target_os = "linux"))]
        self.child.kill()
    }

    fn reap(&mut self) -> io::Result<()> {
        #[cfg(target_os = "linux")]
        {
            if !self.reaped {
                let status = self.child.wait()?;
                self.reaped = true;
                if self.observed_exit.is_none() {
                    self.observed_exit = Some(exit_status_code(status));
                }
            }
        }
        Ok(())
    }
}

#[cfg(target_os = "linux")]
impl Drop for NativePipeChild {
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

struct NativeShellGuard {
    master: Option<Box<dyn MasterPty + Send>>,
    #[cfg(windows)]
    job: Option<Job>,
    #[cfg(any(windows, target_os = "linux"))]
    parent_channel: Option<super::host::ParentChannel>,
    #[cfg(not(any(windows, target_os = "linux")))]
    process_id: u32,
    #[cfg(target_os = "linux")]
    cleanup_confirmed: bool,
    armed: bool,
}

impl super::backend::ShellGuard for NativeShellGuard {
    fn terminate<'a>(&'a mut self) -> ShellTerminateFuture<'a> {
        #[cfg(windows)]
        return Box::pin(async move {
            // This Job is unique and non-inherited. Windows' KILL_ON_JOB_CLOSE contract
            // terminates every associated process when this final Job handle closes.
            drop(self.job.take());
            drop(self.parent_channel.take());
            self.armed = false;
            Ok(())
        });

        #[cfg(target_os = "linux")]
        return Box::pin(async move {
            if self.cleanup_confirmed {
                return Ok(());
            }
            let Some(mut parent_channel) = self.parent_channel.take() else {
                return Err(io::Error::other(
                    "Linux trusted-host cleanup channel is unavailable",
                ));
            };
            let operation = tokio::task::spawn_blocking(move || {
                parent_channel.set_operation_timeout(HOST_CLEANUP_TIMEOUT);
                let result = request_linux_host_cleanup(&mut parent_channel);
                (parent_channel, result)
            })
            .await;
            match operation {
                Ok((_parent_channel, Ok(()))) => {
                    self.cleanup_confirmed = true;
                    Ok(())
                }
                Ok((parent_channel, Err(error))) => {
                    self.parent_channel = Some(parent_channel);
                    Err(error)
                }
                Err(error) => Err(io::Error::other(format!(
                    "Linux trusted-host cleanup task failed: {error}"
                ))),
            }
        });

        #[cfg(not(any(windows, target_os = "linux")))]
        Box::pin(async {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "native PTY containment is supported only on Windows and Linux",
            ))
        })
    }

    fn confirm<'a>(&'a mut self) -> ShellTerminateFuture<'a> {
        #[cfg(target_os = "linux")]
        return Box::pin(async move {
            if !self.cleanup_confirmed {
                return Err(io::Error::other(
                    "Linux trusted host has not confirmed an empty descendant fixed point",
                ));
            }
            Ok(())
        });

        #[cfg(windows)]
        return Box::pin(async move {
            // Closing the final KILL_ON_JOB_CLOSE handle is the Windows containment
            // confirmation boundary; there is no reusable process-group ID to probe.
            if self.job.is_some() {
                Err(io::Error::other(
                    "Windows Job confirmation preceded closing the Job handle",
                ))
            } else {
                Ok(())
            }
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

impl Drop for NativeShellGuard {
    fn drop(&mut self) {
        if self.armed {
            #[cfg(windows)]
            {
                drop(self.job.take());
                drop(self.parent_channel.take());
            }
            #[cfg(target_os = "linux")]
            if let Some(parent_channel) = self.parent_channel.as_mut() {
                parent_channel.set_operation_timeout(Duration::from_millis(100));
                let _ = parent_channel.send_stop();
            }
            #[cfg(not(any(windows, target_os = "linux")))]
            let _ = self.process_id;
        }
    }
}

#[cfg(target_os = "linux")]
fn request_linux_host_cleanup(parent_channel: &mut super::host::ParentChannel) -> io::Result<()> {
    let stop_error = parent_channel.send_stop().err();
    match parent_channel.recv_event() {
        Ok(super::host::HostEvent::Done(_)) => Ok(()),
        Ok(super::host::HostEvent::CleanupFailed) => Err(io::Error::other(
            "Linux trusted host reported cleanup failure and retained retry ownership",
        )),
        Ok(event) => Err(io::Error::other(format!(
            "Linux trusted host returned unexpected cleanup event: {event:?}"
        ))),
        Err(error) => {
            let stop = stop_error
                .map(|error| format!("; STOP failed first: {error}"))
                .unwrap_or_default();
            Err(io::Error::other(format!(
                "Linux trusted-host cleanup confirmation failed: {error}{stop}"
            )))
        }
    }
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

#[cfg(any(not(windows), test))]
#[derive(Debug, Eq, PartialEq)]
pub(super) struct ResolvedShell {
    pub(super) program: PathBuf,
    pub(super) args: Vec<String>,
}

#[cfg(test)]
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

#[cfg(target_os = "linux")]
pub(super) fn resolve_native_shell(command: &str) -> io::Result<ResolvedShell> {
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

fn descriptor_error(error: impl std::fmt::Display) -> io::Error {
    io::Error::other(error.to_string())
}

#[cfg(any(windows, target_os = "linux"))]
fn exit_status_code(status: std::process::ExitStatus) -> i32 {
    status.code().unwrap_or(1)
}

#[cfg(any(windows, target_os = "linux"))]
fn try_wait_process(child: &mut std::process::Child) -> io::Result<Option<i32>> {
    child.try_wait().map(|status| status.map(exit_status_code))
}

#[cfg(any(windows, target_os = "linux"))]
fn try_wait_host_process(child: &mut std::process::Child) -> io::Result<Option<u32>> {
    child.try_wait().map(|status| {
        status.map(|status| {
            status
                .code()
                .and_then(|code| u32::try_from(code).ok())
                .unwrap_or(1)
        })
    })
}

#[cfg(target_os = "linux")]
fn kill_and_wait_process(child: &mut std::process::Child) -> String {
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

#[cfg(windows)]
fn kill_and_poll_child(child: &mut (dyn portable_pty::Child + Send + Sync)) -> String {
    let kill = match child.kill() {
        Ok(()) => "direct kill completed".to_owned(),
        Err(error) => format!("direct kill failed: {error}"),
    };
    let deadline = std::time::Instant::now() + Duration::from_secs(1);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return format!("{kill}; poll observed exit_code={}", status.exit_code());
            }
            Err(error) => return format!("{kill}; poll failed: {error}"),
            Ok(None) if std::time::Instant::now() >= deadline => {
                return format!("{kill}; poll timed out after 1000ms");
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(5)),
        }
    }
}

#[cfg(windows)]
fn kill_and_poll_process(child: &mut std::process::Child) -> String {
    let kill = match child.kill() {
        Ok(()) => "direct kill completed".to_owned(),
        Err(error) => format!("direct kill failed: {error}"),
    };
    let deadline = std::time::Instant::now() + Duration::from_secs(1);
    loop {
        match try_wait_process(child) {
            Ok(Some(exit_code)) => return format!("{kill}; poll observed exit_code={exit_code}"),
            Err(error) => return format!("{kill}; poll failed: {error}"),
            Ok(None) if std::time::Instant::now() >= deadline => {
                return format!("{kill}; poll timed out after 1000ms");
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(5)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::ffi::{OsStr, OsString};
    use std::io::{self, Cursor};
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[cfg(windows)]
    use super::assign_job_before_host_protocol;
    use super::{
        NativeShellBackend, ProcessShellSessionIds, acquire_handles_then_spawn,
        build_host_command_with_environment, is_executable_with_access_check,
        process_id_or_cleanup, resolve_linux_shell, resolve_windows_shell,
    };
    use crate::shell::host::HostListener;
    use crate::shell::{ShellBackend, ShellManagerError, ShellSessionIdSource};

    #[test]
    fn native_launches_only_the_trusted_host_with_fixed_bootstrap_metadata() {
        let fixture = tempfile::tempdir().expect("host launch fixture");
        let trusted_host = fixture.path().join("trusted-minimax-host.exe");
        let cwd = fixture.path().join("working-directory");
        let user_command = "preassignment-secret-command-marker";
        let (listener, bootstrap) =
            HostListener::bind(std::time::Duration::from_secs(1)).expect("host bootstrap");

        let process_path = fixture.path().join("cargo-runtime-dlls");
        let command = build_host_command_with_environment(
            &trusted_host,
            &bootstrap,
            &cwd,
            [
                (
                    OsString::from("PATH"),
                    process_path.clone().into_os_string(),
                ),
                (
                    OsString::from("MINIMAX_SHELL_HOST_TOKEN"),
                    OsString::from("stale-process-token"),
                ),
                (
                    OsString::from("MINIMAX_TEST_PROCESS_ENV"),
                    OsString::from("preserved"),
                ),
            ],
        );

        assert_eq!(
            command.get_argv(),
            &vec![
                trusted_host.into_os_string(),
                "--minimax-internal-shell-host".into(),
            ]
        );
        assert_eq!(command.get_cwd(), Some(&cwd.into_os_string()));
        let bootstrap_environment = bootstrap.environment();
        assert_eq!(bootstrap_environment.len(), 4);
        let extra_environment = command.iter_extra_env_as_str().collect::<Vec<_>>();
        assert!(extra_environment.len() >= bootstrap_environment.len() + 2);
        assert_eq!(
            bootstrap_environment
                .iter()
                .map(|(key, _)| *key)
                .collect::<HashSet<_>>(),
            HashSet::from([
                "MINIMAX_SHELL_HOST_ADDRESS",
                "MINIMAX_SHELL_HOST_TOKEN",
                "MINIMAX_SHELL_HOST_VERSION",
                "MINIMAX_SHELL_HOST_TIMEOUT_MS",
            ])
        );
        for (key, value) in bootstrap_environment {
            assert_eq!(command.get_env(key), Some(OsStr::new(&value)));
        }
        assert_eq!(command.get_env("PATH"), Some(process_path.as_os_str()));
        assert_eq!(
            command.get_env("MINIMAX_TEST_PROCESS_ENV"),
            Some(OsStr::new("preserved"))
        );
        assert!(command.get_argv().iter().all(|value| value != user_command));
        assert!(
            command
                .iter_full_env_as_str()
                .all(|(key, value)| !key.contains(user_command) && !value.contains(user_command))
        );

        drop(listener);
    }

    #[test]
    fn native_backend_completes_its_own_startup_cursor_handshake() {
        assert!(!NativeShellBackend::default().requires_startup_cursor_handshake());
    }

    #[test]
    fn native_backend_rejects_a_relative_trusted_host_override() {
        let error = NativeShellBackend::with_host_executable(PathBuf::from("relative-host.exe"))
            .expect_err("trusted host override must be absolute");

        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
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
    fn windows_assignment_failure_never_starts_the_trusted_host_protocol() {
        let activations = AtomicUsize::new(0);
        let mut child = ();

        let result: io::Result<()> = assign_job_before_host_protocol(
            &mut child,
            |_| Err(io::Error::other("scripted assignment failure")),
            |_| {
                activations.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
        );
        let error = result.expect_err("assignment must fail");

        assert!(error.to_string().contains("scripted assignment failure"));
        assert_eq!(
            activations.load(Ordering::SeqCst),
            0,
            "host protocol activation and command delivery must remain unreachable"
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
