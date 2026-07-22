#[cfg(target_os = "linux")]
use std::collections::{HashMap, VecDeque};
use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
#[cfg(windows)]
use std::path::{Path, PathBuf};
#[cfg(any(windows, target_os = "linux"))]
use std::process::{Child, Command, ExitStatus, Stdio};
#[cfg(target_os = "linux")]
use std::sync::Arc;
#[cfg(target_os = "linux")]
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use minimax_protocol::MAX_SHELL_COMMAND_BYTES;

pub const INTERNAL_HOST_ARGUMENT: &str = "--minimax-internal-shell-host";
const HOST_ADDRESS_ENV: &str = "MINIMAX_SHELL_HOST_ADDRESS";
const HOST_TOKEN_ENV: &str = "MINIMAX_SHELL_HOST_TOKEN";
const HOST_VERSION_ENV: &str = "MINIMAX_SHELL_HOST_VERSION";
const HOST_TIMEOUT_ENV: &str = "MINIMAX_SHELL_HOST_TIMEOUT_MS";
#[cfg(windows)]
pub(super) const WINDOWS_COMMAND_PATH_ENV: &str = "MINIMAX_SHELL_COMMAND_PATH";
#[cfg(windows)]
const WINDOWS_ACK_ADDRESS_ENV: &str = "MINIMAX_SHELL_ACK_ADDRESS";
#[cfg(windows)]
const WINDOWS_ACK_TOKEN_ENV: &str = "MINIMAX_SHELL_ACK_TOKEN";
#[cfg(windows)]
const WINDOWS_COMMAND_BOOTSTRAP: &str = "$p=$env:MINIMAX_SHELL_COMMAND_PATH;$a=$env:MINIMAX_SHELL_ACK_ADDRESS;$t=$env:MINIMAX_SHELL_ACK_TOKEN;Remove-Item Env:MINIMAX_SHELL_COMMAND_PATH -ErrorAction SilentlyContinue;Remove-Item Env:MINIMAX_SHELL_ACK_ADDRESS -ErrorAction SilentlyContinue;Remove-Item Env:MINIMAX_SHELL_ACK_TOKEN -ErrorAction SilentlyContinue;try{$c=[IO.File]::ReadAllText($p,[Text.UTF8Encoding]::new($false,$true))}catch{throw 'shell command payload decode failed'}finally{Remove-Item -LiteralPath $p -Force -ErrorAction SilentlyContinue};if([IO.File]::Exists($p)){throw 'shell command payload cleanup failed'};$s=[Net.Sockets.TcpClient]::new();try{$h,$o=$a.Split(':');$s.Connect($h,[int]$o);$b=[Text.Encoding]::ASCII.GetBytes($t);$n=$s.GetStream();$n.Write($b,0,$b.Length)}finally{$s.Dispose()};Remove-Variable p,a,t,s,h,o,b,n -ErrorAction SilentlyContinue;Invoke-Expression $c";
const PROTOCOL_MAGIC: [u8; 8] = *b"MMXHOST1";
const PROTOCOL_VERSION: u16 = 1;
const HEADER_BYTES: usize = 15;
const AUTH_TOKEN_BYTES: usize = 32;
const AUTH_ATTEMPT_TIMEOUT: Duration = Duration::from_millis(250);
const ACCEPT_POLL_INTERVAL: Duration = Duration::from_millis(5);

#[derive(Debug)]
pub enum HostProtocolError {
    Io(io::Error),
    DeadlineExceeded,
    InvalidFrame,
    InvalidState,
    AuthenticationFailed,
    PeerClosed,
    InvalidBootstrap,
}

impl std::fmt::Display for HostProtocolError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Io(_) => "shell host I/O failed",
            Self::DeadlineExceeded => "shell host protocol deadline exceeded",
            Self::InvalidFrame => "invalid shell host protocol frame",
            Self::InvalidState => "invalid shell host protocol state",
            Self::AuthenticationFailed => "shell host authentication failed",
            Self::PeerClosed => "shell host control channel closed",
            Self::InvalidBootstrap => "invalid shell host bootstrap",
        })
    }
}

impl std::error::Error for HostProtocolError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<HostProtocolError> for io::Error {
    fn from(error: HostProtocolError) -> Self {
        match error {
            HostProtocolError::Io(error) => error,
            HostProtocolError::DeadlineExceeded => {
                Self::new(io::ErrorKind::TimedOut, HostProtocolError::DeadlineExceeded)
            }
            HostProtocolError::AuthenticationFailed => Self::new(
                io::ErrorKind::PermissionDenied,
                HostProtocolError::AuthenticationFailed,
            ),
            HostProtocolError::PeerClosed => {
                Self::new(io::ErrorKind::BrokenPipe, HostProtocolError::PeerClosed)
            }
            HostProtocolError::InvalidFrame => {
                Self::new(io::ErrorKind::InvalidData, HostProtocolError::InvalidFrame)
            }
            HostProtocolError::InvalidState => {
                Self::new(io::ErrorKind::InvalidData, HostProtocolError::InvalidState)
            }
            HostProtocolError::InvalidBootstrap => Self::new(
                io::ErrorKind::InvalidInput,
                HostProtocolError::InvalidBootstrap,
            ),
        }
    }
}

impl From<io::Error> for HostProtocolError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Clone)]
pub struct HostBootstrap {
    address: SocketAddr,
    token: [u8; AUTH_TOKEN_BYTES],
    deadline: Instant,
    timeout: Duration,
}

impl HostBootstrap {
    pub fn from_current_environment() -> Result<Self, HostProtocolError> {
        Self::from_environment(|key| std::env::var_os(key))
    }

    fn from_environment(get: impl Fn(&str) -> Option<OsString>) -> Result<Self, HostProtocolError> {
        let address = required_utf8(&get, HOST_ADDRESS_ENV)?
            .parse::<SocketAddr>()
            .map_err(|_| HostProtocolError::InvalidBootstrap)?;
        if !matches!(address, SocketAddr::V4(address) if address.ip().is_loopback()) {
            return Err(HostProtocolError::InvalidBootstrap);
        }
        let token = decode_token(&required_utf8(&get, HOST_TOKEN_ENV)?)?;
        if required_utf8(&get, HOST_VERSION_ENV)? != PROTOCOL_VERSION.to_string() {
            return Err(HostProtocolError::InvalidBootstrap);
        }
        let timeout_ms = required_utf8(&get, HOST_TIMEOUT_ENV)?
            .parse::<u64>()
            .map_err(|_| HostProtocolError::InvalidBootstrap)?;
        if !(1..=300_000).contains(&timeout_ms) {
            return Err(HostProtocolError::InvalidBootstrap);
        }
        let timeout = Duration::from_millis(timeout_ms);
        Ok(Self {
            address,
            token,
            deadline: Instant::now() + timeout,
            timeout,
        })
    }

    pub fn arguments(&self) -> [&'static str; 1] {
        [INTERNAL_HOST_ARGUMENT]
    }

    pub fn environment(&self) -> Vec<(&'static str, String)> {
        vec![
            (HOST_ADDRESS_ENV, self.address.to_string()),
            (HOST_TOKEN_ENV, encode_hex(&self.token)),
            (HOST_VERSION_ENV, PROTOCOL_VERSION.to_string()),
            (HOST_TIMEOUT_ENV, self.timeout.as_millis().to_string()),
        ]
    }

    pub fn address(&self) -> SocketAddr {
        self.address
    }

    pub fn connect(self) -> Result<HostChannel, HostProtocolError> {
        let mut stream = TcpStream::connect_timeout(&self.address, remaining(self.deadline)?)?;
        write_frame(
            &mut stream,
            FrameKind::Authenticate,
            &self.token,
            Some(self.deadline),
        )?;
        Ok(HostChannel {
            stream,
            state: HostState::WaitingActivation,
            deadline: Some(self.deadline),
            #[cfg(test)]
            fail_next_send: None,
        })
    }
}

pub fn run_internal_shell_host() -> i32 {
    let result = HostBootstrap::from_current_environment().and_then(|bootstrap| {
        let startup_deadline = bootstrap.deadline;
        let channel = bootstrap.connect()?;
        run_host_lifecycle(
            channel,
            platform_host_supervisor(startup_deadline),
            Duration::from_millis(10),
        )
    });
    match result {
        Ok(RootExit::Code(code)) => code,
        Ok(RootExit::Signal(signal)) => 128 + i32::from(signal),
        Err(_) => 125,
    }
}

#[cfg(windows)]
type PlatformHostSupervisor = WindowsProcessSupervisor;

#[cfg(windows)]
fn platform_host_supervisor(startup_deadline: Instant) -> PlatformHostSupervisor {
    WindowsProcessSupervisor::new(startup_deadline)
}

#[cfg(target_os = "linux")]
type PlatformHostSupervisor = LinuxProcessSupervisor;

#[cfg(target_os = "linux")]
fn platform_host_supervisor(_startup_deadline: Instant) -> PlatformHostSupervisor {
    LinuxProcessSupervisor::new()
}

#[cfg(not(any(windows, target_os = "linux")))]
struct PlatformHostSupervisor;

#[cfg(not(any(windows, target_os = "linux")))]
fn platform_host_supervisor(_startup_deadline: Instant) -> PlatformHostSupervisor {
    PlatformHostSupervisor::new()
}

#[cfg(not(any(windows, target_os = "linux")))]
impl PlatformHostSupervisor {
    fn new() -> Self {
        Self
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
impl HostSupervisor for PlatformHostSupervisor {
    fn preflight(&mut self) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "internal shell host containment is not implemented on this platform",
        ))
    }

    fn spawn(&mut self, _command: &str) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "internal shell host containment is not implemented on this platform",
        ))
    }

    fn try_root_exit(&mut self) -> io::Result<Option<RootExit>> {
        Ok(None)
    }

    fn cleanup(&mut self) -> io::Result<Option<RootExit>> {
        Ok(Some(RootExit::Code(125)))
    }
}

#[cfg(target_os = "linux")]
struct LinuxProcessSupervisor {
    child: Option<Child>,
    observed_exit: Option<RootExit>,
    host: LinuxProcessIdentity,
    cleanup_requested: Arc<AtomicBool>,
    signal_registrations: Vec<signal_hook::SigId>,
    term_deadline: Option<Instant>,
    kill_phase: bool,
    empty_scans: u8,
}

#[cfg(target_os = "linux")]
impl LinuxProcessSupervisor {
    fn new() -> Self {
        Self {
            child: None,
            observed_exit: None,
            host: LinuxProcessIdentity {
                pid: std::process::id(),
                parent_pid: 0,
                start_time: 0,
                state: '?',
            },
            cleanup_requested: Arc::new(AtomicBool::new(false)),
            signal_registrations: Vec::new(),
            term_deadline: None,
            kill_phase: false,
            empty_scans: 0,
        }
    }

    fn observe_root_exit(&mut self) -> io::Result<Option<RootExit>> {
        if let Some(exit) = self.observed_exit {
            return Ok(Some(exit));
        }
        let Some(child) = self.child.as_mut() else {
            return Ok(None);
        };
        let Some(status) = child.try_wait()? else {
            return Ok(None);
        };
        let exit = linux_root_exit_from_status(status);
        self.observed_exit = Some(exit);
        Ok(Some(exit))
    }

    fn scan_descendants(&self) -> io::Result<Vec<LinuxProcessIdentity>> {
        let processes = linux_process_table()?;
        let host = processes
            .iter()
            .find(|process| process.pid == self.host.pid)
            .ok_or_else(|| io::Error::other("Linux shell host disappeared from /proc"))?;
        if host.start_time != self.host.start_time {
            return Err(io::Error::other(
                "Linux shell host identity changed while scanning /proc",
            ));
        }

        Ok(linux_descendants_from_table(self.host.pid, processes))
    }

    fn signal_descendants(
        &self,
        descendants: &[LinuxProcessIdentity],
        signal: rustix::process::Signal,
    ) -> io::Result<()> {
        for identity in descendants {
            if identity.state == 'Z' {
                continue;
            }
            let Some(pid) = linux_pid(identity.pid)? else {
                continue;
            };
            let pidfd = match rustix::process::pidfd_open(pid, rustix::process::PidfdFlags::empty())
            {
                Ok(pidfd) => pidfd,
                Err(rustix::io::Errno::SRCH) => continue,
                Err(error) => return Err(io::Error::from(error)),
            };
            let Some(current) = read_linux_process(identity.pid)? else {
                continue;
            };
            if current.start_time != identity.start_time {
                continue;
            }
            let still_contained = self.scan_descendants()?.into_iter().any(|candidate| {
                candidate.pid == identity.pid && candidate.start_time == identity.start_time
            });
            if !still_contained {
                continue;
            }
            match rustix::process::pidfd_send_signal(&pidfd, signal) {
                Ok(()) | Err(rustix::io::Errno::SRCH) => {}
                Err(error) => return Err(io::Error::from(error)),
            }
        }
        Ok(())
    }

    fn reap_exited_children(&mut self) -> io::Result<()> {
        let root_pid = self.child.as_ref().map(Child::id);
        for process in self.scan_descendants()? {
            if process.parent_pid != self.host.pid
                || process.state != 'Z'
                || Some(process.pid) == root_pid
            {
                continue;
            }
            let Some(pid) = linux_pid(process.pid)? else {
                continue;
            };
            match rustix::process::waitpid(Some(pid), rustix::process::WaitOptions::NOHANG) {
                Ok(_) | Err(rustix::io::Errno::CHILD) => {}
                Err(error) => return Err(io::Error::from(error)),
            }
        }

        let root_exited = self.observe_root_exit()?.is_some();
        if !root_exited {
            return Ok(());
        }
        loop {
            match rustix::process::wait(rustix::process::WaitOptions::NOHANG) {
                Ok(Some(_)) => {}
                Ok(None) | Err(rustix::io::Errno::CHILD) => return Ok(()),
                Err(error) => return Err(io::Error::from(error)),
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn linux_descendants_from_table(
    host_pid: u32,
    processes: Vec<LinuxProcessIdentity>,
) -> Vec<LinuxProcessIdentity> {
    let mut children = HashMap::<u32, Vec<LinuxProcessIdentity>>::new();
    for process in processes {
        children
            .entry(process.parent_pid)
            .or_default()
            .push(process);
    }
    let mut descendants = Vec::new();
    let mut parents = VecDeque::from([host_pid]);
    while let Some(parent) = parents.pop_front() {
        if let Some(mut direct_children) = children.remove(&parent) {
            direct_children.sort_by_key(|process| process.pid);
            for child in direct_children {
                parents.push_back(child.pid);
                descendants.push(child);
            }
        }
    }
    descendants
}

#[cfg(target_os = "linux")]
impl HostSupervisor for LinuxProcessSupervisor {
    fn preflight(&mut self) -> io::Result<()> {
        let host = linux_process_table()?
            .into_iter()
            .find(|process| process.pid == std::process::id())
            .ok_or_else(|| io::Error::other("Linux shell host is unavailable in /proc"))?;
        let host_pid = linux_pid(host.pid)?
            .ok_or_else(|| io::Error::other("Linux shell host PID is invalid"))?;
        let _pidfd = rustix::process::pidfd_open(host_pid, rustix::process::PidfdFlags::empty())
            .map_err(io::Error::from)?;
        rustix::process::set_child_subreaper(rustix::process::Pid::from_raw(1))
            .map_err(io::Error::from)?;
        if rustix::process::child_subreaper().map_err(io::Error::from)?
            != rustix::process::Pid::from_raw(1)
        {
            return Err(io::Error::other(
                "Linux shell host could not enable child-subreaper containment",
            ));
        }

        for signal in [
            signal_hook::consts::SIGINT,
            signal_hook::consts::SIGTERM,
            signal_hook::consts::SIGHUP,
            signal_hook::consts::SIGQUIT,
        ] {
            let registration =
                signal_hook::flag::register(signal, Arc::clone(&self.cleanup_requested))?;
            self.signal_registrations.push(registration);
        }
        self.host = host;
        Ok(())
    }

    fn spawn(&mut self, command: &str) -> io::Result<()> {
        if self.child.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "shell root process already exists",
            ));
        }
        let shell = super::native::resolve_native_shell(command)?;
        let mut process = Command::new(shell.program);
        process.args(shell.args);
        for key in [
            HOST_ADDRESS_ENV,
            HOST_TOKEN_ENV,
            HOST_VERSION_ENV,
            HOST_TIMEOUT_ENV,
        ] {
            process.env_remove(key);
        }
        process
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
        self.child = Some(process.spawn()?);
        Ok(())
    }

    fn try_root_exit(&mut self) -> io::Result<Option<RootExit>> {
        self.observe_root_exit()
    }

    fn cleanup_requested(&self) -> bool {
        self.cleanup_requested.load(Ordering::Relaxed)
    }

    fn cleanup(&mut self) -> io::Result<Option<RootExit>> {
        self.reap_exited_children()?;
        let descendants = self.scan_descendants()?;
        if descendants.is_empty() {
            self.empty_scans = self.empty_scans.saturating_add(1);
            if self.empty_scans >= 2 {
                return Ok(Some(self.observed_exit.unwrap_or(RootExit::Code(125))));
            }
            return Ok(None);
        }
        self.empty_scans = 0;

        let term_deadline = *self
            .term_deadline
            .get_or_insert_with(|| Instant::now() + Duration::from_millis(50));
        if !self.kill_phase && Instant::now() < term_deadline {
            self.signal_descendants(&descendants, rustix::process::Signal::TERM)?;
            return Ok(None);
        }
        self.kill_phase = true;
        self.signal_descendants(&descendants, rustix::process::Signal::KILL)?;
        Ok(None)
    }
}

#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LinuxProcessIdentity {
    pid: u32,
    parent_pid: u32,
    start_time: u64,
    state: char,
}

#[cfg(target_os = "linux")]
fn linux_pid(raw_pid: u32) -> io::Result<Option<rustix::process::Pid>> {
    let raw_pid =
        i32::try_from(raw_pid).map_err(|_| io::Error::other("Linux process ID exceeds i32"))?;
    Ok(rustix::process::Pid::from_raw(raw_pid))
}

#[cfg(target_os = "linux")]
fn linux_process_table() -> io::Result<Vec<LinuxProcessIdentity>> {
    let mut processes = Vec::new();
    for entry in std::fs::read_dir("/proc")? {
        let entry = entry?;
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        match read_linux_process(pid) {
            Ok(Some(process)) => processes.push(process),
            Ok(None) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(processes)
}

#[cfg(target_os = "linux")]
fn read_linux_process(pid: u32) -> io::Result<Option<LinuxProcessIdentity>> {
    let stat = match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => stat,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    parse_linux_process_stat(&stat).map(Some)
}

#[cfg(target_os = "linux")]
fn parse_linux_process_stat(stat: &str) -> io::Result<LinuxProcessIdentity> {
    let open = stat
        .find('(')
        .ok_or_else(|| io::Error::other("Linux /proc stat omitted process name"))?;
    let close = stat
        .rfind(')')
        .filter(|close| *close > open)
        .ok_or_else(|| io::Error::other("Linux /proc stat has malformed process name"))?;
    let pid = stat[..open]
        .trim()
        .parse::<u32>()
        .map_err(|_| io::Error::other("Linux /proc stat has invalid PID"))?;
    let fields = stat[close + 1..].split_whitespace().collect::<Vec<_>>();
    if fields.len() <= 19 {
        return Err(io::Error::other("Linux /proc stat is truncated"));
    }
    let state = fields[0]
        .chars()
        .next()
        .ok_or_else(|| io::Error::other("Linux /proc stat has invalid state"))?;
    let parent_pid = fields[1]
        .parse::<u32>()
        .map_err(|_| io::Error::other("Linux /proc stat has invalid parent PID"))?;
    let start_time = fields[19]
        .parse::<u64>()
        .map_err(|_| io::Error::other("Linux /proc stat has invalid start time"))?;
    Ok(LinuxProcessIdentity {
        pid,
        parent_pid,
        start_time,
        state,
    })
}

#[cfg(target_os = "linux")]
fn linux_root_exit_from_status(status: ExitStatus) -> RootExit {
    use std::os::unix::process::ExitStatusExt as _;

    if let Some(code) = status.code() {
        RootExit::Code(code)
    } else if let Some(signal) = status.signal().and_then(|signal| u8::try_from(signal).ok()) {
        RootExit::Signal(signal)
    } else {
        RootExit::Code(125)
    }
}

#[cfg(windows)]
struct WindowsProcessSupervisor {
    child: Option<Child>,
    observed_exit: Option<RootExit>,
    startup_deadline: Instant,
}

#[cfg(windows)]
impl WindowsProcessSupervisor {
    fn new(startup_deadline: Instant) -> Self {
        Self {
            child: None,
            observed_exit: None,
            startup_deadline,
        }
    }
}

#[cfg(windows)]
impl HostSupervisor for WindowsProcessSupervisor {
    fn preflight(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn spawn(&mut self, command: &str) -> io::Result<()> {
        if self.child.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "shell root process already exists",
            ));
        }
        let command_payload_path = std::env::var_os(WINDOWS_COMMAND_PATH_ENV).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "Windows shell command payload path is missing",
            )
        })?;
        let staged_command = std::fs::read_to_string(&command_payload_path)?;
        if staged_command != command {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Windows shell command payload does not match the authenticated command",
            ));
        }
        let acknowledgement = WindowsBootstrapAcknowledgement::bind(self.startup_deadline)?;
        let shell = resolve_windows_process_shell()?;
        let mut process = Command::new(shell.program);
        process.args(shell.args);
        for key in [
            HOST_ADDRESS_ENV,
            HOST_TOKEN_ENV,
            HOST_VERSION_ENV,
            HOST_TIMEOUT_ENV,
        ] {
            process.env_remove(key);
        }
        process.env(WINDOWS_COMMAND_PATH_ENV, command_payload_path);
        process.envs(acknowledgement.environment());
        process
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
        let child = process.spawn()?;
        self.child = Some(child);
        acknowledgement.wait(
            self.child
                .as_mut()
                .expect("spawned PowerShell child is stored before acknowledgement"),
        )?;
        Ok(())
    }

    fn try_root_exit(&mut self) -> io::Result<Option<RootExit>> {
        if let Some(exit) = self.observed_exit {
            return Ok(Some(exit));
        }
        let Some(child) = self.child.as_mut() else {
            return Ok(None);
        };
        let Some(status) = child.try_wait()? else {
            return Ok(None);
        };
        let exit = root_exit_from_status(status);
        self.observed_exit = Some(exit);
        Ok(Some(exit))
    }

    fn cleanup(&mut self) -> io::Result<Option<RootExit>> {
        if let Some(exit) = self.observed_exit {
            return Ok(Some(exit));
        }
        let Some(child) = self.child.as_mut() else {
            return Ok(Some(RootExit::Code(125)));
        };
        let status = match child.try_wait()? {
            Some(status) => status,
            None => {
                child.kill()?;
                child.wait()?
            }
        };
        let exit = root_exit_from_status(status);
        self.observed_exit = Some(exit);
        Ok(Some(exit))
    }
}

#[cfg(windows)]
pub(super) struct WindowsCommandPayload {
    path: tempfile::TempPath,
}

#[cfg(windows)]
struct WindowsBootstrapAcknowledgement {
    listener: TcpListener,
    address: SocketAddr,
    token: String,
    deadline: Instant,
}

#[cfg(windows)]
impl WindowsBootstrapAcknowledgement {
    fn bind(deadline: Instant) -> io::Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let mut token = [0_u8; AUTH_TOKEN_BYTES];
        getrandom::fill(&mut token).map_err(|error| {
            io::Error::other(format!(
                "shell bootstrap acknowledgement token generation failed: {error}"
            ))
        })?;
        Ok(Self {
            listener,
            address,
            token: encode_hex(&token),
            deadline,
        })
    }

    fn environment(&self) -> Vec<(&'static str, String)> {
        vec![
            (WINDOWS_ACK_ADDRESS_ENV, self.address.to_string()),
            (WINDOWS_ACK_TOKEN_ENV, self.token.clone()),
        ]
    }

    fn wait(&self, child: &mut Child) -> io::Result<()> {
        loop {
            if Instant::now() >= self.deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "PowerShell command bootstrap acknowledgement timed out",
                ));
            }
            match self.listener.accept() {
                Ok((mut stream, peer)) if peer.ip().is_loopback() => {
                    let remaining = self.deadline.saturating_duration_since(Instant::now());
                    stream.set_nonblocking(false)?;
                    stream.set_read_timeout(Some(AUTH_ATTEMPT_TIMEOUT.min(remaining)))?;
                    let mut received = vec![0_u8; self.token.len()];
                    if stream.read_exact(&mut received).is_ok()
                        && received.as_slice() == self.token.as_bytes()
                    {
                        return Ok(());
                    }
                }
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    if child.try_wait()?.is_some() {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "PowerShell exited before command bootstrap acknowledgement",
                        ));
                    }
                    std::thread::sleep(ACCEPT_POLL_INTERVAL);
                }
                Err(error) => return Err(error),
            }
        }
    }
}

#[cfg(windows)]
impl WindowsCommandPayload {
    pub(super) fn stage(command: &str) -> io::Result<Self> {
        let mut file = tempfile::Builder::new()
            .prefix("minimax-shell-")
            .suffix(".ps1")
            .tempfile()?;
        file.write_all(command.as_bytes())?;
        file.flush()?;
        Ok(Self {
            path: file.into_temp_path(),
        })
    }

    pub(super) fn path(&self) -> &Path {
        self.path.as_ref()
    }
}

#[cfg(windows)]
fn resolve_windows_process_shell() -> io::Result<ResolvedProcessShell> {
    let pwsh = std::env::var_os("PATH")
        .into_iter()
        .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .map(|directory| directory.join("pwsh.exe"))
        .find(|candidate| candidate.is_file());
    let powershell = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .map(|root| {
            root.join("System32")
                .join("WindowsPowerShell")
                .join("v1.0")
                .join("powershell.exe")
        })
        .filter(|candidate| candidate.is_file());
    let program = pwsh.or(powershell).ok_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, "PowerShell executable not found")
    })?;
    Ok(ResolvedProcessShell {
        program,
        args: vec![
            "-NoLogo".to_owned(),
            "-NoProfile".to_owned(),
            "-Command".to_owned(),
            WINDOWS_COMMAND_BOOTSTRAP.to_owned(),
        ],
    })
}

#[cfg(windows)]
struct ResolvedProcessShell {
    program: PathBuf,
    args: Vec<String>,
}

#[cfg(windows)]
fn root_exit_from_status(status: ExitStatus) -> RootExit {
    RootExit::Code(status.code().unwrap_or(125))
}

pub trait HostSupervisor {
    fn preflight(&mut self) -> io::Result<()>;
    fn spawn(&mut self, command: &str) -> io::Result<()>;
    fn try_root_exit(&mut self) -> io::Result<Option<RootExit>>;
    fn cleanup_requested(&self) -> bool {
        false
    }
    /// Returns the root outcome only after the containment fixed point is empty.
    fn cleanup(&mut self) -> io::Result<Option<RootExit>>;
}

pub fn run_host_lifecycle(
    mut channel: HostChannel,
    mut supervisor: impl HostSupervisor,
    retry_interval: Duration,
) -> Result<RootExit, HostProtocolError> {
    channel.recv_activate()?;
    supervisor.preflight()?;
    channel.send_contained()?;
    let command = channel.recv_command()?;
    let mut root_exit = None;
    let mut terminal_error = supervisor
        .spawn(&command)
        .err()
        .map(HostProtocolError::from);
    let mut cleaning = terminal_error.is_some();
    let mut failure_reported = false;
    let mut parent_connected = terminal_error.is_none();
    let control_rx = if terminal_error.is_some() {
        None
    } else {
        match channel.stream.try_clone() {
            Ok(mut control_stream) => match channel.send_ready() {
                Ok(()) => {
                    let (control_tx, control_rx) = std::sync::mpsc::sync_channel(4);
                    match std::thread::Builder::new()
                        .name("minimax-shell-host-control".to_owned())
                        .spawn(move || {
                            loop {
                                let control = read_host_control(&mut control_stream);
                                let terminal = !matches!(control, Ok(HostControl::Stop));
                                if control_tx.send(control).is_err() || terminal {
                                    break;
                                }
                            }
                        }) {
                        Ok(_) => Some(control_rx),
                        Err(error) => {
                            terminal_error = Some(error.into());
                            parent_connected = false;
                            cleaning = true;
                            None
                        }
                    }
                }
                Err(error) => {
                    terminal_error = Some(error);
                    parent_connected = false;
                    cleaning = true;
                    None
                }
            },
            Err(error) => {
                terminal_error = Some(error.into());
                parent_connected = false;
                cleaning = true;
                None
            }
        }
    };

    loop {
        if !cleaning && supervisor.cleanup_requested() {
            cleaning = true;
        }
        if !cleaning {
            match supervisor.try_root_exit() {
                Ok(Some(exit)) => {
                    root_exit = Some(exit);
                    cleaning = true;
                }
                Ok(None) => {}
                Err(error) => {
                    terminal_error = Some(error.into());
                    cleaning = true;
                }
            }
        }

        if let Some(control_rx) = &control_rx {
            while let Ok(control) = control_rx.try_recv() {
                match control {
                    Ok(HostControl::Stop) => cleaning = true,
                    Ok(HostControl::ParentEof) | Err(_) => {
                        cleaning = true;
                        parent_connected = false;
                    }
                }
            }
        }

        if cleaning {
            match supervisor.cleanup() {
                Ok(Some(cleanup_exit)) => {
                    let exit = root_exit.unwrap_or(cleanup_exit);
                    if parent_connected && terminal_error.is_none() {
                        let _ = channel.send_done(exit);
                    }
                    let _ = channel.stream.shutdown(std::net::Shutdown::Both);
                    return match terminal_error {
                        Some(error) => Err(error),
                        None => Ok(exit),
                    };
                }
                Ok(None) => {}
                Err(_) => {
                    if parent_connected && !failure_reported {
                        if channel.send_cleanup_failed().is_err() {
                            parent_connected = false;
                        } else {
                            failure_reported = true;
                        }
                    }
                }
            }
        }

        if let Some(control_rx) = &control_rx {
            match control_rx.recv_timeout(retry_interval) {
                Ok(Ok(HostControl::Stop)) => cleaning = true,
                Ok(Ok(HostControl::ParentEof) | Err(_))
                | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    cleaning = true;
                    parent_connected = false;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            }
        } else {
            std::thread::sleep(retry_interval);
        }
    }
}

fn read_host_control(stream: &mut TcpStream) -> Result<HostControl, HostProtocolError> {
    let frame = match read_frame(stream, None) {
        Ok(frame) => frame,
        Err(HostProtocolError::PeerClosed) => return Ok(HostControl::ParentEof),
        Err(error) => return Err(error),
    };
    if frame.kind == FrameKind::Stop && frame.payload.is_empty() {
        Ok(HostControl::Stop)
    } else {
        Err(HostProtocolError::InvalidFrame)
    }
}

pub struct HostListener {
    listener: TcpListener,
    token: [u8; AUTH_TOKEN_BYTES],
    deadline: Instant,
}

impl HostListener {
    pub fn bind(timeout: Duration) -> Result<(Self, HostBootstrap), HostProtocolError> {
        let mut token = [0_u8; AUTH_TOKEN_BYTES];
        getrandom::fill(&mut token).map_err(|error| {
            HostProtocolError::Io(io::Error::other(format!(
                "shell host token generation failed: {error}"
            )))
        })?;
        Self::bind_with_token(timeout, token)
    }

    #[cfg(test)]
    fn bind_for_test(
        timeout: Duration,
        token: [u8; AUTH_TOKEN_BYTES],
    ) -> Result<(Self, HostBootstrap), HostProtocolError> {
        Self::bind_with_token(timeout, token)
    }

    fn bind_with_token(
        timeout: Duration,
        token: [u8; AUTH_TOKEN_BYTES],
    ) -> Result<(Self, HostBootstrap), HostProtocolError> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let deadline = Instant::now() + timeout;
        Ok((
            Self {
                listener,
                token,
                deadline,
            },
            HostBootstrap {
                address,
                token,
                deadline,
                timeout,
            },
        ))
    }

    pub fn accept(self) -> Result<ParentChannel, HostProtocolError> {
        self.accept_with_probe(|| Ok(None))
    }

    pub(crate) fn accept_with_probe(
        self,
        mut probe: impl FnMut() -> io::Result<Option<u32>>,
    ) -> Result<ParentChannel, HostProtocolError> {
        loop {
            let total_remaining = remaining(self.deadline)?;
            match self.listener.accept() {
                Ok((mut stream, peer)) => {
                    stream.set_nonblocking(false)?;
                    if !peer.ip().is_loopback() {
                        continue;
                    }
                    let candidate_deadline =
                        self.deadline.min(Instant::now() + AUTH_ATTEMPT_TIMEOUT);
                    let authenticated = matches!(
                        read_frame(&mut stream, Some(candidate_deadline)),
                        Ok(Frame {
                            kind: FrameKind::Authenticate,
                            payload,
                        }) if payload.as_slice() == self.token
                    );
                    if authenticated {
                        return Ok(ParentChannel {
                            stream,
                            state: ParentState::Connected,
                            deadline: Some(self.deadline),
                        });
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    if let Some(exit_code) = probe()? {
                        return Err(HostProtocolError::Io(io::Error::other(format!(
                            "shell host exited before authentication with exit_code={exit_code}"
                        ))));
                    }
                    std::thread::sleep(total_remaining.min(ACCEPT_POLL_INTERVAL));
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => return Err(error.into()),
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RootExit {
    Code(i32),
    Signal(u8),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostControl {
    Stop,
    ParentEof,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostEvent {
    Contained,
    Ready,
    CleanupFailed,
    Done(RootExit),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ParentState {
    Connected,
    WaitingContained,
    Contained,
    WaitingReady,
    Running,
    Cleaning,
    Done,
}

pub struct ParentChannel {
    stream: TcpStream,
    state: ParentState,
    deadline: Option<Instant>,
}

impl ParentChannel {
    #[cfg(target_os = "linux")]
    pub(crate) fn set_operation_timeout(&mut self, timeout: Duration) {
        self.deadline = Some(Instant::now() + timeout);
    }

    pub fn send_activate(&mut self) -> Result<(), HostProtocolError> {
        self.require_state(ParentState::Connected)?;
        write_frame(&mut self.stream, FrameKind::Activate, &[], self.deadline)?;
        self.state = ParentState::WaitingContained;
        Ok(())
    }

    pub fn send_command(&mut self, command: &str) -> Result<(), HostProtocolError> {
        self.require_state(ParentState::Contained)?;
        write_frame(
            &mut self.stream,
            FrameKind::Command,
            command.as_bytes(),
            self.deadline,
        )?;
        self.state = ParentState::WaitingReady;
        Ok(())
    }

    pub fn send_stop(&mut self) -> Result<(), HostProtocolError> {
        if !matches!(self.state, ParentState::Running | ParentState::Cleaning) {
            return Err(HostProtocolError::InvalidState);
        }
        write_frame(&mut self.stream, FrameKind::Stop, &[], self.deadline)?;
        self.state = ParentState::Cleaning;
        Ok(())
    }

    pub fn recv_event(&mut self) -> Result<HostEvent, HostProtocolError> {
        let frame = read_frame(&mut self.stream, self.deadline)?;
        let event = match (self.state, frame.kind) {
            (ParentState::WaitingContained, FrameKind::Contained) if frame.payload.is_empty() => {
                self.state = ParentState::Contained;
                HostEvent::Contained
            }
            (ParentState::WaitingReady, FrameKind::Ready) if frame.payload.is_empty() => {
                self.state = ParentState::Running;
                self.deadline = None;
                HostEvent::Ready
            }
            (ParentState::Running | ParentState::Cleaning, FrameKind::CleanupFailed)
                if frame.payload.is_empty() =>
            {
                self.state = ParentState::Cleaning;
                HostEvent::CleanupFailed
            }
            (ParentState::Running | ParentState::Cleaning, FrameKind::Done) => {
                let exit = decode_root_exit(&frame.payload)?;
                self.state = ParentState::Done;
                HostEvent::Done(exit)
            }
            _ => return Err(HostProtocolError::InvalidState),
        };
        Ok(event)
    }

    fn require_state(&self, expected: ParentState) -> Result<(), HostProtocolError> {
        if self.state == expected {
            Ok(())
        } else {
            Err(HostProtocolError::InvalidState)
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HostState {
    WaitingActivation,
    PreparingContainment,
    WaitingCommand,
    WaitingReady,
    Running,
    Cleaning,
    Done,
}

pub struct HostChannel {
    stream: TcpStream,
    state: HostState,
    deadline: Option<Instant>,
    #[cfg(test)]
    fail_next_send: Option<FrameKind>,
}

impl HostChannel {
    pub fn recv_activate(&mut self) -> Result<(), HostProtocolError> {
        self.require_state(HostState::WaitingActivation)?;
        let frame = read_frame(&mut self.stream, self.deadline)?;
        if frame.kind != FrameKind::Activate || !frame.payload.is_empty() {
            return Err(HostProtocolError::InvalidFrame);
        }
        self.state = HostState::PreparingContainment;
        Ok(())
    }

    pub fn send_contained(&mut self) -> Result<(), HostProtocolError> {
        self.require_state(HostState::PreparingContainment)?;
        write_frame(&mut self.stream, FrameKind::Contained, &[], self.deadline)?;
        self.state = HostState::WaitingCommand;
        Ok(())
    }

    pub fn recv_command(&mut self) -> Result<String, HostProtocolError> {
        self.require_state(HostState::WaitingCommand)?;
        let frame = read_frame(&mut self.stream, self.deadline)?;
        if frame.kind != FrameKind::Command || frame.payload.is_empty() {
            return Err(HostProtocolError::InvalidFrame);
        }
        let command =
            String::from_utf8(frame.payload).map_err(|_| HostProtocolError::InvalidFrame)?;
        self.state = HostState::WaitingReady;
        Ok(command)
    }

    pub fn send_ready(&mut self) -> Result<(), HostProtocolError> {
        self.require_state(HostState::WaitingReady)?;
        #[cfg(test)]
        if self.fail_next_send.take() == Some(FrameKind::Ready) {
            return Err(HostProtocolError::Io(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "scripted READY failure",
            )));
        }
        write_frame(&mut self.stream, FrameKind::Ready, &[], self.deadline)?;
        self.state = HostState::Running;
        self.deadline = None;
        Ok(())
    }

    pub fn recv_control(&mut self) -> Result<HostControl, HostProtocolError> {
        if !matches!(self.state, HostState::Running | HostState::Cleaning) {
            return Err(HostProtocolError::InvalidState);
        }
        let frame = match read_frame(&mut self.stream, self.deadline) {
            Ok(frame) => frame,
            Err(HostProtocolError::PeerClosed) => {
                self.state = HostState::Cleaning;
                return Ok(HostControl::ParentEof);
            }
            Err(error) => return Err(error),
        };
        if frame.kind != FrameKind::Stop || !frame.payload.is_empty() {
            return Err(HostProtocolError::InvalidFrame);
        }
        self.state = HostState::Cleaning;
        Ok(HostControl::Stop)
    }

    pub fn send_cleanup_failed(&mut self) -> Result<(), HostProtocolError> {
        if !matches!(self.state, HostState::Running | HostState::Cleaning) {
            return Err(HostProtocolError::InvalidState);
        }
        write_frame(
            &mut self.stream,
            FrameKind::CleanupFailed,
            &[],
            self.deadline,
        )?;
        self.state = HostState::Cleaning;
        Ok(())
    }

    pub fn send_done(&mut self, exit: RootExit) -> Result<(), HostProtocolError> {
        if !matches!(self.state, HostState::Running | HostState::Cleaning) {
            return Err(HostProtocolError::InvalidState);
        }
        let payload = encode_root_exit(exit);
        write_frame(&mut self.stream, FrameKind::Done, &payload, self.deadline)?;
        self.state = HostState::Done;
        Ok(())
    }

    fn require_state(&self, expected: HostState) -> Result<(), HostProtocolError> {
        if self.state == expected {
            Ok(())
        } else {
            Err(HostProtocolError::InvalidState)
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
enum FrameKind {
    Authenticate = 1,
    Activate = 2,
    Contained = 3,
    Command = 4,
    Ready = 5,
    Stop = 6,
    CleanupFailed = 7,
    Done = 8,
}

impl TryFrom<u8> for FrameKind {
    type Error = HostProtocolError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Authenticate),
            2 => Ok(Self::Activate),
            3 => Ok(Self::Contained),
            4 => Ok(Self::Command),
            5 => Ok(Self::Ready),
            6 => Ok(Self::Stop),
            7 => Ok(Self::CleanupFailed),
            8 => Ok(Self::Done),
            _ => Err(HostProtocolError::InvalidFrame),
        }
    }
}

struct Frame {
    kind: FrameKind,
    payload: Vec<u8>,
}

fn write_frame(
    stream: &mut TcpStream,
    kind: FrameKind,
    payload: &[u8],
    deadline: Option<Instant>,
) -> Result<(), HostProtocolError> {
    validate_payload_length(kind, payload.len())?;
    let payload_len = u32::try_from(payload.len()).map_err(|_| HostProtocolError::InvalidFrame)?;
    set_write_deadline(stream, deadline)?;
    let mut header = [0_u8; HEADER_BYTES];
    header[..8].copy_from_slice(&PROTOCOL_MAGIC);
    header[8..10].copy_from_slice(&PROTOCOL_VERSION.to_be_bytes());
    header[10] = kind as u8;
    header[11..15].copy_from_slice(&payload_len.to_be_bytes());
    stream.write_all(&header)?;
    stream.write_all(payload)?;
    stream.flush()?;
    Ok(())
}

fn read_frame(
    stream: &mut TcpStream,
    deadline: Option<Instant>,
) -> Result<Frame, HostProtocolError> {
    set_read_deadline(stream, deadline)?;
    let mut header = [0_u8; HEADER_BYTES];
    read_header(stream, &mut header)?;
    if header[..8] != PROTOCOL_MAGIC || header[8..10] != PROTOCOL_VERSION.to_be_bytes() {
        return Err(HostProtocolError::InvalidFrame);
    }
    let kind = FrameKind::try_from(header[10])?;
    let payload_len = u32::from_be_bytes(
        header[11..15]
            .try_into()
            .map_err(|_| HostProtocolError::InvalidFrame)?,
    ) as usize;
    validate_payload_length(kind, payload_len)?;
    let mut payload = vec![0_u8; payload_len];
    read_payload(stream, &mut payload)?;
    Ok(Frame { kind, payload })
}

fn validate_payload_length(kind: FrameKind, payload_len: usize) -> Result<(), HostProtocolError> {
    let valid = match kind {
        FrameKind::Authenticate => payload_len == AUTH_TOKEN_BYTES,
        FrameKind::Command => (1..=MAX_SHELL_COMMAND_BYTES).contains(&payload_len),
        FrameKind::Done => payload_len == 5,
        FrameKind::Activate
        | FrameKind::Contained
        | FrameKind::Ready
        | FrameKind::Stop
        | FrameKind::CleanupFailed => payload_len == 0,
    };
    if valid {
        Ok(())
    } else {
        Err(HostProtocolError::InvalidFrame)
    }
}

fn read_header(
    stream: &mut TcpStream,
    header: &mut [u8; HEADER_BYTES],
) -> Result<(), HostProtocolError> {
    let mut read = 0;
    while read < header.len() {
        match stream.read(&mut header[read..]) {
            Ok(0) if read == 0 => return Err(HostProtocolError::PeerClosed),
            Ok(0) => return Err(HostProtocolError::InvalidFrame),
            Ok(bytes) => read += bytes,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(map_read_error(error)),
        }
    }
    Ok(())
}

fn read_payload(stream: &mut TcpStream, payload: &mut [u8]) -> Result<(), HostProtocolError> {
    let mut read = 0;
    while read < payload.len() {
        match stream.read(&mut payload[read..]) {
            Ok(0) => return Err(HostProtocolError::InvalidFrame),
            Ok(bytes) => read += bytes,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(map_read_error(error)),
        }
    }
    Ok(())
}

fn set_read_deadline(
    stream: &TcpStream,
    deadline: Option<Instant>,
) -> Result<(), HostProtocolError> {
    stream.set_read_timeout(deadline.map(remaining).transpose()?)?;
    Ok(())
}

fn set_write_deadline(
    stream: &TcpStream,
    deadline: Option<Instant>,
) -> Result<(), HostProtocolError> {
    stream.set_write_timeout(deadline.map(remaining).transpose()?)?;
    Ok(())
}

fn remaining(deadline: Instant) -> Result<Duration, HostProtocolError> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|remaining| !remaining.is_zero())
        .ok_or(HostProtocolError::DeadlineExceeded)
}

fn map_read_error(error: io::Error) -> HostProtocolError {
    if matches!(
        error.kind(),
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
    ) {
        HostProtocolError::DeadlineExceeded
    } else {
        HostProtocolError::Io(error)
    }
}

fn encode_root_exit(exit: RootExit) -> [u8; 5] {
    match exit {
        RootExit::Code(code) => {
            let mut payload = [0_u8; 5];
            payload[1..].copy_from_slice(&code.to_be_bytes());
            payload
        }
        RootExit::Signal(signal) => [1, signal, 0, 0, 0],
    }
}

fn decode_root_exit(payload: &[u8]) -> Result<RootExit, HostProtocolError> {
    match payload {
        [0, code @ ..] if code.len() == 4 => Ok(RootExit::Code(i32::from_be_bytes(
            code.try_into()
                .map_err(|_| HostProtocolError::InvalidFrame)?,
        ))),
        [1, signal, 0, 0, 0] => Ok(RootExit::Signal(*signal)),
        _ => Err(HostProtocolError::InvalidFrame),
    }
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn required_utf8(
    get: &impl Fn(&str) -> Option<OsString>,
    key: &str,
) -> Result<String, HostProtocolError> {
    get(key)
        .and_then(|value| value.into_string().ok())
        .filter(|value| !value.is_empty())
        .ok_or(HostProtocolError::InvalidBootstrap)
}

fn decode_token(encoded: &str) -> Result<[u8; AUTH_TOKEN_BYTES], HostProtocolError> {
    if encoded.len() != AUTH_TOKEN_BYTES * 2 || !encoded.is_ascii() {
        return Err(HostProtocolError::InvalidBootstrap);
    }
    let mut token = [0_u8; AUTH_TOKEN_BYTES];
    for (index, pair) in encoded.as_bytes().chunks_exact(2).enumerate() {
        token[index] = (decode_hex_digit(pair[0])? << 4) | decode_hex_digit(pair[1])?;
    }
    Ok(token)
}

fn decode_hex_digit(digit: u8) -> Result<u8, HostProtocolError> {
    match digit {
        b'0'..=b'9' => Ok(digit - b'0'),
        b'a'..=b'f' => Ok(digit - b'a' + 10),
        _ => Err(HostProtocolError::InvalidBootstrap),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, VecDeque};
    use std::ffi::OsString;
    use std::io::{self, Write as _};
    use std::net::{Shutdown, TcpListener, TcpStream};
    use std::sync::{Arc, Mutex, mpsc};
    use std::thread;
    use std::time::{Duration, Instant};

    use minimax_protocol::MAX_SHELL_COMMAND_BYTES;

    use super::{
        AUTH_TOKEN_BYTES, FrameKind, HOST_ADDRESS_ENV, HOST_TIMEOUT_ENV, HOST_TOKEN_ENV,
        HOST_VERSION_ENV, HostBootstrap, HostChannel, HostControl, HostEvent, HostListener,
        HostProtocolError, HostState, HostSupervisor, PROTOCOL_MAGIC, PROTOCOL_VERSION, RootExit,
        read_frame, run_host_lifecycle, write_frame,
    };

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_trusted_host_supervisor_is_available_to_the_internal_host() {
        let _ = super::LinuxProcessSupervisor::new();
    }

    #[cfg(windows)]
    #[test]
    fn windows_command_payload_is_utf8_exclusive_and_guard_deleted() {
        let command = "Write-Output '雪-32k'";
        let payload = super::WindowsCommandPayload::stage(command).expect("stage command");
        let path = payload.path().to_owned();
        assert_eq!(
            std::fs::read_to_string(&path).expect("read payload"),
            command
        );
        drop(payload);
        assert!(!path.exists(), "payload guard must delete its path");
    }

    #[cfg(windows)]
    #[test]
    fn windows_powershell_arguments_are_one_bounded_constant_bootstrap() {
        let shell = super::resolve_windows_process_shell().expect("PowerShell");
        assert!(
            shell
                .args
                .iter()
                .any(|argument| argument == super::WINDOWS_COMMAND_BOOTSTRAP)
        );
        assert!(shell.args.iter().map(String::len).sum::<usize>() < 1_024);
    }

    #[cfg(windows)]
    #[test]
    fn windows_command_payload_bootstrap_decodes_unicode_and_removes_path_environment() {
        let command = "Write-Output 'payload-unicode-雪🙂'; Write-Output \"payload-env-empty=$([String]::IsNullOrEmpty($env:MINIMAX_SHELL_COMMAND_PATH))\"";
        let output = run_windows_command_payload(command);
        let stdout = String::from_utf8(output.stdout).expect("UTF-8 PowerShell stdout");

        assert_eq!(output.status.code(), Some(0), "{stdout:?}");
        assert!(stdout.contains("payload-unicode-雪🙂"), "{stdout:?}");
        assert!(stdout.contains("payload-env-empty=True"), "{stdout:?}");
    }

    #[cfg(windows)]
    #[test]
    fn windows_command_payload_bootstrap_removes_ack_variables_before_user_code() {
        let command = "'p','a','t','s','h','o','b','n' | ForEach-Object { Write-Output \"$_=$($null -eq (Get-Variable -Name $_ -ValueOnly -ErrorAction SilentlyContinue))\" }";
        let output = run_windows_command_payload(command);
        let stdout = String::from_utf8(output.stdout).expect("UTF-8 PowerShell stdout");

        assert_eq!(output.status.code(), Some(0), "{stdout:?}");
        for variable in ["p", "a", "t", "s", "h", "o", "b", "n"] {
            assert!(stdout.contains(&format!("{variable}=True")), "{stdout:?}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_command_payload_bootstrap_rejects_invalid_utf8_and_deletes_payload() {
        let payload = super::WindowsCommandPayload::stage("Write-Output 'must-not-run'")
            .expect("stage command");
        let path = payload.path().to_owned();
        std::fs::write(&path, [0xff]).expect("replace payload with invalid UTF-8");
        let shell = super::resolve_windows_process_shell().expect("PowerShell");
        let acknowledgement = super::WindowsBootstrapAcknowledgement::bind(
            std::time::Instant::now() + Duration::from_secs(5),
        )
        .expect("bind bootstrap acknowledgement");
        let mut process = std::process::Command::new(shell.program);
        process
            .args(shell.args)
            .env(super::WINDOWS_COMMAND_PATH_ENV, &path)
            .envs(acknowledgement.environment())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        let mut child = process.spawn().expect("run PowerShell bootstrap");
        acknowledgement
            .wait(&mut child)
            .expect_err("invalid UTF-8 must not acknowledge bootstrap readiness");
        let output = child.wait_with_output().expect("collect PowerShell output");

        assert!(!output.status.success(), "invalid UTF-8 must fail decoding");
        assert!(!String::from_utf8_lossy(&output.stdout).contains("must-not-run"));
        assert!(!path.exists(), "bootstrap must delete a rejected payload");
    }

    #[cfg(windows)]
    #[test]
    fn windows_command_payload_keeps_user_command_out_of_process_argv() {
        let marker = "process-list-secret-marker";
        let command = format!(
            "$m='{marker}'; if ([Environment]::CommandLine.Contains($m)) {{ Write-Output 'argv-dirty' }} else {{ Write-Output 'argv-clean' }}"
        );
        let output = run_windows_command_payload(&command);
        let stdout = String::from_utf8(output.stdout).expect("UTF-8 PowerShell stdout");
        let stderr = String::from_utf8(output.stderr).expect("UTF-8 PowerShell stderr");

        assert_eq!(output.status.code(), Some(0), "{stderr:?}");
        assert!(stdout.contains("argv-clean"), "{stdout:?}");
        assert!(!stdout.contains("argv-dirty"), "{stdout:?}");
        assert!(!stdout.contains(marker), "{stdout:?}");
        assert!(!stderr.contains(marker), "{stderr:?}");
    }

    #[cfg(windows)]
    fn run_windows_command_payload(command: &str) -> std::process::Output {
        let payload = super::WindowsCommandPayload::stage(command).expect("stage command");
        let path = payload.path().to_owned();
        let shell = super::resolve_windows_process_shell().expect("PowerShell");
        let acknowledgement = super::WindowsBootstrapAcknowledgement::bind(
            std::time::Instant::now() + Duration::from_secs(5),
        )
        .expect("bind bootstrap acknowledgement");
        let mut process = std::process::Command::new(shell.program);
        process
            .args(shell.args)
            .env(super::WINDOWS_COMMAND_PATH_ENV, &path)
            .envs(acknowledgement.environment())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        let mut child = process.spawn().expect("run PowerShell bootstrap");
        if let Err(error) = acknowledgement.wait(&mut child) {
            let output = child
                .wait_with_output()
                .expect("collect failed PowerShell output");
            panic!(
                "PowerShell acknowledges bootstrap: {error}; stdout={:?}; stderr={:?}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let output = child.wait_with_output().expect("collect PowerShell output");
        assert!(!path.exists(), "bootstrap must delete its payload");
        output
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_proc_stat_parser_uses_the_last_process_name_delimiter() {
        let stat = "42 (worker ) with spaces) S 7 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 123456";

        let identity = super::parse_linux_process_stat(stat).expect("parse Linux /proc stat");

        assert_eq!(
            identity,
            super::LinuxProcessIdentity {
                pid: 42,
                parent_pid: 7,
                start_time: 123_456,
                state: 'S',
            }
        );
        assert!(super::parse_linux_process_stat("42 (truncated) S 7").is_err());
        assert!(super::parse_linux_process_stat("missing delimiters").is_err());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_descendant_bfs_includes_adopted_daemons_and_excludes_unrelated_processes() {
        fn process(pid: u32, parent_pid: u32) -> super::LinuxProcessIdentity {
            super::LinuxProcessIdentity {
                pid,
                parent_pid,
                start_time: u64::from(pid) * 10,
                state: 'S',
            }
        }

        let descendants = super::linux_descendants_from_table(
            100,
            vec![
                process(100, 1),
                process(101, 100),
                process(102, 101),
                process(103, 100),
                process(200, 1),
            ],
        );
        let process_ids = descendants
            .into_iter()
            .map(|process| process.pid)
            .collect::<Vec<_>>();

        assert_eq!(process_ids, vec![101, 103, 102]);
    }

    #[test]
    fn bootstrap_contains_only_fixed_ipc_metadata_and_never_the_command() {
        let command = "Write-Output preassignment-secret-marker";
        let (listener, bootstrap) = HostListener::bind_for_test(Duration::from_secs(1), [0x5a; 32])
            .expect("bind loopback host listener");

        assert_eq!(bootstrap.arguments(), ["--minimax-internal-shell-host"]);
        assert_eq!(bootstrap.environment().len(), 4);
        assert!(
            bootstrap
                .environment()
                .iter()
                .all(|(key, value)| !key.contains(command) && !value.contains(command))
        );
        assert!(bootstrap.address().ip().is_loopback());

        drop(listener);
    }

    #[test]
    fn cleanup_failure_is_nonterminal_and_stop_can_be_retried_until_done() {
        let (listener, bootstrap) = HostListener::bind_for_test(Duration::from_secs(2), [0x37; 32])
            .expect("bind loopback host listener");
        let host = thread::spawn(move || {
            let mut channel = bootstrap
                .connect()
                .expect("host connects and authenticates");
            channel.recv_activate().expect("activation");
            channel.send_contained().expect("contained");
            assert_eq!(channel.recv_command().expect("command"), "exit-with-seven");
            channel.send_ready().expect("ready");
            assert_eq!(
                channel.recv_control().expect("first stop"),
                HostControl::Stop
            );
            channel
                .send_cleanup_failed()
                .expect("cleanup failure remains reportable");
            assert_eq!(
                channel.recv_control().expect("retry stop"),
                HostControl::Stop
            );
            channel
                .send_done(RootExit::Code(7))
                .expect("done only after fixed-point cleanup");
        });

        let mut channel = listener.accept().expect("authenticated host");
        channel.send_activate().expect("activate");
        assert_eq!(
            channel.recv_event().expect("contained"),
            HostEvent::Contained
        );
        channel.send_command("exit-with-seven").expect("command");
        assert_eq!(channel.recv_event().expect("ready"), HostEvent::Ready);
        channel.send_stop().expect("first stop");
        assert_eq!(
            channel.recv_event().expect("cleanup failed"),
            HostEvent::CleanupFailed
        );
        channel.send_stop().expect("retry stop");
        assert_eq!(
            channel.recv_event().expect("done"),
            HostEvent::Done(RootExit::Code(7))
        );

        host.join().expect("host thread");
    }

    #[test]
    fn wrong_authentication_is_discarded_before_a_valid_host_is_accepted() {
        let (listener, bootstrap) = HostListener::bind_for_test(Duration::from_secs(2), [0x42; 32])
            .expect("bind loopback host listener");
        let mut impostor = TcpStream::connect(bootstrap.address()).expect("connect impostor");
        write_frame(&mut impostor, FrameKind::Authenticate, &[0x24; 32], None)
            .expect("write wrong authentication");
        drop(impostor);

        let host = thread::spawn(move || bootstrap.connect().expect("valid host connects"));
        let parent = listener
            .accept()
            .expect("listener skips impostor and accepts valid host");

        drop(parent);
        drop(host.join().expect("host thread"));
    }

    #[test]
    fn listener_uses_one_real_clock_deadline_when_no_host_connects() {
        let (listener, bootstrap) =
            HostListener::bind_for_test(Duration::from_millis(60), [0x19; 32])
                .expect("bind loopback host listener");
        let started = Instant::now();
        let (result_tx, result_rx) = mpsc::sync_channel(1);
        let accept = thread::spawn(move || {
            result_tx
                .send(listener.accept())
                .expect("publish accept result");
        });

        let result = match result_rx.recv_timeout(Duration::from_millis(250)) {
            Ok(result) => result,
            Err(_) => {
                let wake = TcpStream::connect(bootstrap.address()).expect("wake stuck listener");
                wake.shutdown(Shutdown::Both)
                    .expect("close wake connection");
                accept.join().expect("stuck accept thread");
                panic!("listener ignored its total deadline");
            }
        };
        accept.join().expect("accept thread");
        let error = match result {
            Ok(_) => panic!("listener must time out"),
            Err(error) => error,
        };

        assert!(matches!(error, HostProtocolError::DeadlineExceeded));
        assert!(started.elapsed() >= Duration::from_millis(40));
        assert!(started.elapsed() < Duration::from_millis(500));
    }

    #[test]
    fn oversized_frame_is_rejected_from_its_header_before_payload_read() {
        let (mut sender, mut receiver) = loopback_pair();
        sender
            .write_all(&raw_header(
                FrameKind::Command,
                u32::try_from(MAX_SHELL_COMMAND_BYTES + 1).expect("bounded length"),
            ))
            .expect("write oversized header");

        let error = match read_frame(&mut receiver, Some(Instant::now() + Duration::from_secs(1))) {
            Ok(_) => panic!("oversized frame must fail from header alone"),
            Err(error) => error,
        };

        assert!(matches!(error, HostProtocolError::InvalidFrame));
    }

    #[test]
    fn short_header_and_short_payload_fail_closed() {
        let (mut header_sender, mut header_receiver) = loopback_pair();
        header_sender
            .write_all(&PROTOCOL_MAGIC[..4])
            .expect("write short header");
        header_sender
            .shutdown(Shutdown::Write)
            .expect("close short header sender");
        assert!(matches!(
            read_frame(
                &mut header_receiver,
                Some(Instant::now() + Duration::from_secs(1))
            ),
            Err(HostProtocolError::InvalidFrame)
        ));

        let (mut payload_sender, mut payload_receiver) = loopback_pair();
        payload_sender
            .write_all(&raw_header(FrameKind::Command, 3))
            .expect("write payload header");
        payload_sender.write_all(b"x").expect("write short payload");
        payload_sender
            .shutdown(Shutdown::Write)
            .expect("close short payload sender");
        assert!(matches!(
            read_frame(
                &mut payload_receiver,
                Some(Instant::now() + Duration::from_secs(1))
            ),
            Err(HostProtocolError::InvalidFrame)
        ));
    }

    #[test]
    fn invalid_utf8_command_is_rejected_before_shell_start() {
        let (mut parent, host) = loopback_pair();
        write_frame(&mut parent, FrameKind::Command, &[0xff], None)
            .expect("write invalid UTF-8 command frame");
        let mut channel = HostChannel {
            stream: host,
            state: HostState::WaitingCommand,
            deadline: Some(Instant::now() + Duration::from_secs(1)),
            fail_next_send: None,
        };

        assert!(matches!(
            channel.recv_command(),
            Err(HostProtocolError::InvalidFrame)
        ));
    }

    #[test]
    fn protocol_errors_map_to_stable_secret_free_io_errors() {
        let error = io::Error::from(HostProtocolError::AuthenticationFailed);

        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(error.to_string(), "shell host authentication failed");
    }

    #[test]
    fn bootstrap_environment_rejects_invalid_values_without_echoing_them() {
        let cases = [
            ("missing address", HOST_ADDRESS_ENV, None),
            (
                "non-loopback address",
                HOST_ADDRESS_ENV,
                Some("192.0.2.7:4567"),
            ),
            ("IPv6 loopback", HOST_ADDRESS_ENV, Some("[::1]:4567")),
            ("short token", HOST_TOKEN_ENV, Some("aa")),
            (
                "uppercase token",
                HOST_TOKEN_ENV,
                Some("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"),
            ),
            (
                "nonhex token",
                HOST_TOKEN_ENV,
                Some("gggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggg"),
            ),
            ("wrong version", HOST_VERSION_ENV, Some("2")),
            ("zero timeout", HOST_TIMEOUT_ENV, Some("0")),
            ("oversized timeout", HOST_TIMEOUT_ENV, Some("300001")),
        ];

        for (name, key, value) in cases {
            let mut environment = valid_bootstrap_environment();
            match value {
                Some(value) => {
                    environment.insert(key, value.into());
                }
                None => {
                    environment.remove(key);
                }
            }

            let error = match HostBootstrap::from_environment(|requested| {
                environment.get(requested).cloned()
            }) {
                Ok(_) => panic!("{name}"),
                Err(error) => error,
            };

            assert!(matches!(error, HostProtocolError::InvalidBootstrap));
            if let Some(value) = value {
                assert!(!error.to_string().contains(value), "{name}");
            }
        }
    }

    #[cfg(windows)]
    #[test]
    fn bootstrap_environment_rejects_non_utf8_without_echoing_it() {
        use std::os::windows::ffi::OsStringExt as _;

        let mut environment = valid_bootstrap_environment();
        environment.insert(HOST_TOKEN_ENV, OsString::from_wide(&[0xd800]));

        let error = match HostBootstrap::from_environment(|requested| {
            environment.get(requested).cloned()
        }) {
            Ok(_) => panic!("non-UTF-8 bootstrap value must fail"),
            Err(error) => error,
        };

        assert!(matches!(error, HostProtocolError::InvalidBootstrap));
        assert_eq!(error.to_string(), "invalid shell host bootstrap");
    }

    fn valid_bootstrap_environment() -> BTreeMap<&'static str, OsString> {
        BTreeMap::from([
            (HOST_ADDRESS_ENV, "127.0.0.1:4567".into()),
            (HOST_TOKEN_ENV, "5a".repeat(AUTH_TOKEN_BYTES).into()),
            (HOST_VERSION_ENV, PROTOCOL_VERSION.to_string().into()),
            (HOST_TIMEOUT_ENV, "30000".into()),
        ])
    }

    #[test]
    fn lifecycle_preflights_before_contained_and_preserves_natural_exit_seven() {
        let state = Arc::new(Mutex::new(ScriptedState {
            root_polls: VecDeque::from([Some(RootExit::Code(7))]),
            cleanup: VecDeque::from([CleanupStep::Done(RootExit::Code(7))]),
            ..ScriptedState::default()
        }));
        let (listener, bootstrap) = HostListener::bind_for_test(Duration::from_secs(2), [0x71; 32])
            .expect("lifecycle listener");
        let host_state = Arc::clone(&state);
        let host = thread::spawn(move || {
            let channel = bootstrap.connect().expect("host connects");
            run_host_lifecycle(
                channel,
                ScriptedSupervisor { state: host_state },
                Duration::from_millis(2),
            )
        });

        let mut parent = listener.accept().expect("accept host");
        parent.send_activate().expect("activate host");
        assert_eq!(
            parent.recv_event().expect("contained"),
            HostEvent::Contained
        );
        parent.send_command("exit 7").expect("deliver command");
        assert_eq!(parent.recv_event().expect("ready"), HostEvent::Ready);
        assert_eq!(
            parent.recv_event().expect("done"),
            HostEvent::Done(RootExit::Code(7))
        );
        assert_eq!(
            host.join()
                .expect("host thread")
                .expect("lifecycle succeeds"),
            RootExit::Code(7)
        );

        let state = state.lock().expect("scripted state");
        assert_eq!(state.events, ["preflight", "spawn", "poll", "cleanup"]);
        assert_eq!(state.commands, ["exit 7"]);
    }

    #[test]
    fn lifecycle_reports_cleanup_failure_once_then_retries_until_fixed_point() {
        let state = Arc::new(Mutex::new(ScriptedState {
            root_polls: VecDeque::from([Some(RootExit::Code(7))]),
            cleanup: VecDeque::from([
                CleanupStep::Fail,
                CleanupStep::Pending,
                CleanupStep::Done(RootExit::Code(7)),
            ]),
            ..ScriptedState::default()
        }));
        let (listener, bootstrap) = HostListener::bind_for_test(Duration::from_secs(2), [0x72; 32])
            .expect("retry listener");
        let host_state = Arc::clone(&state);
        let host = thread::spawn(move || {
            let channel = bootstrap.connect().expect("host connects");
            run_host_lifecycle(
                channel,
                ScriptedSupervisor { state: host_state },
                Duration::from_millis(2),
            )
        });

        let mut parent = listener.accept().expect("accept host");
        parent.send_activate().expect("activate host");
        assert_eq!(
            parent.recv_event().expect("contained"),
            HostEvent::Contained
        );
        parent.send_command("exit 7").expect("deliver command");
        assert_eq!(parent.recv_event().expect("ready"), HostEvent::Ready);
        assert_eq!(
            parent.recv_event().expect("one cleanup failure"),
            HostEvent::CleanupFailed
        );
        parent.send_stop().expect("retry stop remains accepted");
        assert_eq!(
            parent.recv_event().expect("eventual done"),
            HostEvent::Done(RootExit::Code(7))
        );
        assert_eq!(
            host.join()
                .expect("host thread")
                .expect("lifecycle succeeds"),
            RootExit::Code(7)
        );

        let state = state.lock().expect("scripted state");
        assert_eq!(
            state
                .events
                .iter()
                .filter(|event| **event == "cleanup")
                .count(),
            3
        );
    }

    #[test]
    fn lifecycle_keeps_normal_cleanup_pending_silent_until_done() {
        let state = Arc::new(Mutex::new(ScriptedState {
            root_polls: VecDeque::from([Some(RootExit::Code(0))]),
            cleanup: VecDeque::from([CleanupStep::Pending, CleanupStep::Done(RootExit::Code(0))]),
            ..ScriptedState::default()
        }));
        let (listener, bootstrap) = HostListener::bind_for_test(Duration::from_secs(2), [0x74; 32])
            .expect("pending listener");
        let host_state = Arc::clone(&state);
        let host = thread::spawn(move || {
            let channel = bootstrap.connect().expect("host connects");
            run_host_lifecycle(
                channel,
                ScriptedSupervisor { state: host_state },
                Duration::from_millis(2),
            )
        });

        let mut parent = listener.accept().expect("accept host");
        parent.send_activate().expect("activate host");
        assert_eq!(
            parent.recv_event().expect("contained"),
            HostEvent::Contained
        );
        parent.send_command("exit 0").expect("deliver command");
        assert_eq!(parent.recv_event().expect("ready"), HostEvent::Ready);
        assert_eq!(
            parent.recv_event().expect("pending remains silent"),
            HostEvent::Done(RootExit::Code(0))
        );
        assert_eq!(
            host.join()
                .expect("host thread")
                .expect("lifecycle succeeds"),
            RootExit::Code(0)
        );
    }

    #[test]
    fn lifecycle_treats_parent_eof_as_stop_and_cleans_before_exit() {
        let state = Arc::new(Mutex::new(ScriptedState {
            root_polls: VecDeque::from([None]),
            cleanup: VecDeque::from([CleanupStep::Done(RootExit::Code(0))]),
            ..ScriptedState::default()
        }));
        let (listener, bootstrap) =
            HostListener::bind_for_test(Duration::from_secs(2), [0x73; 32]).expect("EOF listener");
        let host_state = Arc::clone(&state);
        let host = thread::spawn(move || {
            let channel = bootstrap.connect().expect("host connects");
            run_host_lifecycle(
                channel,
                ScriptedSupervisor { state: host_state },
                Duration::from_millis(2),
            )
        });

        let mut parent = listener.accept().expect("accept host");
        parent.send_activate().expect("activate host");
        assert_eq!(
            parent.recv_event().expect("contained"),
            HostEvent::Contained
        );
        parent
            .send_command("long-running")
            .expect("deliver command");
        assert_eq!(parent.recv_event().expect("ready"), HostEvent::Ready);
        drop(parent);

        assert_eq!(
            host.join()
                .expect("host thread")
                .expect("lifecycle succeeds"),
            RootExit::Code(0)
        );
        let state = state.lock().expect("scripted state");
        assert_eq!(
            state
                .events
                .iter()
                .filter(|event| **event == "cleanup")
                .count(),
            1
        );
    }

    #[test]
    fn lifecycle_cleans_spawned_shell_when_parent_disconnects_before_ready() {
        let state = Arc::new(Mutex::new(ScriptedState {
            root_polls: VecDeque::from([None]),
            cleanup: VecDeque::from([CleanupStep::Done(RootExit::Code(0))]),
            ..ScriptedState::default()
        }));
        let (spawn_release_tx, spawn_release_rx) = mpsc::sync_channel(1);
        let (listener, bootstrap) = HostListener::bind_for_test(Duration::from_secs(2), [0x75; 32])
            .expect("pre-ready disconnect listener");
        let host_state = Arc::clone(&state);
        let host = thread::spawn(move || {
            let mut channel = bootstrap.connect().expect("host connects");
            channel.fail_next_send = Some(FrameKind::Ready);
            run_host_lifecycle(
                channel,
                BlockingSpawnSupervisor {
                    inner: ScriptedSupervisor { state: host_state },
                    release: spawn_release_rx,
                },
                Duration::from_millis(2),
            )
        });

        let mut parent = listener.accept().expect("accept host");
        parent.send_activate().expect("activate host");
        assert_eq!(
            parent.recv_event().expect("contained"),
            HostEvent::Contained
        );
        parent
            .send_command("spawn-then-disconnect")
            .expect("command");
        spawn_release_tx.send(()).expect("release successful spawn");

        assert!(matches!(
            parent.recv_event(),
            Err(HostProtocolError::PeerClosed)
        ));

        let error = host
            .join()
            .expect("host thread")
            .expect_err("READY failure remains an internal host failure");
        assert!(matches!(
            error,
            HostProtocolError::Io(error) if error.kind() == io::ErrorKind::BrokenPipe
        ));
        let state = state.lock().expect("scripted state");
        assert_eq!(
            state
                .events
                .iter()
                .filter(|event| **event == "cleanup")
                .count(),
            1
        );
    }

    #[test]
    fn lifecycle_cleans_partial_spawn_before_returning_a_post_command_failure() {
        let state = Arc::new(Mutex::new(ScriptedState {
            cleanup: VecDeque::from([CleanupStep::Done(RootExit::Code(125))]),
            ..ScriptedState::default()
        }));
        let (spawn_release_tx, spawn_release_rx) = mpsc::sync_channel(1);
        let (listener, bootstrap) = HostListener::bind_for_test(Duration::from_secs(2), [0x76; 32])
            .expect("partial spawn listener");
        let host_state = Arc::clone(&state);
        let host = thread::spawn(move || {
            let channel = bootstrap.connect().expect("host connects");
            run_host_lifecycle(
                channel,
                BlockingSpawnSupervisor {
                    inner: ScriptedSupervisor { state: host_state },
                    release: spawn_release_rx,
                },
                Duration::from_millis(2),
            )
        });

        let mut parent = listener.accept().expect("accept host");
        parent.send_activate().expect("activate host");
        assert_eq!(
            parent.recv_event().expect("contained"),
            HostEvent::Contained
        );
        parent
            .send_command("spawn-then-fail")
            .expect("deliver command");
        drop(spawn_release_tx);

        assert!(matches!(
            parent.recv_event(),
            Err(HostProtocolError::PeerClosed)
        ));
        assert!(host.join().expect("host thread").is_err());
        let state = state.lock().expect("scripted state");
        assert_eq!(
            state
                .events
                .iter()
                .filter(|event| **event == "cleanup")
                .count(),
            1
        );
    }

    trait ScriptedCleanupResult {
        fn into_result(self) -> io::Result<Option<RootExit>>;
    }

    enum CleanupStep {
        Fail,
        Pending,
        Done(RootExit),
    }

    impl ScriptedCleanupResult for CleanupStep {
        fn into_result(self) -> io::Result<Option<RootExit>> {
            match self {
                Self::Fail => Err(io::Error::other("scripted cleanup failure")),
                Self::Pending => Ok(None),
                Self::Done(exit) => Ok(Some(exit)),
            }
        }
    }

    #[derive(Default)]
    struct ScriptedState {
        events: Vec<&'static str>,
        commands: Vec<String>,
        root_polls: VecDeque<Option<RootExit>>,
        cleanup: VecDeque<CleanupStep>,
    }

    struct ScriptedSupervisor {
        state: Arc<Mutex<ScriptedState>>,
    }

    struct BlockingSpawnSupervisor {
        inner: ScriptedSupervisor,
        release: mpsc::Receiver<()>,
    }

    impl HostSupervisor for BlockingSpawnSupervisor {
        fn preflight(&mut self) -> io::Result<()> {
            self.inner.preflight()
        }

        fn spawn(&mut self, command: &str) -> io::Result<()> {
            self.inner.spawn(command)?;
            self.release
                .recv()
                .map_err(|_| io::Error::other("spawn release dropped"))
        }

        fn try_root_exit(&mut self) -> io::Result<Option<RootExit>> {
            self.inner.try_root_exit()
        }

        fn cleanup(&mut self) -> io::Result<Option<RootExit>> {
            self.inner.cleanup()
        }
    }

    impl HostSupervisor for ScriptedSupervisor {
        fn preflight(&mut self) -> io::Result<()> {
            self.state
                .lock()
                .expect("scripted state")
                .events
                .push("preflight");
            Ok(())
        }

        fn spawn(&mut self, command: &str) -> io::Result<()> {
            let mut state = self.state.lock().expect("scripted state");
            state.events.push("spawn");
            state.commands.push(command.to_owned());
            Ok(())
        }

        fn try_root_exit(&mut self) -> io::Result<Option<RootExit>> {
            let mut state = self.state.lock().expect("scripted state");
            state.events.push("poll");
            Ok(state.root_polls.pop_front().flatten())
        }

        fn cleanup(&mut self) -> io::Result<Option<RootExit>> {
            let mut state = self.state.lock().expect("scripted state");
            state.events.push("cleanup");
            state
                .cleanup
                .pop_front()
                .unwrap_or(CleanupStep::Pending)
                .into_result()
        }
    }

    fn loopback_pair() -> (TcpStream, TcpStream) {
        let listener =
            TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).expect("bind loopback pair");
        let address = listener.local_addr().expect("loopback address");
        let (sender_tx, sender_rx) = mpsc::sync_channel(1);
        let connector = thread::spawn(move || {
            sender_tx
                .send(TcpStream::connect(address).expect("connect loopback pair"))
                .expect("publish sender");
        });
        let (receiver, _) = listener.accept().expect("accept loopback pair");
        connector.join().expect("connector thread");
        (sender_rx.recv().expect("receive sender"), receiver)
    }

    fn raw_header(kind: FrameKind, payload_len: u32) -> [u8; 15] {
        let mut header = [0_u8; 15];
        header[..8].copy_from_slice(&PROTOCOL_MAGIC);
        header[8..10].copy_from_slice(&PROTOCOL_VERSION.to_be_bytes());
        header[10] = kind as u8;
        header[11..15].copy_from_slice(&payload_len.to_be_bytes());
        header
    }
}
