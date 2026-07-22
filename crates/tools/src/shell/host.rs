use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::time::{Duration, Instant};

use minimax_protocol::MAX_SHELL_COMMAND_BYTES;

pub const INTERNAL_HOST_ARGUMENT: &str = "--minimax-internal-shell-host";
const HOST_ADDRESS_ENV: &str = "MINIMAX_SHELL_HOST_ADDRESS";
const HOST_TOKEN_ENV: &str = "MINIMAX_SHELL_HOST_TOKEN";
const HOST_VERSION_ENV: &str = "MINIMAX_SHELL_HOST_VERSION";
const HOST_TIMEOUT_ENV: &str = "MINIMAX_SHELL_HOST_TIMEOUT_MS";
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

pub fn run_internal_shell_host_bootstrap() -> i32 {
    let result = HostBootstrap::from_current_environment()
        .and_then(HostBootstrap::connect)
        .and_then(|mut channel| channel.recv_activate());
    let _ = result;
    125
}

pub trait HostSupervisor {
    fn preflight(&mut self) -> io::Result<()>;
    fn spawn(&mut self, command: &str) -> io::Result<()>;
    fn try_root_exit(&mut self) -> io::Result<Option<RootExit>>;
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
    supervisor.spawn(&command)?;
    let mut root_exit = None;
    let mut cleaning = false;
    let mut failure_reported = false;
    let mut parent_connected = true;
    let mut terminal_error = None;
    let control_rx = match channel.stream.try_clone() {
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
    };

    loop {
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
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) =>
                {
                    std::thread::sleep(total_remaining.min(ACCEPT_POLL_INTERVAL));
                }
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
