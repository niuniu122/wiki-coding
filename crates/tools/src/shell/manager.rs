use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex as StdMutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use minimax_core::{CancellationPort, Clock};
use minimax_protocol::{
    MAX_SHELL_COMMAND_BYTES, MAX_SHELL_CWD_BYTES, MAX_SHELL_INPUT_BYTES, MAX_SHELL_OUTPUT_BYTES,
    MAX_TOOL_RESULT_BYTES, ShellReceipt, ShellSessionId, ShellSessionState,
};
use tokio::sync::Mutex;

use super::backend::{
    ReaderSpawner, ShellBackend, ShellChild, ShellGuard, ShellIoMode, ShellSessionIdSource,
    ShellSpawnRequest, SpawnedShell, SystemReaderSpawner,
};
use super::buffer::{ShellOutputBudget, ShellOutputBuffer};

pub const MAX_RUNNING_SHELL_SESSIONS: usize = 8;
pub const MAX_TERMINAL_SHELL_RECEIPTS: usize = 32;
pub const TERMINAL_RECEIPT_TTL: Duration = Duration::from_secs(5 * 60);
pub const DEFAULT_COMMAND_YIELD: Duration = Duration::from_secs(10);
pub const DEFAULT_POLL_YIELD: Duration = Duration::from_secs(1);
pub const DEFAULT_WRITE_YIELD: Duration = Duration::from_millis(250);

const MAX_REQUEST_YIELD: Duration = Duration::from_secs(60);
const PROCESS_POLL_INTERVAL: Duration = Duration::from_millis(5);
const CLEANUP_WAIT: Duration = Duration::from_secs(2);
const DEFAULT_OUTPUT_BUDGET_BYTES: usize = 8 * 1_024 * 1_024;
const READER_CHUNK_BYTES: usize = 8 * 1_024;
const STARTUP_CURSOR_HANDSHAKE_WAIT: Duration = Duration::from_secs(2);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShellCommandRequest {
    pub command: String,
    pub cwd: PathBuf,
    pub tty: bool,
    pub yield_time: Duration,
    pub max_output_bytes: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShellPollRequest {
    pub session_id: ShellSessionId,
    pub yield_time: Duration,
    pub max_output_bytes: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShellWriteRequest {
    pub session_id: ShellSessionId,
    pub input: String,
    pub submit: bool,
    pub yield_time: Duration,
    pub max_output_bytes: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShellManagerError {
    Disabled,
    SessionNotFound,
    SessionLimit,
    InvalidArguments,
    Launch,
    Io,
    Cancelled,
    Indeterminate,
    Identifier,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShellCleanupError {
    pub session_ids: Vec<ShellSessionId>,
}

#[derive(Clone)]
pub struct ShellSessionManager {
    inner: Arc<Mutex<ShellSessionRegistry>>,
    unpublished_sessions: Arc<StdMutex<BTreeMap<ShellSessionId, Arc<StdMutex<ShellSession>>>>>,
    lifecycle: Arc<Mutex<()>>,
    backend: Arc<dyn ShellBackend>,
    ids: Arc<dyn ShellSessionIdSource>,
    clock: Arc<dyn Clock + Send + Sync>,
    output_budget: Arc<ShellOutputBudget>,
    slots: Arc<SlotLedger>,
    reader_spawner: Arc<dyn ReaderSpawner>,
    unpublished_sequence: Arc<AtomicU64>,
    #[cfg(test)]
    unpublished_registration_gate: Arc<StdMutex<Option<Arc<UnpublishedRegistrationGate>>>>,
}

#[cfg(test)]
struct UnpublishedRegistrationGate {
    entered: tokio::sync::Barrier,
    release: tokio::sync::Barrier,
}

struct ShellSessionRegistry {
    accepting: bool,
    terminal_sequence: u64,
    sessions: BTreeMap<ShellSessionId, Arc<StdMutex<ShellSession>>>,
}

struct ShellSession {
    id: ShellSessionId,
    guard: Option<Box<dyn ShellGuard>>,
    running_slot: Option<RunningSlotLease>,
    child: Arc<StdMutex<Box<dyn ShellChild>>>,
    writer: Option<Arc<StdMutex<Box<dyn Write + Send>>>>,
    output: Arc<StdMutex<ShellOutputBuffer>>,
    reader: ReaderState,
    state: ShellSessionState,
    stopping: bool,
    cleanup_generation: u64,
    cleanup_attempt: Option<Arc<CleanupAttempt>>,
    cleanup_succeeded: bool,
    exit_code: Option<i32>,
    terminal_at_unix_ms: Option<u64>,
    terminal_sequence: Option<u64>,
}

enum ReaderState {
    Pending {
        handle: thread::JoinHandle<()>,
        completion: Arc<ReaderCompletion>,
    },
    Complete,
    Failed,
}

#[derive(Default)]
struct ReaderCompletion {
    finished: StdMutex<bool>,
    completed: Condvar,
}

impl ReaderCompletion {
    fn finish(&self) {
        if let Ok(mut finished) = self.finished.lock() {
            *finished = true;
            self.completed.notify_all();
        }
    }

    fn wait_timeout(&self, timeout: Duration) -> bool {
        let Ok(finished) = self.finished.lock() else {
            return false;
        };
        if *finished {
            return true;
        }
        self.completed
            .wait_timeout_while(finished, timeout, |finished| !*finished)
            .is_ok_and(|(finished, _)| *finished)
    }
}

struct CleanupAttempt {
    generation: u64,
    result: StdMutex<Option<Result<(), ShellManagerError>>>,
    completed: tokio::sync::Notify,
}

impl CleanupAttempt {
    fn new(generation: u64) -> Self {
        Self {
            generation,
            result: StdMutex::new(None),
            completed: tokio::sync::Notify::new(),
        }
    }

    fn result(&self) -> Option<Result<(), ShellManagerError>> {
        self.result.lock().ok().and_then(|result| *result)
    }

    async fn wait(&self) -> Result<(), ShellManagerError> {
        loop {
            let notified = self.completed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if let Some(result) = self.result() {
                return result;
            }
            notified.as_mut().await;
        }
    }

    fn complete(&self, result: Result<(), ShellManagerError>) {
        if let Ok(mut completed) = self.result.lock() {
            debug_assert!(
                completed.is_none(),
                "cleanup attempt completes exactly once"
            );
            *completed = Some(result);
        }
        self.completed.notify_waiters();
    }
}

#[derive(Default)]
struct SlotLedger {
    starting: AtomicUsize,
    running: AtomicUsize,
    changed: tokio::sync::Notify,
}

impl SlotLedger {
    fn reserve(
        self: &Arc<Self>,
    ) -> Result<(StartingSlotLease, RunningSlotLease), ShellManagerError> {
        self.running
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |running| {
                (running < MAX_RUNNING_SHELL_SESSIONS).then_some(running + 1)
            })
            .map_err(|_| ShellManagerError::SessionLimit)?;
        self.starting.fetch_add(1, Ordering::AcqRel);
        Ok((
            StartingSlotLease {
                ledger: Arc::clone(self),
            },
            RunningSlotLease {
                ledger: Arc::clone(self),
            },
        ))
    }

    fn release_starting(&self) {
        let released = self
            .starting
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                count.checked_sub(1)
            });
        debug_assert!(released.is_ok(), "starting slot released exactly once");
        self.changed.notify_waiters();
    }

    fn release_running(&self) {
        let released = self
            .running
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                count.checked_sub(1)
            });
        debug_assert!(released.is_ok(), "running slot released exactly once");
        self.changed.notify_waiters();
    }
}

struct StartingSlotLease {
    ledger: Arc<SlotLedger>,
}

impl Drop for StartingSlotLease {
    fn drop(&mut self) {
        self.ledger.release_starting();
    }
}

struct RunningSlotLease {
    ledger: Arc<SlotLedger>,
}

impl Drop for RunningSlotLease {
    fn drop(&mut self) {
        self.ledger.release_running();
    }
}

impl ShellSessionManager {
    #[must_use]
    pub fn new(
        backend: Arc<dyn ShellBackend>,
        ids: Arc<dyn ShellSessionIdSource>,
        clock: Arc<dyn Clock + Send + Sync>,
    ) -> Self {
        Self::new_with_reader_spawner(backend, ids, clock, Arc::new(SystemReaderSpawner))
    }

    #[must_use]
    pub fn new_with_reader_spawner(
        backend: Arc<dyn ShellBackend>,
        ids: Arc<dyn ShellSessionIdSource>,
        clock: Arc<dyn Clock + Send + Sync>,
        reader_spawner: Arc<dyn ReaderSpawner>,
    ) -> Self {
        Self {
            inner: Arc::new(Mutex::new(ShellSessionRegistry {
                accepting: false,
                terminal_sequence: 0,
                sessions: BTreeMap::new(),
            })),
            unpublished_sessions: Arc::new(StdMutex::new(BTreeMap::new())),
            lifecycle: Arc::new(Mutex::new(())),
            backend,
            ids,
            clock,
            output_budget: Arc::new(ShellOutputBudget::new(DEFAULT_OUTPUT_BUDGET_BYTES)),
            slots: Arc::new(SlotLedger::default()),
            reader_spawner,
            unpublished_sequence: Arc::new(AtomicU64::new(0)),
            #[cfg(test)]
            unpublished_registration_gate: Arc::new(StdMutex::new(None)),
        }
    }

    pub async fn enable(&self) {
        let _lifecycle = self.lifecycle.lock().await;
        self.gc().await;
        self.inner.lock().await.accepting = true;
    }

    pub async fn start(
        &self,
        request: ShellCommandRequest,
        cancellation: &dyn CancellationPort,
    ) -> Result<ShellReceipt, ShellManagerError> {
        self.gc().await;
        self.reap_newly_terminal_sessions().await;
        validate_command_request(&request)?;
        if cancellation.is_cancelled() {
            return Err(ShellManagerError::Cancelled);
        }
        let (starting_slot, running_slot) = self.reserve_running_slot().await?;
        let mut pending_start = PendingStart::reserved(self.clone(), starting_slot, running_slot);

        let io_mode = if request.tty {
            ShellIoMode::Terminal {
                cols: 120,
                rows: 30,
            }
        } else {
            ShellIoMode::Pipe
        };
        let spawn_request = ShellSpawnRequest {
            command: request.command,
            cwd: request.cwd,
            io_mode,
        };
        let spawned = match self.backend.spawn(&spawn_request) {
            Ok(spawned) => spawned,
            Err(error) => {
                let (_source, cleanup) = error.into_parts();
                if let Some(spawned) = cleanup {
                    let resources = self
                        .own_spawned_resources(spawned, io_mode)
                        .unwrap_or_else(|resources| resources);
                    pending_start.own_resources(resources);
                }
                let (manager, session) = pending_start.take_settlement();
                manager.settle_unpublished_start(session).await?;
                return Err(ShellManagerError::Launch);
            }
        };
        match self.own_spawned_resources(spawned, io_mode) {
            Ok(resources) => pending_start.own_resources(resources),
            Err(resources) => {
                pending_start.own_resources(resources);
                let (manager, session) = pending_start.take_settlement();
                manager.settle_unpublished_start(session).await?;
                return Err(ShellManagerError::Io);
            }
        }
        if let Err(error) = self
            .wait_for_startup_cursor_handshake(pending_start.resources_mut(), cancellation)
            .await
        {
            let (manager, session) = pending_start.take_settlement();
            manager.settle_unpublished_start(session).await?;
            return Err(error);
        }
        let id = match self.ids.next_session_id() {
            Ok(id) => id,
            Err(_) => {
                let (manager, session) = pending_start.take_settlement();
                manager.settle_unpublished_start(session).await?;
                return Err(ShellManagerError::Identifier);
            }
        };
        let session = Arc::new(StdMutex::new(
            pending_start
                .take_resources()
                .into_session(id.clone(), pending_start.take_running_slot()),
        ));
        pending_start.own_session(Arc::clone(&session));

        let published = {
            let mut registry = self.inner.lock().await;
            if !registry.accepting || registry.sessions.contains_key(&id) {
                false
            } else {
                registry.sessions.insert(id.clone(), Arc::clone(&session));
                pending_start.disarm();
                true
            }
        };
        if !published {
            let (manager, settlement_session) = pending_start.take_settlement();
            manager.settle_unpublished_start(settlement_session).await?;
            let accepting = self.inner.lock().await.accepting;
            return Err(if accepting {
                ShellManagerError::Identifier
            } else {
                ShellManagerError::Disabled
            });
        }

        match self
            .wait_for_receipt(
                Arc::clone(&session),
                request.yield_time,
                request.max_output_bytes,
                cancellation,
                false,
            )
            .await
        {
            Err(ShellManagerError::Cancelled) => {
                match self.stop_entry(session, MAX_SHELL_OUTPUT_BYTES).await {
                    Ok(_) => Err(ShellManagerError::Cancelled),
                    Err(_) => Err(ShellManagerError::Indeterminate),
                }
            }
            result => result,
        }
    }

    pub async fn poll(
        &self,
        request: ShellPollRequest,
        cancellation: &dyn CancellationPort,
    ) -> Result<ShellReceipt, ShellManagerError> {
        self.gc().await;
        validate_yield_and_output(request.yield_time, request.max_output_bytes)?;
        let session = self.session(&request.session_id).await?;
        self.wait_for_receipt(
            session,
            request.yield_time,
            request.max_output_bytes,
            cancellation,
            true,
        )
        .await
    }

    pub async fn write(
        &self,
        request: ShellWriteRequest,
        cancellation: &dyn CancellationPort,
    ) -> Result<ShellReceipt, ShellManagerError> {
        self.gc().await;
        validate_write_request(&request)?;
        if cancellation.is_cancelled() {
            return Err(ShellManagerError::Cancelled);
        }
        let session = {
            let registry = self.inner.lock().await;
            if !registry.accepting {
                return Err(ShellManagerError::Disabled);
            }
            registry
                .sessions
                .get(&request.session_id)
                .cloned()
                .ok_or(ShellManagerError::SessionNotFound)?
        };
        self.refresh_session(&session).await?;
        let writer = {
            let session = session.lock().map_err(|_| ShellManagerError::Io)?;
            if session.state != ShellSessionState::Running || session.stopping {
                None
            } else {
                Some(session.writer.clone().ok_or(ShellManagerError::Io)?)
            }
        };
        let Some(writer) = writer else {
            self.ensure_cleanup(&session).await?;
            return Err(ShellManagerError::SessionNotFound);
        };

        let mut bytes = request.input.into_bytes();
        if request.submit {
            #[cfg(target_os = "windows")]
            bytes.push(b'\r');
            #[cfg(target_os = "linux")]
            bytes.push(b'\n');
        }
        let boundary = Arc::new(AtomicU8::new(WriteBoundary::Pending as u8));
        let operation_boundary = Arc::clone(&boundary);
        let mut operation = tokio::task::spawn_blocking(move || {
            let mut writer = match writer.lock() {
                Ok(writer) => writer,
                Err(_) => {
                    return BlockingWriteResult::Completed(Err(io::Error::other(
                        "writer poisoned",
                    )));
                }
            };
            if operation_boundary
                .compare_exchange(
                    WriteBoundary::Pending as u8,
                    WriteBoundary::Crossing as u8,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_err()
            {
                return BlockingWriteResult::CancelledBeforeWrite;
            }
            if let Err(error) = writer.write_all(&bytes) {
                return BlockingWriteResult::Completed(Err(error));
            }
            operation_boundary.store(WriteBoundary::Committed as u8, Ordering::Release);
            BlockingWriteResult::Completed(writer.flush())
        });

        let write_result = tokio::select! {
            result = &mut operation => Some(result),
            () = cancellation.cancelled() => None,
        };
        match write_result {
            Some(Ok(BlockingWriteResult::Completed(Ok(())))) => {}
            Some(Ok(BlockingWriteResult::CancelledBeforeWrite)) => {
                return Err(ShellManagerError::Cancelled);
            }
            Some(Ok(BlockingWriteResult::Completed(Err(_)))) | Some(Err(_)) => {
                return Err(
                    if boundary.load(Ordering::Acquire) == WriteBoundary::Pending as u8 {
                        ShellManagerError::Io
                    } else {
                        ShellManagerError::Indeterminate
                    },
                );
            }
            None => {
                let cancelled_before_write = boundary
                    .compare_exchange(
                        WriteBoundary::Pending as u8,
                        WriteBoundary::Cancelled as u8,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    )
                    .is_ok();
                return Err(if cancelled_before_write {
                    ShellManagerError::Cancelled
                } else {
                    ShellManagerError::Indeterminate
                });
            }
        }

        match self
            .wait_for_receipt(
                session,
                request.yield_time,
                request.max_output_bytes,
                cancellation,
                true,
            )
            .await
        {
            Err(ShellManagerError::Cancelled) => Err(ShellManagerError::Indeterminate),
            result => result,
        }
    }

    pub async fn stop(
        &self,
        session_id: &ShellSessionId,
        max_output_bytes: usize,
    ) -> Result<ShellReceipt, ShellManagerError> {
        self.gc().await;
        validate_yield_and_output(Duration::ZERO, max_output_bytes)?;
        let session = self.session(session_id).await?;
        self.stop_entry(session, max_output_bytes).await
    }

    pub async fn disable_and_stop_all(&self) -> Result<(), ShellCleanupError> {
        let _lifecycle = self.lifecycle.lock().await;
        self.disable_and_stop_all_locked().await
    }

    async fn disable_and_stop_all_locked(&self) -> Result<(), ShellCleanupError> {
        self.gc().await;
        loop {
            let settled = self.slots.changed.notified();
            tokio::pin!(settled);
            settled.as_mut().enable();
            let has_starting_sessions = {
                let mut registry = self.inner.lock().await;
                registry.accepting = false;
                self.slots.starting.load(Ordering::Acquire) > 0
            };
            if !has_starting_sessions {
                break;
            }
            settled.as_mut().await;
        }
        let mut sessions = {
            let registry = self.inner.lock().await;
            registry
                .sessions
                .iter()
                .filter_map(|(id, session)| {
                    let include = session
                        .lock()
                        .is_ok_and(|session| !session.cleanup_succeeded);
                    include.then(|| (id.clone(), Arc::clone(session)))
                })
                .collect::<Vec<_>>()
        };
        sessions.extend(
            self.unpublished_sessions
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .iter()
                .map(|(id, session)| (id.clone(), Arc::clone(session))),
        );

        let mut failed = Vec::new();
        for (session_id, session) in sessions {
            if self
                .stop_entry(session, MAX_SHELL_OUTPUT_BYTES)
                .await
                .is_err()
            {
                failed.push(session_id);
            }
        }
        self.unpublished_sessions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .retain(|_, session| {
                !session
                    .lock()
                    .is_ok_and(|session| session.cleanup_succeeded)
            });
        if failed.is_empty() {
            Ok(())
        } else {
            Err(ShellCleanupError {
                session_ids: failed,
            })
        }
    }

    pub async fn shutdown(&self) -> Result<(), ShellCleanupError> {
        self.disable_and_stop_all().await
    }

    async fn reserve_running_slot(
        &self,
    ) -> Result<(StartingSlotLease, RunningSlotLease), ShellManagerError> {
        let registry = self.inner.lock().await;
        if !registry.accepting {
            return Err(ShellManagerError::Disabled);
        }
        self.slots.reserve()
    }

    fn next_unpublished_id(&self) -> ShellSessionId {
        let sequence = self.unpublished_sequence.fetch_add(1, Ordering::AcqRel) + 1;
        ShellSessionId::new(format!("shell-unpublished-{sequence:04}"))
            .expect("generated unpublished shell identifier is valid")
    }

    fn own_spawned_resources(
        &self,
        spawned: SpawnedShell,
        io_mode: ShellIoMode,
    ) -> Result<OwnedSessionResources, OwnedSessionResources> {
        let SpawnedShell {
            child,
            mut reader,
            writer,
            guard,
        } = spawned;
        let requires_cursor_handshake = matches!(io_mode, ShellIoMode::Terminal { .. })
            && self.backend.requires_startup_cursor_handshake();
        let process_id = child.process_id();
        let child = Arc::new(StdMutex::new(child));
        let writer = Arc::new(StdMutex::new(writer));
        let output = Arc::new(StdMutex::new(ShellOutputBuffer::new(Arc::clone(
            &self.output_budget,
        ))));
        let reader_output = Arc::clone(&output);
        let mut handshake_writer = requires_cursor_handshake.then(|| Arc::clone(&writer));
        let (handshake_sender, handshake_receiver) = if requires_cursor_handshake {
            let (sender, receiver) = mpsc::sync_channel(1);
            (Some(sender), Some(receiver))
        } else {
            (None, None)
        };
        let reader_done = Arc::new(ReaderCompletion::default());
        let reader_completion = Arc::clone(&reader_done);
        let reader = self.reader_spawner.spawn(
            format!("shell-reader-{process_id}"),
            Box::new(move || {
                let mut chunk = [0_u8; READER_CHUNK_BYTES];
                let mut handshake = requires_cursor_handshake.then(StartupCursorHandshake::default);
                let mut handshake_sender = handshake_sender;
                loop {
                    match reader.read(&mut chunk) {
                        Ok(0) => {
                            report_startup_handshake_failure(&mut handshake_sender);
                            handshake_writer.take();
                            if let Ok(mut output) = reader_output.lock() {
                                output.finish();
                            }
                            break;
                        }
                        Ok(read) => {
                            let handshake_observed = handshake
                                .as_mut()
                                .is_some_and(|handshake| handshake.observe(&chunk[..read]));
                            let handshake_result = handshake_observed.then(|| {
                                handshake.take();
                                let result = handshake_writer
                                    .take()
                                    .ok_or_else(|| io::Error::other("PTY writer missing"))
                                    .and_then(|writer| {
                                        let mut writer = writer
                                            .lock()
                                            .map_err(|_| io::Error::other("PTY writer poisoned"))?;
                                        writer.write_all(b"\x1b[1;1R")?;
                                        writer.flush()
                                    });
                                if let Some(sender) = handshake_sender.take() {
                                    let _ = sender.send(if result.is_ok() {
                                        StartupHandshakeOutcome::Complete
                                    } else {
                                        StartupHandshakeOutcome::Failed
                                    });
                                }
                                result
                            });
                            let handshake_failed =
                                handshake_result.as_ref().is_some_and(Result::is_err);
                            if let Ok(mut output) = reader_output.lock() {
                                output.append(&chunk[..read]);
                                if handshake_failed {
                                    output.finish();
                                }
                            } else {
                                break;
                            }
                            if handshake_failed {
                                break;
                            }
                        }
                        Err(_) => {
                            report_startup_handshake_failure(&mut handshake_sender);
                            handshake_writer.take();
                            if let Ok(mut output) = reader_output.lock() {
                                output.finish();
                            }
                            break;
                        }
                    }
                }
                reader_completion.finish();
            }),
        );
        let resources = |reader, reader_done| OwnedSessionResources {
            child: Arc::clone(&child),
            writer: Arc::clone(&writer),
            output: Arc::clone(&output),
            startup_handshake: handshake_receiver,
            reader,
            reader_done,
            guard: None,
        };
        match reader {
            Ok(reader) => Ok(OwnedSessionResources {
                guard: Some(guard),
                ..resources(Some(reader), Some(reader_done))
            }),
            Err(_) => Err(OwnedSessionResources {
                guard: Some(guard),
                ..resources(None, None)
            }),
        }
    }

    async fn wait_for_startup_cursor_handshake(
        &self,
        resources: &mut OwnedSessionResources,
        cancellation: &dyn CancellationPort,
    ) -> Result<(), ShellManagerError> {
        let Some(receiver) = resources.startup_handshake.take() else {
            return Ok(());
        };
        let deadline = tokio::time::Instant::now() + STARTUP_CURSOR_HANDSHAKE_WAIT;
        loop {
            match receiver.try_recv() {
                Ok(StartupHandshakeOutcome::Complete) => return Ok(()),
                Ok(StartupHandshakeOutcome::Failed) | Err(mpsc::TryRecvError::Disconnected) => {
                    return Err(ShellManagerError::Io);
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
            if cancellation.is_cancelled() {
                return Err(ShellManagerError::Cancelled);
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(ShellManagerError::Io);
            }
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            tokio::select! {
                () = cancellation.cancelled() => return Err(ShellManagerError::Cancelled),
                () = tokio::time::sleep(PROCESS_POLL_INTERVAL.min(remaining)) => {}
            }
        }
    }

    async fn session(
        &self,
        session_id: &ShellSessionId,
    ) -> Result<Arc<StdMutex<ShellSession>>, ShellManagerError> {
        self.inner
            .lock()
            .await
            .sessions
            .get(session_id)
            .cloned()
            .ok_or(ShellManagerError::SessionNotFound)
    }

    async fn wait_for_receipt(
        &self,
        session: Arc<StdMutex<ShellSession>>,
        yield_time: Duration,
        max_output_bytes: usize,
        cancellation: &dyn CancellationPort,
        return_on_output: bool,
    ) -> Result<ShellReceipt, ShellManagerError> {
        let deadline = tokio::time::Instant::now() + yield_time;
        loop {
            if cancellation.is_cancelled() {
                return Err(ShellManagerError::Cancelled);
            }
            if self.refresh_session(&session).await.is_err() {
                self.ensure_cleanup(&session).await?;
                return self.receipt(&session, max_output_bytes);
            }
            let (terminal, has_output) = {
                let session = session.lock().map_err(|_| ShellManagerError::Io)?;
                let has_output = session
                    .output
                    .lock()
                    .map_err(|_| ShellManagerError::Io)?
                    .unread_bytes()
                    > 0;
                (session.state != ShellSessionState::Running, has_output)
            };
            if terminal {
                self.ensure_cleanup(&session).await?;
                return self.receipt(&session, max_output_bytes);
            }
            if (return_on_output && has_output) || tokio::time::Instant::now() >= deadline {
                return self.receipt(&session, max_output_bytes);
            }

            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            let sleep_for = PROCESS_POLL_INTERVAL.min(remaining);
            tokio::select! {
                () = cancellation.cancelled() => return Err(ShellManagerError::Cancelled),
                () = tokio::time::sleep(sleep_for) => {}
            }
        }
    }

    async fn refresh_session(
        &self,
        session: &Arc<StdMutex<ShellSession>>,
    ) -> Result<bool, ShellManagerError> {
        let became_terminal = {
            let mut session = session.lock().map_err(|_| ShellManagerError::Io)?;
            if session.state != ShellSessionState::Running {
                false
            } else {
                let wait = session
                    .child
                    .lock()
                    .map_err(|_| ShellManagerError::Io)?
                    .try_wait();
                match wait {
                    Ok(Some(exit_code)) => {
                        session.exit_code = Some(exit_code);
                        session.state = if session.stopping {
                            ShellSessionState::Stopped
                        } else {
                            ShellSessionState::Exited
                        };
                        session.terminal_at_unix_ms = Some(self.clock.now_unix_ms());
                        true
                    }
                    Ok(None) => false,
                    Err(_) => return Err(ShellManagerError::Io),
                }
            }
        };
        if became_terminal {
            let sequence = {
                let mut registry = self.inner.lock().await;
                registry.terminal_sequence = registry.terminal_sequence.saturating_add(1);
                registry.terminal_sequence
            };
            if let Ok(mut session) = session.lock() {
                session.terminal_sequence = Some(sequence);
            }
        }
        Ok(became_terminal)
    }

    async fn stop_entry(
        &self,
        session: Arc<StdMutex<ShellSession>>,
        max_output_bytes: usize,
    ) -> Result<ShellReceipt, ShellManagerError> {
        self.ensure_cleanup(&session).await?;
        self.receipt(&session, max_output_bytes)
    }

    async fn ensure_cleanup(
        &self,
        session: &Arc<StdMutex<ShellSession>>,
    ) -> Result<(), ShellManagerError> {
        let (attempt, owner, running) = {
            let mut session = session.lock().map_err(|_| ShellManagerError::Io)?;
            if session.cleanup_succeeded {
                return Ok(());
            }
            if let Some(attempt) = session.cleanup_attempt.as_ref()
                && attempt.result().is_none()
            {
                (
                    Arc::clone(attempt),
                    false,
                    session.state == ShellSessionState::Running,
                )
            } else {
                session.cleanup_generation = session.cleanup_generation.saturating_add(1);
                let attempt = Arc::new(CleanupAttempt::new(session.cleanup_generation));
                let running = session.state == ShellSessionState::Running;
                if running {
                    session.stopping = true;
                }
                session.cleanup_attempt = Some(Arc::clone(&attempt));
                (attempt, true, running)
            }
        };
        if owner {
            let manager = self.clone();
            let session = Arc::clone(session);
            let cleanup_attempt = Arc::clone(&attempt);
            drop(tokio::spawn(async move {
                manager.run_cleanup(session, running, cleanup_attempt).await;
            }));
        }
        attempt.wait().await
    }

    async fn run_cleanup(
        &self,
        session: Arc<StdMutex<ShellSession>>,
        running: bool,
        attempt: Arc<CleanupAttempt>,
    ) {
        let result = if running {
            self.cleanup_running_session(&session).await
        } else {
            let destructive_ok = self.terminate_containment(&session).await;
            let reaped = destructive_ok && self.reap_child(&session).await;
            let containment_ok = reaped && self.confirm_containment(&session).await;
            let reader_done = self
                .close_handles_and_join_reader(&session, containment_ok)
                .await;
            if containment_ok && reader_done {
                Ok(())
            } else {
                Err(ShellManagerError::Indeterminate)
            }
        };
        let running_slot = session.lock().ok().and_then(|mut session| {
            debug_assert_eq!(session.cleanup_generation, attempt.generation);
            if result.is_ok() {
                session.cleanup_succeeded = true;
                session.running_slot.take()
            } else {
                None
            }
        });
        drop(running_slot);
        attempt.complete(result);
    }

    async fn cleanup_running_session(
        &self,
        session: &Arc<StdMutex<ShellSession>>,
    ) -> Result<(), ShellManagerError> {
        let _ = self.write_interrupt(session).await;
        let exited_after_interrupt = self.wait_for_exit(session, CLEANUP_WAIT).await?;
        let terminate_ok = self.terminate_containment(session).await;
        if !exited_after_interrupt && self.kill_child(session).await == ChildKillOutcome::TimedOut {
            return Err(ShellManagerError::Indeterminate);
        }
        let confirmed = if exited_after_interrupt {
            true
        } else {
            self.wait_for_exit(session, CLEANUP_WAIT).await?
        };
        let reaped = confirmed && terminate_ok && self.reap_child(session).await;
        let containment_ok = reaped && self.confirm_containment(session).await;
        let reader_done = if confirmed && containment_ok {
            self.close_handles_and_join_reader(session, containment_ok)
                .await
        } else {
            false
        };
        if !containment_ok || !confirmed || !reader_done {
            return Err(ShellManagerError::Indeterminate);
        }
        Ok(())
    }

    async fn terminate_containment(&self, session: &Arc<StdMutex<ShellSession>>) -> bool {
        let mut guard = match session.lock() {
            Ok(mut session) => session.guard.take(),
            Err(_) => return false,
        };
        let Some(mut guard) = guard.take() else {
            return false;
        };
        let terminated = matches!(
            tokio::time::timeout(CLEANUP_WAIT, guard.terminate()).await,
            Ok(Ok(()))
        );
        match session.lock() {
            Ok(mut session) if session.guard.is_none() => {
                session.guard = Some(guard);
                terminated
            }
            _ => false,
        }
    }

    async fn confirm_containment(&self, session: &Arc<StdMutex<ShellSession>>) -> bool {
        let mut guard = match session.lock() {
            Ok(mut session) => session.guard.take(),
            Err(_) => return false,
        };
        let Some(mut guard) = guard.take() else {
            return false;
        };
        let confirmed = matches!(
            tokio::time::timeout(CLEANUP_WAIT, guard.confirm()).await,
            Ok(Ok(()))
        );
        match session.lock() {
            Ok(mut session) if session.guard.is_none() => {
                session.guard = Some(guard);
                confirmed
            }
            _ => false,
        }
    }

    async fn reap_child(&self, session: &Arc<StdMutex<ShellSession>>) -> bool {
        let child = match session.lock() {
            Ok(session) => Arc::clone(&session.child),
            Err(_) => return false,
        };
        matches!(
            tokio::time::timeout(
                CLEANUP_WAIT,
                tokio::task::spawn_blocking(move || {
                    child.lock().map_err(|_| ())?.reap().map_err(|_| ())
                })
            )
            .await,
            Ok(Ok(Ok(())))
        )
    }

    async fn wait_for_exit(
        &self,
        session: &Arc<StdMutex<ShellSession>>,
        timeout: Duration,
    ) -> Result<bool, ShellManagerError> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if self.refresh_session(session).await.is_ok()
                && session.lock().map_err(|_| ShellManagerError::Io)?.state
                    != ShellSessionState::Running
            {
                return Ok(true);
            }
            if tokio::time::Instant::now() >= deadline {
                return Ok(false);
            }
            tokio::time::sleep(PROCESS_POLL_INTERVAL).await;
        }
    }

    async fn write_interrupt(&self, session: &Arc<StdMutex<ShellSession>>) -> bool {
        let writer = session
            .lock()
            .ok()
            .and_then(|session| session.writer.clone());
        let Some(writer) = writer else {
            return false;
        };
        matches!(
            tokio::time::timeout(
                CLEANUP_WAIT,
                tokio::task::spawn_blocking(move || {
                    let mut writer = writer.lock().map_err(|_| ())?;
                    writer.write_all(b"\x03").map_err(|_| ())?;
                    writer.flush().map_err(|_| ())
                })
            )
            .await,
            Ok(Ok(Ok(())))
        )
    }

    async fn kill_child(&self, session: &Arc<StdMutex<ShellSession>>) -> ChildKillOutcome {
        let child = match session.lock() {
            Ok(session) => Arc::clone(&session.child),
            Err(_) => return ChildKillOutcome::Failed,
        };
        let deadline = tokio::time::sleep(CLEANUP_WAIT);
        tokio::pin!(deadline);
        let mut operation = tokio::task::spawn_blocking(move || {
            child.lock().map_err(|_| ())?.kill().map_err(|_| ())
        });
        tokio::select! {
            result = &mut operation => match result {
                Ok(Ok(())) => ChildKillOutcome::Completed,
                Ok(Err(())) | Err(_) => ChildKillOutcome::Failed,
            },
            () = deadline.as_mut() => ChildKillOutcome::TimedOut,
        }
    }

    async fn close_handles_and_join_reader(
        &self,
        session: &Arc<StdMutex<ShellSession>>,
        tree_cleanup_confirmed: bool,
    ) -> bool {
        let deadline = Instant::now() + CLEANUP_WAIT;
        let (writer, mut guard) = match session.lock() {
            Ok(mut session) => (session.writer.take(), session.guard.take()),
            Err(_) => return false,
        };
        drop(writer);
        if let Some(io_guard) = guard.as_mut()
            && io_guard.close_io(deadline).is_err()
        {
            if let Ok(mut session) = session.lock() {
                session.guard = guard.take();
            }
            return false;
        }
        if let Ok(mut session) = session.lock() {
            session.guard = guard.take();
        } else {
            return false;
        }

        let completion = match session.lock() {
            Ok(session) => match &session.reader {
                ReaderState::Pending { completion, .. } => Some(Arc::clone(completion)),
                ReaderState::Complete => None,
                ReaderState::Failed => return false,
            },
            Err(_) => return false,
        };
        if let Some(completion) = completion
            && !wait_for_reader_completion(
                &completion,
                deadline.saturating_duration_since(Instant::now()),
            )
            .await
        {
            return false;
        }

        let handle = match session.lock() {
            Ok(mut session) => match std::mem::replace(&mut session.reader, ReaderState::Failed) {
                ReaderState::Pending { handle, .. } => Some(handle),
                ReaderState::Complete => {
                    session.reader = ReaderState::Complete;
                    None
                }
                ReaderState::Failed => return false,
            },
            Err(_) => return false,
        };
        let reader_closed = match handle {
            Some(handle) => matches!(
                tokio::time::timeout(
                    deadline.saturating_duration_since(Instant::now()),
                    tokio::task::spawn_blocking(move || handle.join().is_ok()),
                )
                .await,
                Ok(Ok(true))
            ),
            None => true,
        };
        if let Ok(mut session) = session.lock() {
            session.reader = if reader_closed {
                ReaderState::Complete
            } else {
                ReaderState::Failed
            };
        } else {
            return false;
        }

        let completed = tree_cleanup_confirmed && reader_closed;
        if completed {
            let mut guard = match session.lock() {
                Ok(mut session) => session.guard.take(),
                Err(_) => return false,
            };
            if let Some(guard) = guard.as_mut() {
                guard.disarm();
            }
            drop(guard);
        }
        completed
    }

    fn receipt(
        &self,
        session: &Arc<StdMutex<ShellSession>>,
        max_output_bytes: usize,
    ) -> Result<ShellReceipt, ShellManagerError> {
        let session = session.lock().map_err(|_| ShellManagerError::Io)?;
        let mut output = session.output.lock().map_err(|_| ShellManagerError::Io)?;
        let requested = max_output_bytes.min(MAX_SHELL_OUTPUT_BYTES);
        let mut low = 0;
        let mut high = requested.min(output.unread_bytes());
        let mut safe_limit = 0;
        while low <= high {
            let candidate_limit = low + (high - low) / 2;
            let candidate_output = output.peek(candidate_limit);
            let candidate = ShellReceipt {
                session_id: session.id.clone(),
                state: session.state,
                exit_code: session.exit_code,
                output: candidate_output.output,
                output_truncated: candidate_output.truncated,
            };
            let serialized = serde_json::to_vec(&candidate).map_err(|_| ShellManagerError::Io)?;
            if serialized.len() <= MAX_TOOL_RESULT_BYTES {
                safe_limit = candidate_limit;
                low = candidate_limit.saturating_add(1);
            } else if candidate_limit == 0 {
                break;
            } else {
                high = candidate_limit - 1;
            }
        }
        let output = output.take(safe_limit);
        Ok(ShellReceipt {
            session_id: session.id.clone(),
            state: session.state,
            exit_code: session.exit_code,
            output: output.output,
            output_truncated: output.truncated,
        })
    }

    fn settle_unpublished_start(
        &self,
        settlement: PendingSettlement,
    ) -> impl std::future::Future<Output = Result<(), ShellManagerError>> + Send + 'static {
        let registered = self.register_unpublished_start(settlement);
        let manager = self.clone();
        async move {
            let Some((internal_id, session)) = registered else {
                return Ok(());
            };
            tokio::spawn(async move {
                manager
                    .finish_registered_unpublished_start(internal_id, session)
                    .await
            })
            .await
            .unwrap_or(Err(ShellManagerError::Indeterminate))
        }
    }

    fn register_unpublished_start(
        &self,
        mut settlement: PendingSettlement,
    ) -> Option<(ShellSessionId, Arc<StdMutex<ShellSession>>)> {
        let Some(session) = settlement.session.take() else {
            drop(settlement.running_slot.take());
            drop(settlement.starting_slot.take());
            return None;
        };
        let internal_id = session
            .lock()
            .map(|session| session.id.clone())
            .unwrap_or_else(|_| self.next_unpublished_id());
        self.unpublished_sessions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(internal_id.clone(), Arc::clone(&session));
        drop(settlement.running_slot.take());
        drop(settlement.starting_slot.take());
        Some((internal_id, session))
    }

    async fn finish_registered_unpublished_start(
        &self,
        internal_id: ShellSessionId,
        session: Arc<StdMutex<ShellSession>>,
    ) -> Result<(), ShellManagerError> {
        let cleanup = self.ensure_cleanup(&session).await;
        #[cfg(test)]
        self.wait_at_unpublished_registration_gate().await;
        if cleanup.is_ok() {
            let mut unpublished = self
                .unpublished_sessions
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if unpublished
                .get(&internal_id)
                .is_some_and(|registered| Arc::ptr_eq(registered, &session))
            {
                unpublished.remove(&internal_id);
            }
        }
        cleanup
    }

    #[cfg(test)]
    async fn wait_at_unpublished_registration_gate(&self) {
        let gate = self
            .unpublished_registration_gate
            .lock()
            .ok()
            .and_then(|gate| gate.clone());
        if let Some(gate) = gate {
            gate.entered.wait().await;
            gate.release.wait().await;
        }
    }

    async fn reap_newly_terminal_sessions(&self) {
        let sessions = {
            let registry = self.inner.lock().await;
            registry.sessions.values().cloned().collect::<Vec<_>>()
        };
        for session in sessions {
            if self.refresh_session(&session).await == Ok(true) {
                let _ = self.ensure_cleanup(&session).await;
            }
        }
    }

    async fn gc(&self) {
        let now = self.clock.now_unix_ms();
        let ttl_ms = u64::try_from(TERMINAL_RECEIPT_TTL.as_millis()).unwrap_or(u64::MAX);
        let mut registry = self.inner.lock().await;
        let mut terminal = registry
            .sessions
            .iter()
            .filter_map(|(id, session)| {
                let session = session.lock().ok()?;
                let terminal_at = session.terminal_at_unix_ms?;
                let sequence = session.terminal_sequence?;
                (session.state != ShellSessionState::Running && session.cleanup_succeeded)
                    .then(|| (id.clone(), terminal_at, sequence))
            })
            .collect::<Vec<_>>();
        let expired = terminal
            .iter()
            .filter(|(_, terminal_at, _)| now.saturating_sub(*terminal_at) >= ttl_ms)
            .map(|(id, _, _)| id.clone())
            .collect::<Vec<_>>();
        for id in expired {
            registry.sessions.remove(&id);
        }
        terminal.retain(|(id, _, _)| registry.sessions.contains_key(id));
        terminal.sort_by_key(|(_, terminal_at, sequence)| (*terminal_at, *sequence));
        let excess = terminal.len().saturating_sub(MAX_TERMINAL_SHELL_RECEIPTS);
        for (id, _, _) in terminal.into_iter().take(excess) {
            registry.sessions.remove(&id);
        }
    }
}

impl Drop for ShellSessionManager {
    fn drop(&mut self) {
        if Arc::strong_count(&self.inner) != 1 {
            return;
        }
        let mut sessions = match self.inner.try_lock() {
            Ok(mut registry) => {
                registry.accepting = false;
                registry.sessions.values().cloned().collect::<Vec<_>>()
            }
            Err(_) => return,
        };
        sessions.extend(
            self.unpublished_sessions
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .values()
                .cloned(),
        );
        for session in sessions {
            let (guard, writer, child) = match session.try_lock() {
                Ok(mut session) if session.state == ShellSessionState::Running => {
                    session.stopping = true;
                    (
                        session.guard.take(),
                        session.writer.take(),
                        Arc::clone(&session.child),
                    )
                }
                _ => continue,
            };
            drop(guard);
            if let Some(writer) = writer
                && let Ok(mut writer) = writer.try_lock()
            {
                let _ = writer.write_all(b"\x03");
                let _ = writer.flush();
            }
            if let Ok(mut child) = child.try_lock() {
                let _ = child.kill();
            }
        }
    }
}

async fn wait_for_reader_completion(completion: &Arc<ReaderCompletion>, timeout: Duration) -> bool {
    let completion = Arc::clone(completion);
    matches!(
        tokio::task::spawn_blocking(move || completion.wait_timeout(timeout)).await,
        Ok(true)
    )
}

struct OwnedSessionResources {
    guard: Option<Box<dyn ShellGuard>>,
    child: Arc<StdMutex<Box<dyn ShellChild>>>,
    writer: Arc<StdMutex<Box<dyn Write + Send>>>,
    output: Arc<StdMutex<ShellOutputBuffer>>,
    startup_handshake: Option<mpsc::Receiver<StartupHandshakeOutcome>>,
    reader: Option<thread::JoinHandle<()>>,
    reader_done: Option<Arc<ReaderCompletion>>,
}

enum PendingStartOwnership {
    Reservation,
    Resources(OwnedSessionResources),
    Session(Arc<StdMutex<ShellSession>>),
}

struct PendingStart {
    manager: Option<ShellSessionManager>,
    starting_slot: Option<StartingSlotLease>,
    running_slot: Option<RunningSlotLease>,
    ownership: Option<PendingStartOwnership>,
}

impl PendingStart {
    fn reserved(
        manager: ShellSessionManager,
        starting_slot: StartingSlotLease,
        running_slot: RunningSlotLease,
    ) -> Self {
        Self {
            manager: Some(manager),
            starting_slot: Some(starting_slot),
            running_slot: Some(running_slot),
            ownership: Some(PendingStartOwnership::Reservation),
        }
    }

    fn own_resources(&mut self, resources: OwnedSessionResources) {
        self.ownership = Some(PendingStartOwnership::Resources(resources));
    }

    fn resources_mut(&mut self) -> &mut OwnedSessionResources {
        match self.ownership.as_mut() {
            Some(PendingStartOwnership::Resources(resources)) => resources,
            _ => unreachable!("pending start owns resources before startup handshake"),
        }
    }

    fn take_resources(&mut self) -> OwnedSessionResources {
        match self.ownership.take() {
            Some(PendingStartOwnership::Resources(resources)) => resources,
            _ => unreachable!("pending start owns resources before session construction"),
        }
    }

    fn take_running_slot(&mut self) -> RunningSlotLease {
        self.running_slot
            .take()
            .expect("pending start owns its running slot before session construction")
    }

    fn own_session(&mut self, session: Arc<StdMutex<ShellSession>>) {
        self.ownership = Some(PendingStartOwnership::Session(session));
    }

    fn take_settlement(&mut self) -> (ShellSessionManager, PendingSettlement) {
        let manager = self
            .manager
            .take()
            .expect("armed pending start owns its manager");
        let session = match self.ownership.take() {
            Some(PendingStartOwnership::Reservation) | None => None,
            Some(PendingStartOwnership::Resources(resources)) => {
                Some(Arc::new(StdMutex::new(resources.into_unpublished_session(
                    manager.next_unpublished_id(),
                    self.take_running_slot(),
                ))))
            }
            Some(PendingStartOwnership::Session(session)) => {
                if let Ok(mut session_state) = session.lock() {
                    session_state.id = manager.next_unpublished_id();
                }
                Some(session)
            }
        };
        (
            manager,
            PendingSettlement {
                starting_slot: self.starting_slot.take(),
                running_slot: self.running_slot.take(),
                session,
            },
        )
    }

    fn disarm(&mut self) {
        drop(self.starting_slot.take());
        debug_assert!(self.running_slot.is_none());
        self.manager.take();
        self.ownership.take();
    }
}

struct PendingSettlement {
    starting_slot: Option<StartingSlotLease>,
    running_slot: Option<RunningSlotLease>,
    session: Option<Arc<StdMutex<ShellSession>>>,
}

impl Drop for PendingStart {
    fn drop(&mut self) {
        if self.manager.is_none() {
            return;
        }
        let (manager, settlement) = self.take_settlement();
        if let Some((internal_id, session)) = manager.register_unpublished_start(settlement) {
            settle_unpublished_start_on_independent_executor(manager, internal_id, session);
        }
    }
}

fn settle_unpublished_start_on_independent_executor(
    manager: ShellSessionManager,
    internal_id: ShellSessionId,
    session: Arc<StdMutex<ShellSession>>,
) {
    let work = Arc::new(StdMutex::new(Some((manager, internal_id, session))));
    let worker_work = Arc::clone(&work);
    let _ = thread::Builder::new()
        .name("shell-unpublished-cleanup".to_owned())
        .spawn(move || {
            let Some((manager, internal_id, session)) =
                worker_work.lock().ok().and_then(|mut work| work.take())
            else {
                return;
            };
            let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                return;
            };
            let _ =
                runtime.block_on(manager.finish_registered_unpublished_start(internal_id, session));
            runtime.shutdown_timeout(CLEANUP_WAIT);
        });
}

impl OwnedSessionResources {
    fn into_session(self, id: ShellSessionId, running_slot: RunningSlotLease) -> ShellSession {
        let reader = match (self.reader, self.reader_done) {
            (Some(handle), Some(completion)) => ReaderState::Pending { handle, completion },
            (None, None) => ReaderState::Complete,
            _ => ReaderState::Failed,
        };
        ShellSession {
            id,
            guard: self.guard,
            running_slot: Some(running_slot),
            child: self.child,
            writer: Some(self.writer),
            output: self.output,
            reader,
            state: ShellSessionState::Running,
            stopping: false,
            cleanup_generation: 0,
            cleanup_attempt: None,
            cleanup_succeeded: false,
            exit_code: None,
            terminal_at_unix_ms: None,
            terminal_sequence: None,
        }
    }

    fn into_unpublished_session(
        self,
        id: ShellSessionId,
        running_slot: RunningSlotLease,
    ) -> ShellSession {
        self.into_session(id, running_slot)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ChildKillOutcome {
    Completed,
    Failed,
    TimedOut,
}

enum BlockingWriteResult {
    CancelledBeforeWrite,
    Completed(io::Result<()>),
}

#[repr(u8)]
enum WriteBoundary {
    Pending = 0,
    Crossing = 1,
    Committed = 2,
    Cancelled = 3,
}

fn validate_command_request(request: &ShellCommandRequest) -> Result<(), ShellManagerError> {
    if request.command.trim().is_empty()
        || request.command.len() > MAX_SHELL_COMMAND_BYTES
        || request.cwd.as_os_str().to_string_lossy().len() > MAX_SHELL_CWD_BYTES
        || !request.cwd.is_dir()
    {
        return Err(ShellManagerError::InvalidArguments);
    }
    validate_yield_and_output(request.yield_time, request.max_output_bytes)
}

#[derive(Default)]
struct StartupCursorHandshake {
    matched: usize,
}

impl StartupCursorHandshake {
    fn observe(&mut self, chunk: &[u8]) -> bool {
        const CURSOR_QUERY: &[u8] = b"\x1b[6n";
        for &byte in chunk {
            if CURSOR_QUERY.get(self.matched).copied() == Some(byte) {
                self.matched += 1;
                if self.matched == CURSOR_QUERY.len() {
                    self.matched = 0;
                    return true;
                }
            } else {
                self.matched = usize::from(byte == CURSOR_QUERY[0]);
            }
        }
        false
    }
}

#[derive(Clone, Copy)]
enum StartupHandshakeOutcome {
    Complete,
    Failed,
}

fn report_startup_handshake_failure(
    sender: &mut Option<mpsc::SyncSender<StartupHandshakeOutcome>>,
) {
    if let Some(sender) = sender.take() {
        let _ = sender.send(StartupHandshakeOutcome::Failed);
    }
}

fn validate_write_request(request: &ShellWriteRequest) -> Result<(), ShellManagerError> {
    if (request.input.is_empty() && !request.submit) || request.input.len() > MAX_SHELL_INPUT_BYTES
    {
        return Err(ShellManagerError::InvalidArguments);
    }
    validate_yield_and_output(request.yield_time, request.max_output_bytes)
}

#[cfg(test)]
mod tests {
    use std::io::{self, Read, Write};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Condvar, Mutex};
    use std::time::{Duration, Instant};

    use minimax_protocol::{MAX_SHELL_OUTPUT_BYTES, ShellSessionId};

    use super::{
        CleanupAttempt, MAX_RUNNING_SHELL_SESSIONS, PendingStart, ShellCleanupError,
        ShellCommandRequest, ShellManagerError, ShellSessionManager, UnpublishedRegistrationGate,
    };
    use crate::NeverCancelled;
    use crate::shell::{
        ShellBackend, ShellChild, ShellIoMode, ShellSessionIdSource, ShellSpawnError,
        ShellSpawnRequest, ShellTerminateFuture, SpawnedShell, SystemShellClock,
    };

    #[derive(Default)]
    struct PublishLockBackend {
        process: Arc<PublishLockProcess>,
    }

    #[derive(Default)]
    struct PublishLockProcess {
        running: AtomicBool,
        reader: Mutex<Option<std::sync::mpsc::Sender<()>>>,
        guard_terminations: std::sync::atomic::AtomicUsize,
    }

    struct RecoverableSpawnFailureBackend {
        process: Arc<PublishLockProcess>,
        fail_cleanup: Arc<AtomicBool>,
    }

    impl ShellBackend for RecoverableSpawnFailureBackend {
        fn spawn(&self, _request: &ShellSpawnRequest) -> Result<SpawnedShell, ShellSpawnError> {
            self.process.running.store(true, Ordering::Release);
            Err(ShellSpawnError::with_cleanup(
                io::Error::other("scripted recoverable spawn failure"),
                SpawnedShell {
                    child: Box::new(PublishLockChild {
                        process: Arc::clone(&self.process),
                    }),
                    reader: Box::new(io::empty()),
                    writer: Box::new(SilentWriter),
                    guard: Box::new(RecoveringContainmentGuard {
                        process: Arc::clone(&self.process),
                        fail: Arc::clone(&self.fail_cleanup),
                    }),
                },
            ))
        }
    }

    impl PublishLockProcess {
        fn exit(&self) {
            self.running.store(false, Ordering::Release);
            if let Some(sender) = self.reader.lock().expect("reader sender").take() {
                let _ = sender.send(());
            }
        }
    }

    impl ShellBackend for PublishLockBackend {
        fn spawn(&self, _request: &ShellSpawnRequest) -> crate::shell::ShellSpawnResult {
            self.process.running.store(true, Ordering::Release);
            let (sender, receiver) = std::sync::mpsc::channel();
            *self.process.reader.lock().expect("reader sender") = Some(sender);
            Ok(SpawnedShell {
                child: Box::new(PublishLockChild {
                    process: Arc::clone(&self.process),
                }),
                reader: Box::new(PublishLockReader { receiver }),
                writer: Box::new(PublishLockWriter {
                    process: Arc::clone(&self.process),
                }),
                guard: Box::new(PublishLockGuard {
                    process: Arc::clone(&self.process),
                    armed: true,
                }),
            })
        }
    }

    struct PublishLockChild {
        process: Arc<PublishLockProcess>,
    }

    struct PublishLockGuard {
        process: Arc<PublishLockProcess>,
        armed: bool,
    }

    struct NoRuntimeOrderGuard {
        process: Arc<PublishLockProcess>,
        slots: Arc<super::SlotLedger>,
        drops: Arc<std::sync::atomic::AtomicUsize>,
        early_running_release: Arc<AtomicBool>,
    }

    struct BlockingNoRuntimeGuard {
        process: Arc<PublishLockProcess>,
        slots: Arc<super::SlotLedger>,
        entered: Arc<(Mutex<bool>, Condvar)>,
        release: Arc<(Mutex<bool>, Condvar)>,
        early_running_release: Arc<AtomicBool>,
    }

    struct BlockingKillChild {
        process: Arc<PublishLockProcess>,
        entered: Arc<(Mutex<bool>, Condvar)>,
        release: Arc<(Mutex<bool>, Condvar)>,
    }

    struct SilentWriter;

    struct PassiveGuard;

    struct FailingContainmentGuard;

    struct RecoveringContainmentGuard {
        process: Arc<PublishLockProcess>,
        fail: Arc<AtomicBool>,
    }

    impl crate::shell::ShellGuard for FailingContainmentGuard {
        fn terminate<'a>(&'a mut self) -> ShellTerminateFuture<'a> {
            Box::pin(async { Err(io::Error::other("scripted containment failure")) })
        }

        fn confirm<'a>(&'a mut self) -> ShellTerminateFuture<'a> {
            Box::pin(async { Ok(()) })
        }
    }

    impl crate::shell::ShellGuard for RecoveringContainmentGuard {
        fn terminate<'a>(&'a mut self) -> ShellTerminateFuture<'a> {
            Box::pin(async move {
                if self.fail.load(Ordering::Acquire) {
                    Err(io::Error::other("scripted containment failure"))
                } else {
                    self.process.exit();
                    Ok(())
                }
            })
        }

        fn confirm<'a>(&'a mut self) -> ShellTerminateFuture<'a> {
            Box::pin(async { Ok(()) })
        }
    }

    impl crate::shell::ShellGuard for NoRuntimeOrderGuard {
        fn terminate<'a>(&'a mut self) -> ShellTerminateFuture<'a> {
            Box::pin(async { Ok(()) })
        }

        fn confirm<'a>(&'a mut self) -> ShellTerminateFuture<'a> {
            Box::pin(async { Ok(()) })
        }
    }

    impl Drop for NoRuntimeOrderGuard {
        fn drop(&mut self) {
            if self.slots.running.load(Ordering::Acquire) == 0 {
                self.early_running_release.store(true, Ordering::Release);
            }
            self.drops.fetch_add(1, Ordering::AcqRel);
            self.process.exit();
        }
    }

    impl crate::shell::ShellGuard for BlockingNoRuntimeGuard {
        fn terminate<'a>(&'a mut self) -> ShellTerminateFuture<'a> {
            Box::pin(async { Ok(()) })
        }

        fn confirm<'a>(&'a mut self) -> ShellTerminateFuture<'a> {
            Box::pin(async { Ok(()) })
        }
    }

    impl ShellChild for BlockingKillChild {
        fn process_id(&self) -> u32 {
            78
        }

        fn try_wait(&mut self) -> io::Result<Option<i32>> {
            Ok((!self.process.running.load(Ordering::Acquire)).then_some(-2))
        }

        fn kill(&mut self) -> io::Result<()> {
            {
                let (entered, signal) = &*self.entered;
                *entered.lock().expect("blocking kill entered") = true;
                signal.notify_all();
            }
            let (release, signal) = &*self.release;
            let mut release = release.lock().expect("blocking kill release");
            while !*release {
                release = signal.wait(release).expect("blocking kill release wait");
            }
            self.process.exit();
            Ok(())
        }
    }

    impl Write for SilentWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl crate::shell::ShellGuard for PassiveGuard {
        fn terminate<'a>(&'a mut self) -> ShellTerminateFuture<'a> {
            Box::pin(async { Ok(()) })
        }

        fn confirm<'a>(&'a mut self) -> ShellTerminateFuture<'a> {
            Box::pin(async { Ok(()) })
        }
    }

    impl Drop for BlockingNoRuntimeGuard {
        fn drop(&mut self) {
            if self.slots.running.load(Ordering::Acquire) == 0 {
                self.early_running_release.store(true, Ordering::Release);
            }
            {
                let (entered, signal) = &*self.entered;
                *entered.lock().expect("guard entered") = true;
                signal.notify_all();
            }
            let (release, signal) = &*self.release;
            let mut release = release.lock().expect("guard release");
            while !*release {
                release = signal.wait(release).expect("guard release wait");
            }
            self.process.exit();
        }
    }

    impl crate::shell::ShellGuard for PublishLockGuard {
        fn terminate<'a>(&'a mut self) -> ShellTerminateFuture<'a> {
            Box::pin(async move {
                self.process.exit();
                Ok(())
            })
        }

        fn confirm<'a>(&'a mut self) -> ShellTerminateFuture<'a> {
            Box::pin(async { Ok(()) })
        }

        fn disarm(&mut self) {
            self.armed = false;
        }
    }

    impl Drop for PublishLockGuard {
        fn drop(&mut self) {
            if self.armed {
                self.process
                    .guard_terminations
                    .fetch_add(1, Ordering::AcqRel);
                self.process.exit();
            }
        }
    }

    #[test]
    fn no_runtime_pending_start_drop_releases_eight_slots_after_bounded_guard_cleanup() {
        let backend = Arc::new(PublishLockBackend::default());
        let manager = ShellSessionManager::new(
            backend.clone(),
            Arc::new(FixedIds),
            Arc::new(SystemShellClock),
        );
        let guard_drops = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let early_running_release = Arc::new(AtomicBool::new(false));
        let mut starts = Vec::new();
        for index in 0..MAX_RUNNING_SHELL_SESSIONS {
            let (starting_slot, running_slot) = manager.slots.reserve().expect("slot reservation");
            let mut pending = PendingStart::reserved(manager.clone(), starting_slot, running_slot);
            let mut spawned = backend
                .spawn(&ShellSpawnRequest {
                    command: format!("no-runtime-{index}"),
                    cwd: std::path::PathBuf::from("."),
                    io_mode: ShellIoMode::Pipe,
                })
                .expect("no-runtime process");
            spawned.guard = Box::new(NoRuntimeOrderGuard {
                process: Arc::clone(&backend.process),
                slots: Arc::clone(&manager.slots),
                drops: Arc::clone(&guard_drops),
                early_running_release: Arc::clone(&early_running_release),
            });
            let resources = manager
                .own_spawned_resources(spawned, ShellIoMode::Pipe)
                .unwrap_or_else(|_| panic!("no-runtime resources"));
            pending.own_resources(resources);
            starts.push(pending);
        }

        let started = Instant::now();
        drop(starts);

        assert!(started.elapsed() < Duration::from_secs(2));
        assert_eq!(manager.slots.starting.load(Ordering::Acquire), 0);
        let deadline = Instant::now() + Duration::from_secs(5);
        while (guard_drops.load(Ordering::Acquire) != MAX_RUNNING_SHELL_SESSIONS
            || manager.slots.running.load(Ordering::Acquire) != 0)
            && Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            guard_drops.load(Ordering::Acquire),
            MAX_RUNNING_SHELL_SESSIONS
        );
        assert!(!early_running_release.load(Ordering::Acquire));
        assert_eq!(manager.slots.running.load(Ordering::Acquire), 0);
    }

    #[test]
    fn idle_runtime_pending_start_drop_is_settled_by_an_independent_executor() {
        let backend = Arc::new(PublishLockBackend::default());
        let manager = ShellSessionManager::new(
            backend.clone(),
            Arc::new(FixedIds),
            Arc::new(SystemShellClock),
        );
        let idle_runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("idle runtime");
        idle_runtime.block_on(async {
            manager.enable().await;
            let (starting_slot, running_slot) = manager.slots.reserve().expect("slot reservation");
            let mut pending = PendingStart::reserved(manager.clone(), starting_slot, running_slot);
            let spawned = backend
                .spawn(&ShellSpawnRequest {
                    command: "idle-runtime-drop".to_owned(),
                    cwd: std::path::PathBuf::from("."),
                    io_mode: ShellIoMode::Pipe,
                })
                .expect("idle-runtime process");
            let resources = manager
                .own_spawned_resources(spawned, ShellIoMode::Pipe)
                .unwrap_or_else(|_| panic!("idle-runtime resources"));
            pending.own_resources(resources);
            drop(pending);
        });

        let cleanup_runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("cleanup runtime");
        cleanup_runtime
            .block_on(async {
                tokio::time::timeout(Duration::from_secs(1), manager.disable_and_stop_all()).await
            })
            .expect("cleanup must not depend on polling the first runtime")
            .expect("independent unpublished cleanup succeeds");
        assert_eq!(manager.slots.starting.load(Ordering::Acquire), 0);
        assert_eq!(manager.slots.running.load(Ordering::Acquire), 0);
    }

    #[test]
    fn idle_runtime_cleanup_failure_is_registered_for_a_later_retry() {
        let backend = Arc::new(PublishLockBackend::default());
        let manager = ShellSessionManager::new(
            backend.clone(),
            Arc::new(FixedIds),
            Arc::new(SystemShellClock),
        );
        let fail = Arc::new(AtomicBool::new(true));
        let idle_runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("idle runtime");
        idle_runtime.block_on(async {
            manager.enable().await;
            let (starting_slot, running_slot) = manager.slots.reserve().expect("slot reservation");
            let mut pending = PendingStart::reserved(manager.clone(), starting_slot, running_slot);
            let mut spawned = backend
                .spawn(&ShellSpawnRequest {
                    command: "idle-runtime-failure".to_owned(),
                    cwd: std::path::PathBuf::from("."),
                    io_mode: ShellIoMode::Pipe,
                })
                .expect("idle-runtime process");
            spawned.guard = Box::new(RecoveringContainmentGuard {
                process: Arc::clone(&backend.process),
                fail: Arc::clone(&fail),
            });
            let resources = manager
                .own_spawned_resources(spawned, ShellIoMode::Pipe)
                .unwrap_or_else(|_| panic!("idle-runtime resources"));
            pending.own_resources(resources);
            drop(pending);
        });

        let cleanup_runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("cleanup runtime");
        let first = cleanup_runtime
            .block_on(async {
                tokio::time::timeout(Duration::from_secs(1), manager.disable_and_stop_all()).await
            })
            .expect("failed cleanup must still register outside the idle runtime");
        assert_eq!(
            first,
            Err(ShellCleanupError {
                session_ids: vec![
                    ShellSessionId::new("shell-unpublished-0001")
                        .expect("valid unpublished identifier"),
                ],
            })
        );
        assert_eq!(manager.slots.starting.load(Ordering::Acquire), 0);
        assert_eq!(manager.slots.running.load(Ordering::Acquire), 1);

        fail.store(false, Ordering::Release);
        cleanup_runtime
            .block_on(manager.disable_and_stop_all())
            .expect("later unpublished cleanup retry succeeds");
        assert_eq!(manager.slots.running.load(Ordering::Acquire), 0);
    }

    #[test]
    fn blocked_cleanup_is_registered_before_the_starting_lease_is_released() {
        let backend = Arc::new(PublishLockBackend::default());
        let manager = ShellSessionManager::new(
            backend.clone(),
            Arc::new(FixedIds),
            Arc::new(SystemShellClock),
        );
        let entered = Arc::new((Mutex::new(false), Condvar::new()));
        let release = Arc::new((Mutex::new(false), Condvar::new()));
        let idle_runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("idle runtime");
        idle_runtime.block_on(async {
            manager.enable().await;
            let (starting_slot, running_slot) = manager.slots.reserve().expect("slot reservation");
            let mut pending = PendingStart::reserved(manager.clone(), starting_slot, running_slot);
            let mut spawned = backend
                .spawn(&ShellSpawnRequest {
                    command: "blocked-cleanup".to_owned(),
                    cwd: std::path::PathBuf::from("."),
                    io_mode: ShellIoMode::Pipe,
                })
                .expect("blocked cleanup process");
            spawned.child = Box::new(BlockingKillChild {
                process: Arc::clone(&backend.process),
                entered: Arc::clone(&entered),
                release: Arc::clone(&release),
            });
            spawned.writer = Box::new(SilentWriter);
            spawned.guard.disarm();
            spawned.guard = Box::new(PassiveGuard);
            let resources = manager
                .own_spawned_resources(spawned, ShellIoMode::Pipe)
                .unwrap_or_else(|_| panic!("blocked cleanup resources"));
            pending.own_resources(resources);
            drop(pending);
        });

        let (entered_lock, entered_signal) = &*entered;
        let (entered_lock, _) = entered_signal
            .wait_timeout_while(
                entered_lock.lock().expect("blocking kill entered"),
                Duration::from_secs(5),
                |entered| !*entered,
            )
            .expect("blocking kill entered wait");
        let kill_entered = *entered_lock;
        drop(entered_lock);
        let starting_while_kill_is_blocked = manager.slots.starting.load(Ordering::Acquire);
        let running_while_kill_is_blocked = manager.slots.running.load(Ordering::Acquire);
        {
            let (release, signal) = &*release;
            *release.lock().expect("blocking kill release") = true;
            signal.notify_all();
        }

        assert!(
            kill_entered,
            "cleanup never reached the blocking child kill"
        );
        assert_eq!(
            starting_while_kill_is_blocked, 0,
            "starting lease remained held after fail-closed registration"
        );
        assert_eq!(running_while_kill_is_blocked, 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn disable_waits_for_no_runtime_pending_start_cleanup_to_finish() {
        let backend = Arc::new(PublishLockBackend::default());
        let manager = ShellSessionManager::new(
            backend.clone(),
            Arc::new(FixedIds),
            Arc::new(SystemShellClock),
        );
        manager.enable().await;
        let entered = Arc::new((Mutex::new(false), Condvar::new()));
        let release = Arc::new((Mutex::new(false), Condvar::new()));
        let early_running_release = Arc::new(AtomicBool::new(false));
        let (starting_slot, running_slot) = manager.slots.reserve().expect("slot reservation");
        let mut pending = PendingStart::reserved(manager.clone(), starting_slot, running_slot);
        let mut spawned = backend
            .spawn(&ShellSpawnRequest {
                command: "no-runtime-disable-barrier".to_owned(),
                cwd: std::path::PathBuf::from("."),
                io_mode: ShellIoMode::Pipe,
            })
            .expect("no-runtime process");
        spawned.guard = Box::new(BlockingNoRuntimeGuard {
            process: Arc::clone(&backend.process),
            slots: Arc::clone(&manager.slots),
            entered: Arc::clone(&entered),
            release: Arc::clone(&release),
            early_running_release: Arc::clone(&early_running_release),
        });
        let resources = manager
            .own_spawned_resources(spawned, ShellIoMode::Pipe)
            .unwrap_or_else(|_| panic!("no-runtime resources"));
        pending.own_resources(resources);

        let dropper = std::thread::spawn(move || drop(pending));
        let entered_wait = Arc::clone(&entered);
        tokio::task::spawn_blocking(move || {
            let (entered, signal) = &*entered_wait;
            let mut entered = entered.lock().expect("guard entered");
            while !*entered {
                entered = signal.wait(entered).expect("guard entered wait");
            }
        })
        .await
        .expect("guard entered waiter joins");
        let starting_during_cleanup = manager.slots.starting.load(Ordering::Acquire);
        let running_during_cleanup = manager.slots.running.load(Ordering::Acquire);
        let disable_manager = manager.clone();
        let mut disable = tokio::spawn(async move { disable_manager.disable_and_stop_all().await });
        let early = tokio::time::timeout(Duration::from_millis(50), &mut disable).await;
        let finished_early = early.is_ok();
        {
            let (release, signal) = &*release;
            *release.lock().expect("guard release") = true;
            signal.notify_all();
        }
        tokio::task::spawn_blocking(move || dropper.join().expect("dropper joins"))
            .await
            .expect("dropper join task");
        let disable_result = match early {
            Ok(result) => result.expect("early disable joins"),
            Err(_) => disable.await.expect("waiting disable joins"),
        };

        assert_eq!(starting_during_cleanup, 0);
        assert_eq!(running_during_cleanup, 1);
        assert!(!finished_early, "disable bypassed no-runtime cleanup");
        disable_result.expect("disable succeeds after no-runtime cleanup");
        assert!(!early_running_release.load(Ordering::Acquire));
        assert_eq!(manager.slots.starting.load(Ordering::Acquire), 0);
        assert_eq!(manager.slots.running.load(Ordering::Acquire), 0);
    }

    #[tokio::test]
    async fn cleanup_waiters_keep_their_generation_result_when_the_next_attempt_completes() {
        let first = Arc::new(CleanupAttempt::new(1));
        let first_waiter_a = {
            let attempt = Arc::clone(&first);
            tokio::spawn(async move { attempt.wait().await })
        };
        let first_waiter_b = {
            let attempt = Arc::clone(&first);
            tokio::spawn(async move { attempt.wait().await })
        };
        first.complete(Err(ShellManagerError::Indeterminate));

        let second = Arc::new(CleanupAttempt::new(2));
        let second_waiter = {
            let attempt = Arc::clone(&second);
            tokio::spawn(async move { attempt.wait().await })
        };
        second.complete(Ok(()));

        assert_eq!(
            first_waiter_a.await.expect("first waiter A joins"),
            Err(ShellManagerError::Indeterminate)
        );
        assert_eq!(
            first_waiter_b.await.expect("first waiter B joins"),
            Err(ShellManagerError::Indeterminate)
        );
        assert_eq!(second_waiter.await.expect("second waiter joins"), Ok(()));
        assert_eq!(first.generation, 1);
        assert_eq!(second.generation, 2);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn disable_observes_a_failed_start_registered_before_async_cleanup_finishes() {
        let backend = Arc::new(PublishLockBackend::default());
        let manager = ShellSessionManager::new(
            backend.clone(),
            Arc::new(FixedIds),
            Arc::new(SystemShellClock),
        );
        manager.enable().await;
        let gate = Arc::new(UnpublishedRegistrationGate {
            entered: tokio::sync::Barrier::new(2),
            release: tokio::sync::Barrier::new(2),
        });
        *manager
            .unpublished_registration_gate
            .lock()
            .expect("registration gate lock") = Some(Arc::clone(&gate));

        let (starting_slot, running_slot) = manager.slots.reserve().expect("slot reservation");
        let mut pending = PendingStart::reserved(manager.clone(), starting_slot, running_slot);
        let mut spawned = backend
            .spawn(&ShellSpawnRequest {
                command: "registration-race".to_owned(),
                cwd: std::path::PathBuf::from("."),
                io_mode: ShellIoMode::Pipe,
            })
            .expect("scripted process starts");
        spawned.guard = Box::new(FailingContainmentGuard);
        let resources = manager
            .own_spawned_resources(spawned, ShellIoMode::Pipe)
            .unwrap_or_else(|_| panic!("scripted resources are owned"));
        pending.own_resources(resources);
        let (settlement_manager, settlement) = pending.take_settlement();
        let finish = tokio::spawn(async move {
            settlement_manager
                .settle_unpublished_start(settlement)
                .await
        });
        gate.entered.wait().await;

        let disable_manager = manager.clone();
        let disable = tokio::spawn(async move { disable_manager.disable_and_stop_all().await });
        let disable_result = tokio::time::timeout(Duration::from_secs(1), disable)
            .await
            .expect("disable sees the synchronously registered failed start")
            .expect("disable task joins");
        gate.release.wait().await;
        assert_eq!(
            finish.await.expect("unpublished settlement joins"),
            Err(ShellManagerError::Indeterminate)
        );
        assert_eq!(
            disable_result,
            Err(ShellCleanupError {
                session_ids: vec![
                    ShellSessionId::new("shell-unpublished-0001")
                        .expect("valid unpublished identifier"),
                ],
            })
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn aborted_recoverable_spawn_failure_stays_owned_until_disable_retry() {
        let process = Arc::new(PublishLockProcess::default());
        let fail_cleanup = Arc::new(AtomicBool::new(true));
        let backend = Arc::new(RecoverableSpawnFailureBackend {
            process: Arc::clone(&process),
            fail_cleanup: Arc::clone(&fail_cleanup),
        });
        let manager =
            ShellSessionManager::new(backend, Arc::new(FixedIds), Arc::new(SystemShellClock));
        manager.enable().await;
        let gate = Arc::new(UnpublishedRegistrationGate {
            entered: tokio::sync::Barrier::new(2),
            release: tokio::sync::Barrier::new(2),
        });
        *manager
            .unpublished_registration_gate
            .lock()
            .expect("registration gate lock") = Some(Arc::clone(&gate));

        let start_manager = manager.clone();
        let start = tokio::spawn(async move {
            start_manager
                .start(
                    ShellCommandRequest {
                        command: "recoverable-spawn-failure".to_owned(),
                        cwd: std::path::PathBuf::from("."),
                        tty: false,
                        yield_time: Duration::ZERO,
                        max_output_bytes: MAX_SHELL_OUTPUT_BYTES,
                    },
                    &NeverCancelled,
                )
                .await
        });
        gate.entered.wait().await;
        assert_eq!(
            manager
                .unpublished_sessions
                .lock()
                .expect("unpublished registry")
                .len(),
            1
        );
        assert_eq!(manager.slots.starting.load(Ordering::Acquire), 0);
        assert_eq!(manager.slots.running.load(Ordering::Acquire), 1);

        start.abort();
        assert!(
            start
                .await
                .expect_err("start task is aborted")
                .is_cancelled()
        );
        fail_cleanup.store(false, Ordering::Release);
        manager
            .disable_and_stop_all()
            .await
            .expect("disable retries the registered spawn failure");
        assert_eq!(manager.slots.running.load(Ordering::Acquire), 0);
        assert!(!process.running.load(Ordering::Acquire));
        assert!(
            manager
                .unpublished_sessions
                .lock()
                .expect("unpublished registry")
                .is_empty()
        );
        gate.release.wait().await;
    }

    #[tokio::test]
    async fn successful_unpublished_cleanup_retries_are_pruned_each_cycle() {
        let backend = Arc::new(PublishLockBackend::default());
        let manager = ShellSessionManager::new(
            backend.clone(),
            Arc::new(FixedIds),
            Arc::new(SystemShellClock),
        );

        for cycle in 0..3 {
            manager.enable().await;
            let fail = Arc::new(AtomicBool::new(true));
            let (starting_slot, running_slot) = manager.slots.reserve().expect("slot reservation");
            let mut pending = PendingStart::reserved(manager.clone(), starting_slot, running_slot);
            let mut spawned = backend
                .spawn(&ShellSpawnRequest {
                    command: format!("unpublished-cycle-{cycle}"),
                    cwd: std::path::PathBuf::from("."),
                    io_mode: ShellIoMode::Pipe,
                })
                .expect("scripted process starts");
            spawned.guard = Box::new(RecoveringContainmentGuard {
                process: Arc::clone(&backend.process),
                fail: Arc::clone(&fail),
            });
            let resources = manager
                .own_spawned_resources(spawned, ShellIoMode::Pipe)
                .unwrap_or_else(|_| panic!("scripted resources are owned"));
            pending.own_resources(resources);
            let (settlement_manager, settlement) = pending.take_settlement();
            assert_eq!(
                settlement_manager
                    .settle_unpublished_start(settlement)
                    .await,
                Err(ShellManagerError::Indeterminate)
            );
            assert_eq!(
                manager
                    .unpublished_sessions
                    .lock()
                    .expect("unpublished registry")
                    .len(),
                1,
                "failed unpublished cleanup is retained"
            );
            assert_eq!(manager.slots.running.load(Ordering::Acquire), 1);

            fail.store(false, Ordering::Release);
            manager
                .disable_and_stop_all()
                .await
                .expect("unpublished retry succeeds");
            assert_eq!(
                manager
                    .unpublished_sessions
                    .lock()
                    .expect("unpublished registry")
                    .len(),
                0,
                "successful unpublished cleanup is pruned in cycle {cycle}"
            );
            assert_eq!(manager.slots.starting.load(Ordering::Acquire), 0);
            assert_eq!(manager.slots.running.load(Ordering::Acquire), 0);
        }
    }

    impl ShellChild for PublishLockChild {
        fn process_id(&self) -> u32 {
            77
        }

        fn try_wait(&mut self) -> io::Result<Option<i32>> {
            Ok((!self.process.running.load(Ordering::Acquire)).then_some(-2))
        }

        fn kill(&mut self) -> io::Result<()> {
            self.process.exit();
            Ok(())
        }
    }

    struct PublishLockReader {
        receiver: std::sync::mpsc::Receiver<()>,
    }

    impl Read for PublishLockReader {
        fn read(&mut self, _destination: &mut [u8]) -> io::Result<usize> {
            let _ = self.receiver.recv();
            Ok(0)
        }
    }

    struct PublishLockWriter {
        process: Arc<PublishLockProcess>,
    }

    impl Write for PublishLockWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if bytes == b"\x03" {
                self.process.exit();
            }
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct BlockingIds {
        entered: Arc<(Mutex<bool>, Condvar)>,
        released: Arc<(Mutex<bool>, Condvar)>,
    }

    impl BlockingIds {
        fn wait_until_entered(&self) {
            let (entered, signal) = &*self.entered;
            let mut entered = entered.lock().expect("identifier entered");
            while !*entered {
                entered = signal.wait(entered).expect("identifier entered wait");
            }
        }

        fn release(&self) {
            let (released, signal) = &*self.released;
            *released.lock().expect("identifier released") = true;
            signal.notify_all();
        }
    }

    impl ShellSessionIdSource for BlockingIds {
        fn next_session_id(&self) -> Result<ShellSessionId, super::ShellManagerError> {
            {
                let (entered, signal) = &*self.entered;
                *entered.lock().expect("identifier entered") = true;
                signal.notify_all();
            }
            let (released, signal) = &*self.released;
            let mut released = released.lock().expect("identifier released");
            while !*released {
                released = signal.wait(released).expect("identifier release wait");
            }
            ShellSessionId::new("shell-publish-lock")
                .map_err(|_| super::ShellManagerError::Identifier)
        }
    }

    struct FixedIds;

    impl ShellSessionIdSource for FixedIds {
        fn next_session_id(&self) -> Result<ShellSessionId, super::ShellManagerError> {
            ShellSessionId::new("shell-lock-contention")
                .map_err(|_| super::ShellManagerError::Identifier)
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn abort_while_waiting_for_publish_lock_settles_reserved_start() {
        let backend = Arc::new(PublishLockBackend::default());
        let ids = Arc::new(BlockingIds::default());
        let manager =
            ShellSessionManager::new(backend.clone(), ids.clone(), Arc::new(SystemShellClock));
        manager.enable().await;

        let start_manager = manager.clone();
        let start = tokio::spawn(async move {
            start_manager
                .start(
                    ShellCommandRequest {
                        command: "publish-lock".to_owned(),
                        cwd: std::path::PathBuf::from("."),
                        tty: false,
                        yield_time: Duration::ZERO,
                        max_output_bytes: MAX_SHELL_OUTPUT_BYTES,
                    },
                    &NeverCancelled,
                )
                .await
        });
        let wait_ids = ids.clone();
        tokio::task::spawn_blocking(move || wait_ids.wait_until_entered())
            .await
            .expect("identifier waiter joins");
        let registry = manager.inner.lock().await;
        ids.release();
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
        start.abort();
        assert!(
            start
                .await
                .expect_err("publish waiter is aborted")
                .is_cancelled()
        );
        drop(registry);

        tokio::time::timeout(Duration::from_secs(1), manager.disable_and_stop_all())
            .await
            .expect("disable must not wait on an abandoned publish reservation")
            .expect("aborted publish cleanup succeeds");
        assert!(!backend.process.running.load(Ordering::Acquire));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn session_lock_contention_eventually_drops_the_armed_guard() {
        let backend = Arc::new(PublishLockBackend::default());
        let manager = ShellSessionManager::new(
            backend.clone(),
            Arc::new(FixedIds),
            Arc::new(SystemShellClock),
        );
        manager.enable().await;
        manager
            .start(
                ShellCommandRequest {
                    command: "lock-contention".to_owned(),
                    cwd: std::path::PathBuf::from("."),
                    tty: false,
                    yield_time: Duration::ZERO,
                    max_output_bytes: MAX_SHELL_OUTPUT_BYTES,
                },
                &NeverCancelled,
            )
            .await
            .expect("session starts");
        let session = manager
            .inner
            .lock()
            .await
            .sessions
            .values()
            .next()
            .cloned()
            .expect("published session");
        let entered = Arc::new((Mutex::new(false), Condvar::new()));
        let released = Arc::new((Mutex::new(false), Condvar::new()));
        let thread_entered = entered.clone();
        let thread_released = released.clone();
        let holder = std::thread::spawn(move || {
            let _session = session.lock().expect("session lock");
            {
                let (entered, signal) = &*thread_entered;
                *entered.lock().expect("holder entered") = true;
                signal.notify_all();
            }
            let (released, signal) = &*thread_released;
            let mut released = released.lock().expect("holder released");
            while !*released {
                released = signal.wait(released).expect("holder release wait");
            }
        });
        tokio::task::spawn_blocking(move || {
            let (entered, signal) = &*entered;
            let mut entered = entered.lock().expect("holder entered");
            while !*entered {
                entered = signal.wait(entered).expect("holder entered wait");
            }
        })
        .await
        .expect("holder waiter joins");

        drop(manager);
        assert!(backend.process.running.load(Ordering::Acquire));
        {
            let (released, signal) = &*released;
            *released.lock().expect("holder released") = true;
            signal.notify_all();
        }
        holder.join().expect("holder joins");

        assert!(!backend.process.running.load(Ordering::Acquire));
        assert_eq!(
            backend.process.guard_terminations.load(Ordering::Acquire),
            1
        );
    }
}

fn validate_yield_and_output(
    yield_time: Duration,
    max_output_bytes: usize,
) -> Result<(), ShellManagerError> {
    if yield_time > MAX_REQUEST_YIELD
        || max_output_bytes == 0
        || max_output_bytes > MAX_SHELL_OUTPUT_BYTES
    {
        Err(ShellManagerError::InvalidArguments)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod terminal_handshake_tests {
    use super::StartupCursorHandshake;

    #[test]
    fn complete_cursor_query_finishes_the_startup_handshake() {
        let mut handshake = StartupCursorHandshake::default();

        assert!(handshake.observe(b"\x1b[6n"));
    }

    #[test]
    fn split_cursor_query_finishes_only_after_its_final_chunk() {
        let mut handshake = StartupCursorHandshake::default();

        assert!(!handshake.observe(b"prefix\x1b["));
        assert!(handshake.observe(b"6n\x1b[6n"));
    }

    #[test]
    fn ordinary_terminal_content_does_not_finish_the_startup_handshake() {
        let mut handshake = StartupCursorHandshake::default();

        assert!(!handshake.observe(b"hello [6n \x1b[31mred"));
    }
}
