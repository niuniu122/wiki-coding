use std::future::Future;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::pin::Pin;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use minimax_core::Clock;
use minimax_protocol::ShellSessionId;

use super::manager::ShellManagerError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShellIoMode {
    Pipe,
    Terminal { cols: u16, rows: u16 },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShellSpawnRequest {
    pub command: String,
    pub cwd: PathBuf,
    pub io_mode: ShellIoMode,
}

pub trait ShellChild: Send {
    fn process_id(&self) -> u32;
    fn try_wait(&mut self) -> io::Result<Option<i32>>;
    fn kill(&mut self) -> io::Result<()>;
    fn reap(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub trait ShellGuard: Send {
    fn terminate<'a>(&'a mut self) -> ShellTerminateFuture<'a>;
    fn confirm<'a>(&'a mut self) -> ShellTerminateFuture<'a>;
    fn close_io(&mut self) {}
    fn disarm(&mut self) {}
}

pub struct SpawnedShell {
    pub child: Box<dyn ShellChild>,
    pub reader: Box<dyn Read + Send>,
    pub writer: Box<dyn Write + Send>,
    pub guard: Box<dyn ShellGuard>,
}

pub type ReaderTask = Box<dyn FnOnce() + Send + 'static>;

pub trait ReaderSpawner: Send + Sync {
    fn spawn(&self, name: String, task: ReaderTask) -> io::Result<thread::JoinHandle<()>>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemReaderSpawner;

impl ReaderSpawner for SystemReaderSpawner {
    fn spawn(&self, name: String, task: ReaderTask) -> io::Result<thread::JoinHandle<()>> {
        thread::Builder::new().name(name).spawn(task)
    }
}

pub type ShellTerminateFuture<'a> = Pin<Box<dyn Future<Output = io::Result<()>> + Send + 'a>>;

pub trait ShellBackend: Send + Sync {
    fn requires_startup_cursor_handshake(&self) -> bool {
        false
    }

    fn spawn(&self, request: &ShellSpawnRequest) -> io::Result<SpawnedShell>;
}

pub trait ShellSessionIdSource: Send + Sync {
    fn next_session_id(&self) -> Result<ShellSessionId, ShellManagerError>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemShellClock;

impl Clock for SystemShellClock {
    fn now_unix_ms(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| {
                u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
            })
    }
}
