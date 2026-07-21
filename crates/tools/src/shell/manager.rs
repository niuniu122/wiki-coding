use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex, mpsc};
use std::thread;
use std::time::Duration;

use minimax_core::{CancellationPort, Clock};
use minimax_protocol::{
    MAX_SHELL_COMMAND_BYTES, MAX_SHELL_CWD_BYTES, MAX_SHELL_INPUT_BYTES, MAX_SHELL_OUTPUT_BYTES,
    MAX_TOOL_RESULT_BYTES, ShellReceipt, ShellSessionId, ShellSessionState,
};
use tokio::sync::Mutex;

use super::backend::{
    PtyBackend, PtyChild, PtyGuard, ReaderSpawner, ShellSessionIdSource, ShellSpawnRequest,
    SpawnedPty, SystemReaderSpawner,
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShellCommandRequest {
    pub command: String,
    pub cwd: PathBuf,
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
    backend: Arc<dyn PtyBackend>,
    ids: Arc<dyn ShellSessionIdSource>,
    clock: Arc<dyn Clock + Send + Sync>,
    output_budget: Arc<ShellOutputBudget>,
    start_settled: Arc<tokio::sync::Notify>,
    reader_spawner: Arc<dyn ReaderSpawner>,
    unpublished_sequence: Arc<AtomicU64>,
}

struct ShellSessionRegistry {
    accepting: bool,
    starting_slots: usize,
    running_slots: usize,
    terminal_sequence: u64,
    sessions: BTreeMap<ShellSessionId, Arc<StdMutex<ShellSession>>>,
    unpublished_sessions: BTreeMap<ShellSessionId, Arc<StdMutex<ShellSession>>>,
}

struct ShellSession {
    id: ShellSessionId,
    process_id: u32,
    child: Arc<StdMutex<Box<dyn PtyChild>>>,
    writer: Option<Arc<StdMutex<Box<dyn Write + Send>>>>,
    output: Arc<StdMutex<ShellOutputBuffer>>,
    reader: Option<thread::JoinHandle<()>>,
    reader_done: Option<mpsc::Receiver<()>>,
    guard: Option<Box<dyn PtyGuard>>,
    state: ShellSessionState,
    stopping: bool,
    cleanup_started: bool,
    cleanup_result: Option<Result<(), ShellManagerError>>,
    cleanup_notify: Arc<tokio::sync::Notify>,
    slot_release_deferred: bool,
    exit_code: Option<i32>,
    terminal_at_unix_ms: Option<u64>,
    terminal_sequence: Option<u64>,
}

impl ShellSessionManager {
    #[must_use]
    pub fn new(
        backend: Arc<dyn PtyBackend>,
        ids: Arc<dyn ShellSessionIdSource>,
        clock: Arc<dyn Clock + Send + Sync>,
    ) -> Self {
        Self::new_with_reader_spawner(backend, ids, clock, Arc::new(SystemReaderSpawner))
    }

    #[must_use]
    pub fn new_with_reader_spawner(
        backend: Arc<dyn PtyBackend>,
        ids: Arc<dyn ShellSessionIdSource>,
        clock: Arc<dyn Clock + Send + Sync>,
        reader_spawner: Arc<dyn ReaderSpawner>,
    ) -> Self {
        Self {
            inner: Arc::new(Mutex::new(ShellSessionRegistry {
                accepting: false,
                starting_slots: 0,
                running_slots: 0,
                terminal_sequence: 0,
                sessions: BTreeMap::new(),
                unpublished_sessions: BTreeMap::new(),
            })),
            backend,
            ids,
            clock,
            output_budget: Arc::new(ShellOutputBudget::new(DEFAULT_OUTPUT_BUDGET_BYTES)),
            start_settled: Arc::new(tokio::sync::Notify::new()),
            reader_spawner,
            unpublished_sequence: Arc::new(AtomicU64::new(0)),
        }
    }

    pub async fn enable(&self) {
        self.gc().await;
        self.inner.lock().await.accepting = true;
    }

    pub async fn start(
        &self,
        request: ShellCommandRequest,
        cancellation: &dyn CancellationPort,
    ) -> Result<ShellReceipt, ShellManagerError> {
        self.gc().await;
        validate_command_request(&request)?;
        if cancellation.is_cancelled() {
            return Err(ShellManagerError::Cancelled);
        }
        self.reserve_running_slot().await?;

        let spawn_request = ShellSpawnRequest {
            command: request.command,
            cwd: request.cwd,
            cols: 120,
            rows: 30,
        };
        let spawned = match self.backend.spawn(&spawn_request) {
            Ok(spawned) => spawned,
            Err(_) => {
                self.settle_unpublished_start(None).await?;
                return Err(ShellManagerError::Launch);
            }
        };
        let resources = match self.own_spawned_resources(spawned) {
            Ok(resources) => resources,
            Err(resources) => {
                let session = Arc::new(StdMutex::new(
                    resources.into_unpublished_session(self.next_unpublished_id()),
                ));
                self.settle_unpublished_start(Some(session)).await?;
                return Err(ShellManagerError::Io);
            }
        };
        let id = match self.ids.next_session_id() {
            Ok(id) => id,
            Err(_) => {
                let session = Arc::new(StdMutex::new(
                    resources.into_unpublished_session(self.next_unpublished_id()),
                ));
                self.settle_unpublished_start(Some(session)).await?;
                return Err(ShellManagerError::Identifier);
            }
        };
        let session = Arc::new(StdMutex::new(resources.into_session(id.clone())));

        let published = {
            let mut registry = self.inner.lock().await;
            if !registry.accepting || registry.sessions.contains_key(&id) {
                false
            } else {
                registry.sessions.insert(id.clone(), Arc::clone(&session));
                registry.starting_slots = registry.starting_slots.saturating_sub(1);
                true
            }
        };
        if published {
            self.start_settled.notify_waiters();
        }
        if !published {
            if let Ok(mut session) = session.lock() {
                session.id = self.next_unpublished_id();
                session.slot_release_deferred = true;
            }
            self.settle_unpublished_start(Some(session)).await?;
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
                return Err(ShellManagerError::Io);
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
        self.gc().await;
        loop {
            let settled = self.start_settled.notified();
            tokio::pin!(settled);
            settled.as_mut().enable();
            let has_starting_sessions = {
                let mut registry = self.inner.lock().await;
                registry.accepting = false;
                registry.starting_slots > 0
            };
            if !has_starting_sessions {
                break;
            }
            settled.as_mut().await;
        }
        let sessions = {
            let registry = self.inner.lock().await;
            let mut sessions = registry
                .sessions
                .iter()
                .filter_map(|(id, session)| {
                    let include = session
                        .lock()
                        .is_ok_and(|session| session.cleanup_result != Some(Ok(())));
                    include.then(|| (id.clone(), Arc::clone(session)))
                })
                .collect::<Vec<_>>();
            sessions.extend(
                registry
                    .unpublished_sessions
                    .iter()
                    .map(|(id, session)| (id.clone(), Arc::clone(session))),
            );
            sessions
        };

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

    async fn reserve_running_slot(&self) -> Result<(), ShellManagerError> {
        let mut registry = self.inner.lock().await;
        if !registry.accepting {
            return Err(ShellManagerError::Disabled);
        }
        if registry.running_slots >= MAX_RUNNING_SHELL_SESSIONS {
            return Err(ShellManagerError::SessionLimit);
        }
        registry.starting_slots += 1;
        registry.running_slots += 1;
        Ok(())
    }

    fn next_unpublished_id(&self) -> ShellSessionId {
        let sequence = self.unpublished_sequence.fetch_add(1, Ordering::AcqRel) + 1;
        ShellSessionId::new(format!("shell-unpublished-{sequence:04}"))
            .expect("generated unpublished shell identifier is valid")
    }

    fn own_spawned_resources(
        &self,
        spawned: SpawnedPty,
    ) -> Result<OwnedSessionResources, OwnedSessionResources> {
        let SpawnedPty {
            child,
            mut reader,
            writer,
            guard,
        } = spawned;
        let process_id = child.process_id();
        let child = Arc::new(StdMutex::new(child));
        let writer = Arc::new(StdMutex::new(writer));
        let output = Arc::new(StdMutex::new(ShellOutputBuffer::new(Arc::clone(
            &self.output_budget,
        ))));
        let reader_output = Arc::clone(&output);
        let (reader_done_tx, reader_done) = mpsc::sync_channel(1);
        let reader = self.reader_spawner.spawn(
            format!("shell-reader-{process_id}"),
            Box::new(move || {
                let mut chunk = [0_u8; READER_CHUNK_BYTES];
                loop {
                    match reader.read(&mut chunk) {
                        Ok(0) => {
                            if let Ok(mut output) = reader_output.lock() {
                                output.finish();
                            }
                            break;
                        }
                        Ok(read) => {
                            if let Ok(mut output) = reader_output.lock() {
                                output.append(&chunk[..read]);
                            } else {
                                break;
                            }
                        }
                        Err(_) => {
                            if let Ok(mut output) = reader_output.lock() {
                                output.finish();
                            }
                            break;
                        }
                    }
                }
                let _ = reader_done_tx.send(());
            }),
        );
        let resources = |reader, reader_done| OwnedSessionResources {
            process_id,
            child: Arc::clone(&child),
            writer: Arc::clone(&writer),
            output: Arc::clone(&output),
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
            if has_output || tokio::time::Instant::now() >= deadline {
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
    ) -> Result<(), ShellManagerError> {
        let release_running_slot = {
            let mut session = session.lock().map_err(|_| ShellManagerError::Io)?;
            if session.state != ShellSessionState::Running {
                None
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
                        Some(!session.slot_release_deferred)
                    }
                    Ok(None) => None,
                    Err(_) => return Err(ShellManagerError::Io),
                }
            }
        };
        if let Some(release_running_slot) = release_running_slot {
            let sequence = {
                let mut registry = self.inner.lock().await;
                if release_running_slot {
                    registry.running_slots = registry.running_slots.saturating_sub(1);
                }
                registry.terminal_sequence = registry.terminal_sequence.saturating_add(1);
                registry.terminal_sequence
            };
            if let Ok(mut session) = session.lock() {
                session.terminal_sequence = Some(sequence);
            }
        }
        Ok(())
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
        let notify = session
            .lock()
            .map_err(|_| ShellManagerError::Io)?
            .cleanup_notify
            .clone();
        loop {
            let notified = notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let role = {
                let mut session = session.lock().map_err(|_| ShellManagerError::Io)?;
                if let Some(result) = session.cleanup_result {
                    CleanupRole::Complete(result)
                } else if session.cleanup_started {
                    CleanupRole::Wait
                } else {
                    session.cleanup_started = true;
                    let running = session.state == ShellSessionState::Running;
                    if running {
                        session.stopping = true;
                    }
                    CleanupRole::Start { running }
                }
            };
            match role {
                CleanupRole::Complete(result) => return result,
                CleanupRole::Wait => notified.as_mut().await,
                CleanupRole::Start { running } => {
                    let manager = self.clone();
                    let session = Arc::clone(session);
                    let cleanup_notify = Arc::clone(&notify);
                    drop(tokio::spawn(async move {
                        manager.run_cleanup(session, running, cleanup_notify).await;
                    }));
                    notified.as_mut().await;
                }
            }
        }
    }

    async fn run_cleanup(
        &self,
        session: Arc<StdMutex<ShellSession>>,
        running: bool,
        notify: Arc<tokio::sync::Notify>,
    ) {
        let result = if running {
            self.cleanup_running_session(&session).await
        } else if self.close_handles_and_join_reader(&session).await {
            Ok(())
        } else {
            Err(ShellManagerError::Indeterminate)
        };
        if let Ok(mut session) = session.lock() {
            session.cleanup_started = false;
            session.cleanup_result = Some(result);
        }
        notify.notify_waiters();
    }

    async fn cleanup_running_session(
        &self,
        session: &Arc<StdMutex<ShellSession>>,
    ) -> Result<(), ShellManagerError> {
        let _ = self.write_interrupt(session).await;
        let exited_after_interrupt = self.wait_for_exit(session, CLEANUP_WAIT).await?;
        let mut terminate_ok = true;
        if !exited_after_interrupt {
            let process_id = session
                .lock()
                .map_err(|_| ShellManagerError::Io)?
                .process_id;
            terminate_ok = matches!(
                tokio::time::timeout(CLEANUP_WAIT, self.backend.terminate_tree(process_id)).await,
                Ok(Ok(()))
            );
            if self.kill_child(session).await == ChildKillOutcome::TimedOut {
                return Err(ShellManagerError::Indeterminate);
            }
        }
        let confirmed = if exited_after_interrupt {
            true
        } else {
            self.wait_for_exit(session, CLEANUP_WAIT).await?
        };
        let reader_done = if confirmed {
            self.close_handles_and_join_reader(session).await
        } else {
            false
        };
        if !terminate_ok || !confirmed || !reader_done {
            return Err(ShellManagerError::Indeterminate);
        }
        Ok(())
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

    async fn close_handles_and_join_reader(&self, session: &Arc<StdMutex<ShellSession>>) -> bool {
        let (reader, reader_done, writer, guard) = match session.lock() {
            Ok(mut session) => (
                session.reader.take(),
                session.reader_done.take(),
                session.writer.take(),
                session.guard.take(),
            ),
            Err(_) => return false,
        };
        drop(writer);
        drop(guard);
        let Some(reader) = reader else {
            return true;
        };
        let Some(reader_done) = reader_done else {
            drop(reader);
            return false;
        };
        matches!(
            tokio::task::spawn_blocking(move || {
                if reader_done.recv_timeout(CLEANUP_WAIT).is_err() {
                    drop(reader);
                    return false;
                }
                reader.join().is_ok()
            })
            .await,
            Ok(true)
        )
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

    async fn settle_unpublished_start(
        &self,
        session: Option<Arc<StdMutex<ShellSession>>>,
    ) -> Result<(), ShellManagerError> {
        let manager = self.clone();
        let settlement =
            tokio::spawn(async move { manager.finish_unpublished_start(session).await });
        settlement
            .await
            .unwrap_or(Err(ShellManagerError::Indeterminate))
    }

    async fn finish_unpublished_start(
        &self,
        session: Option<Arc<StdMutex<ShellSession>>>,
    ) -> Result<(), ShellManagerError> {
        let Some(session) = session else {
            let mut registry = self.inner.lock().await;
            registry.starting_slots = registry.starting_slots.saturating_sub(1);
            registry.running_slots = registry.running_slots.saturating_sub(1);
            self.start_settled.notify_waiters();
            return Ok(());
        };
        let internal_id = session
            .lock()
            .map(|session| session.id.clone())
            .unwrap_or_else(|_| self.next_unpublished_id());
        let cleanup = self.ensure_cleanup(&session).await;
        let mut registry = self.inner.lock().await;
        registry.starting_slots = registry.starting_slots.saturating_sub(1);
        match cleanup {
            Ok(()) => {
                registry.running_slots = registry.running_slots.saturating_sub(1);
            }
            Err(_) => {
                registry.unpublished_sessions.insert(internal_id, session);
            }
        }
        self.start_settled.notify_waiters();
        cleanup
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
                (session.state != ShellSessionState::Running
                    && !session.cleanup_started
                    && session.cleanup_result.is_some())
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
        let sessions = match self.inner.try_lock() {
            Ok(mut registry) => {
                registry.accepting = false;
                registry
                    .sessions
                    .values()
                    .chain(registry.unpublished_sessions.values())
                    .cloned()
                    .collect::<Vec<_>>()
            }
            Err(_) => return,
        };
        for session in sessions {
            let (writer, child) = match session.lock() {
                Ok(mut session) if session.state == ShellSessionState::Running => {
                    session.stopping = true;
                    session.guard.take();
                    (session.writer.take(), Arc::clone(&session.child))
                }
                _ => continue,
            };
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

struct OwnedSessionResources {
    process_id: u32,
    child: Arc<StdMutex<Box<dyn PtyChild>>>,
    writer: Arc<StdMutex<Box<dyn Write + Send>>>,
    output: Arc<StdMutex<ShellOutputBuffer>>,
    reader: Option<thread::JoinHandle<()>>,
    reader_done: Option<mpsc::Receiver<()>>,
    guard: Option<Box<dyn PtyGuard>>,
}

impl OwnedSessionResources {
    fn into_session(self, id: ShellSessionId) -> ShellSession {
        ShellSession {
            id,
            process_id: self.process_id,
            child: self.child,
            writer: Some(self.writer),
            output: self.output,
            reader: self.reader,
            reader_done: self.reader_done,
            guard: self.guard,
            state: ShellSessionState::Running,
            stopping: false,
            cleanup_started: false,
            cleanup_result: None,
            cleanup_notify: Arc::new(tokio::sync::Notify::new()),
            slot_release_deferred: false,
            exit_code: None,
            terminal_at_unix_ms: None,
            terminal_sequence: None,
        }
    }

    fn into_unpublished_session(self, id: ShellSessionId) -> ShellSession {
        let mut session = self.into_session(id);
        session.slot_release_deferred = true;
        session
    }
}

enum CleanupRole {
    Start { running: bool },
    Wait,
    Complete(Result<(), ShellManagerError>),
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

fn validate_write_request(request: &ShellWriteRequest) -> Result<(), ShellManagerError> {
    if (request.input.is_empty() && !request.submit) || request.input.len() > MAX_SHELL_INPUT_BYTES
    {
        return Err(ShellManagerError::InvalidArguments);
    }
    validate_yield_and_output(request.yield_time, request.max_output_bytes)
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
