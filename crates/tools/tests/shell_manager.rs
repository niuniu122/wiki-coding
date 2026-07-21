use std::collections::{HashMap, VecDeque};
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use minimax_core::{CancellationFuture, CancellationPort, Clock};
use minimax_protocol::{ShellSessionId, ShellSessionState};
use minimax_tools::{
    MAX_RUNNING_SHELL_SESSIONS, MAX_TERMINAL_SHELL_RECEIPTS, PtyBackend, PtyChild,
    PtyTerminateFuture, ShellCommandRequest, ShellManagerError, ShellPollRequest,
    ShellSessionIdSource, ShellSessionManager, ShellSpawnRequest, ShellWriteRequest, SpawnedPty,
    TERMINAL_RECEIPT_TTL,
};

const OUTPUT_LIMIT: usize = 1024;

#[derive(Default)]
struct ManualClock {
    unix_ms: AtomicU64,
}

impl ManualClock {
    fn advance(&self, duration: Duration) {
        self.unix_ms.fetch_add(
            u64::try_from(duration.as_millis()).expect("test duration fits u64"),
            Ordering::AcqRel,
        );
    }
}

impl Clock for ManualClock {
    fn now_unix_ms(&self) -> u64 {
        self.unix_ms.load(Ordering::Acquire)
    }
}

#[derive(Default)]
struct TestIds {
    next: AtomicUsize,
}

impl ShellSessionIdSource for TestIds {
    fn next_session_id(&self) -> Result<ShellSessionId, ShellManagerError> {
        let next = self.next.fetch_add(1, Ordering::AcqRel) + 1;
        ShellSessionId::new(format!("shell-test-{next:04}"))
            .map_err(|_| ShellManagerError::Identifier)
    }
}

#[derive(Default)]
struct TestCancellation {
    cancelled: AtomicBool,
    notify: tokio::sync::Notify,
}

impl TestCancellation {
    fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.notify.notify_waiters();
    }
}

impl CancellationPort for TestCancellation {
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    fn cancelled<'a>(&'a self) -> CancellationFuture<'a> {
        Box::pin(async move {
            while !self.is_cancelled() {
                self.notify.notified().await;
            }
        })
    }
}

struct NeverCancelled;

impl CancellationPort for NeverCancelled {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn cancelled<'a>(&'a self) -> CancellationFuture<'a> {
        Box::pin(std::future::pending())
    }
}

enum ReaderEvent {
    Bytes(Vec<u8>),
    Eof,
}

struct FakeShared {
    process_id: u32,
    exit_code: Mutex<Option<i32>>,
    input: Mutex<Vec<u8>>,
    reader_tx: Mutex<Option<std::sync::mpsc::Sender<ReaderEvent>>>,
    interrupts: AtomicUsize,
    kills: AtomicUsize,
    exit_on_interrupt: AtomicBool,
    flush_hook: Mutex<Option<FlushHook>>,
}

impl FakeShared {
    fn exit(&self, code: i32) {
        let mut exit_code = self.exit_code.lock().expect("exit lock");
        if exit_code.is_none() {
            *exit_code = Some(code);
            if let Some(sender) = self.reader_tx.lock().expect("reader sender lock").take() {
                let _ = sender.send(ReaderEvent::Eof);
            }
        }
    }
}

#[derive(Clone)]
struct FakeControl {
    shared: Arc<FakeShared>,
}

impl FakeControl {
    fn emit(&self, bytes: impl Into<Vec<u8>>) {
        if let Some(sender) = self
            .shared
            .reader_tx
            .lock()
            .expect("reader sender lock")
            .as_ref()
        {
            sender
                .send(ReaderEvent::Bytes(bytes.into()))
                .expect("reader remains connected");
        }
    }

    fn exit(&self, code: i32) {
        self.shared.exit(code);
    }

    fn input(&self) -> Vec<u8> {
        self.shared.input.lock().expect("input lock").clone()
    }

    fn set_flush_hook(&self, hook: FlushHook) {
        *self.shared.flush_hook.lock().expect("flush hook lock") = Some(hook);
    }

    fn interrupts(&self) -> usize {
        self.shared.interrupts.load(Ordering::Acquire)
    }

    fn kills(&self) -> usize {
        self.shared.kills.load(Ordering::Acquire)
    }

    fn is_running(&self) -> bool {
        self.shared.exit_code.lock().expect("exit lock").is_none()
    }
}

#[derive(Clone)]
struct FlushHook {
    cancellation: Arc<TestCancellation>,
    release: Arc<(Mutex<bool>, Condvar)>,
}

impl FlushHook {
    fn new(cancellation: Arc<TestCancellation>) -> Self {
        Self {
            cancellation,
            release: Arc::new((Mutex::new(false), Condvar::new())),
        }
    }

    fn release(&self) {
        let (released, signal) = &*self.release;
        *released.lock().expect("flush release lock") = true;
        signal.notify_all();
    }
}

struct FakePlan {
    shared: Arc<FakeShared>,
    reader_rx: std::sync::mpsc::Receiver<ReaderEvent>,
    spawn_gate: Option<SpawnGate>,
}

#[derive(Clone)]
struct SpawnGate {
    entered: Arc<(Mutex<bool>, Condvar)>,
    released: Arc<(Mutex<bool>, Condvar)>,
}

impl SpawnGate {
    fn new() -> Self {
        Self {
            entered: Arc::new((Mutex::new(false), Condvar::new())),
            released: Arc::new((Mutex::new(false), Condvar::new())),
        }
    }

    fn wait_until_entered(&self) {
        let (entered, signal) = &*self.entered;
        let mut entered = entered.lock().expect("spawn entered lock");
        while !*entered {
            entered = signal.wait(entered).expect("spawn entered wait");
        }
    }

    fn release(&self) {
        let (released, signal) = &*self.released;
        *released.lock().expect("spawn release lock") = true;
        signal.notify_all();
    }

    fn block_spawn(&self) {
        {
            let (entered, signal) = &*self.entered;
            *entered.lock().expect("spawn entered lock") = true;
            signal.notify_all();
        }
        let (released, signal) = &*self.released;
        let mut released = released.lock().expect("spawn release lock");
        while !*released {
            released = signal.wait(released).expect("spawn release wait");
        }
    }
}

#[derive(Default)]
struct FakeBackend {
    plans: Mutex<VecDeque<FakePlan>>,
    processes: Mutex<HashMap<u32, Arc<FakeShared>>>,
    spawns: AtomicUsize,
    terminations: AtomicUsize,
    spawn_notify: tokio::sync::Notify,
    next_process_id: AtomicU64,
}

impl FakeBackend {
    fn queue_process(&self) -> FakeControl {
        self.queue_process_with_gate(None)
    }

    fn queue_blocked_process(&self) -> (FakeControl, SpawnGate) {
        let gate = SpawnGate::new();
        let control = self.queue_process_with_gate(Some(gate.clone()));
        (control, gate)
    }

    fn queue_process_with_gate(&self, spawn_gate: Option<SpawnGate>) -> FakeControl {
        let process_id = u32::try_from(self.next_process_id.fetch_add(1, Ordering::AcqRel) + 1)
            .expect("test process id fits u32");
        let (reader_tx, reader_rx) = std::sync::mpsc::channel();
        let shared = Arc::new(FakeShared {
            process_id,
            exit_code: Mutex::new(None),
            input: Mutex::new(Vec::new()),
            reader_tx: Mutex::new(Some(reader_tx)),
            interrupts: AtomicUsize::new(0),
            kills: AtomicUsize::new(0),
            exit_on_interrupt: AtomicBool::new(true),
            flush_hook: Mutex::new(None),
        });
        self.plans.lock().expect("plans lock").push_back(FakePlan {
            shared: Arc::clone(&shared),
            reader_rx,
            spawn_gate,
        });
        FakeControl { shared }
    }

    fn queue_fast(&self, output: &[u8], exit_code: i32) -> FakeControl {
        let control = self.queue_process();
        control.emit(output.to_vec());
        control.exit(exit_code);
        control
    }

    fn spawn_count(&self) -> usize {
        self.spawns.load(Ordering::Acquire)
    }

    async fn wait_for_spawn_count(&self, expected: usize) {
        while self.spawn_count() < expected {
            self.spawn_notify.notified().await;
        }
    }
}

impl PtyBackend for FakeBackend {
    fn spawn(&self, request: &ShellSpawnRequest) -> io::Result<SpawnedPty> {
        assert_eq!((request.cols, request.rows), (120, 30));
        let plan = self
            .plans
            .lock()
            .expect("plans lock")
            .pop_front()
            .ok_or_else(|| io::Error::other("no fake process queued"))?;
        if let Some(gate) = &plan.spawn_gate {
            gate.block_spawn();
        }
        self.spawns.fetch_add(1, Ordering::AcqRel);
        self.spawn_notify.notify_waiters();
        self.processes
            .lock()
            .expect("processes lock")
            .insert(plan.shared.process_id, Arc::clone(&plan.shared));
        Ok(SpawnedPty {
            child: Box::new(FakeChild {
                shared: Arc::clone(&plan.shared),
            }),
            reader: Box::new(FakeReader {
                receiver: plan.reader_rx,
                pending: VecDeque::new(),
            }),
            writer: Box::new(FakeWriter {
                shared: Arc::clone(&plan.shared),
            }),
            guard: Box::new(()),
        })
    }

    fn terminate_tree<'a>(&'a self, process_id: u32) -> PtyTerminateFuture<'a> {
        Box::pin(async move {
            self.terminations.fetch_add(1, Ordering::AcqRel);
            let process = self
                .processes
                .lock()
                .expect("processes lock")
                .get(&process_id)
                .cloned()
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "unknown process"))?;
            process.exit(-15);
            Ok(())
        })
    }
}

struct FakeChild {
    shared: Arc<FakeShared>,
}

impl PtyChild for FakeChild {
    fn process_id(&self) -> u32 {
        self.shared.process_id
    }

    fn try_wait(&mut self) -> io::Result<Option<i32>> {
        Ok(*self.shared.exit_code.lock().expect("exit lock"))
    }

    fn kill(&mut self) -> io::Result<()> {
        self.shared.kills.fetch_add(1, Ordering::AcqRel);
        self.shared.exit(-9);
        Ok(())
    }
}

struct FakeReader {
    receiver: std::sync::mpsc::Receiver<ReaderEvent>,
    pending: VecDeque<u8>,
}

impl Read for FakeReader {
    fn read(&mut self, destination: &mut [u8]) -> io::Result<usize> {
        while self.pending.is_empty() {
            match self.receiver.recv() {
                Ok(ReaderEvent::Bytes(bytes)) => self.pending.extend(bytes),
                Ok(ReaderEvent::Eof) | Err(_) => return Ok(0),
            }
        }
        let count = destination.len().min(self.pending.len());
        for slot in &mut destination[..count] {
            *slot = self.pending.pop_front().expect("pending byte");
        }
        Ok(count)
    }
}

struct FakeWriter {
    shared: Arc<FakeShared>,
}

impl Write for FakeWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.shared
            .input
            .lock()
            .expect("input lock")
            .extend_from_slice(bytes);
        if bytes == b"\x03" {
            self.shared.interrupts.fetch_add(1, Ordering::AcqRel);
            if self.shared.exit_on_interrupt.load(Ordering::Acquire) {
                self.shared.exit(-2);
            }
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        let hook = self
            .shared
            .flush_hook
            .lock()
            .expect("flush hook lock")
            .clone();
        if let Some(hook) = hook {
            hook.cancellation.cancel();
            let (released, signal) = &*hook.release;
            let mut released = released.lock().expect("flush release lock");
            while !*released {
                released = signal.wait(released).expect("flush release wait");
            }
        }
        Ok(())
    }
}

fn command_request(command: &str, yield_time: Duration) -> ShellCommandRequest {
    ShellCommandRequest {
        command: command.to_owned(),
        cwd: PathBuf::from("."),
        yield_time,
        max_output_bytes: OUTPUT_LIMIT,
    }
}

fn poll_request(session_id: ShellSessionId, yield_time: Duration) -> ShellPollRequest {
    ShellPollRequest {
        session_id,
        yield_time,
        max_output_bytes: OUTPUT_LIMIT,
    }
}

fn manager(backend: Arc<FakeBackend>, clock: Arc<ManualClock>) -> ShellSessionManager {
    ShellSessionManager::new(backend, Arc::new(TestIds::default()), clock)
}

async fn enabled_manager(
    backend: Arc<FakeBackend>,
    clock: Arc<ManualClock>,
) -> ShellSessionManager {
    let manager = manager(backend, clock);
    manager.enable().await;
    manager
}

#[tokio::test]
async fn fast_command_returns_terminal_receipt_without_running_slot() {
    let backend = Arc::new(FakeBackend::default());
    backend.queue_fast(b"done\n", 0);
    let manager = enabled_manager(Arc::clone(&backend), Arc::new(ManualClock::default())).await;

    let receipt = manager
        .start(
            command_request("fast", Duration::from_millis(100)),
            &NeverCancelled,
        )
        .await
        .expect("fast command succeeds");
    assert_eq!(receipt.state, ShellSessionState::Exited);
    assert_eq!(receipt.exit_code, Some(0));
    assert_eq!(receipt.output, "done\n");

    for index in 0..MAX_RUNNING_SHELL_SESSIONS {
        backend.queue_process();
        let running = manager
            .start(
                command_request(&format!("long-{index}"), Duration::ZERO),
                &NeverCancelled,
            )
            .await
            .expect("terminal receipt did not retain a running slot");
        assert_eq!(running.state, ShellSessionState::Running);
    }
    assert_eq!(backend.spawn_count(), MAX_RUNNING_SHELL_SESSIONS + 1);
    manager.shutdown().await.expect("cleanup succeeds");
}

#[tokio::test]
async fn long_command_returns_id_and_poll_delivers_only_new_output() {
    let backend = Arc::new(FakeBackend::default());
    let control = backend.queue_process();
    let manager = enabled_manager(Arc::clone(&backend), Arc::new(ManualClock::default())).await;
    let started = manager
        .start(command_request("long", Duration::ZERO), &NeverCancelled)
        .await
        .expect("long command starts");
    assert_eq!(started.session_id.as_str(), "shell-test-0001");
    assert_eq!(started.state, ShellSessionState::Running);

    control.emit(b"first\n".to_vec());
    let first = manager
        .poll(
            poll_request(started.session_id.clone(), Duration::from_millis(100)),
            &NeverCancelled,
        )
        .await
        .expect("first poll succeeds");
    assert_eq!(first.output, "first\n");

    control.emit(b"second\n".to_vec());
    let second = manager
        .poll(
            poll_request(started.session_id.clone(), Duration::from_millis(100)),
            &NeverCancelled,
        )
        .await
        .expect("second poll succeeds");
    assert_eq!(second.output, "second\n");
    let empty = manager
        .poll(
            poll_request(started.session_id, Duration::ZERO),
            &NeverCancelled,
        )
        .await
        .expect("empty poll succeeds");
    assert!(empty.output.is_empty());
    manager.shutdown().await.expect("cleanup succeeds");
}

#[tokio::test]
async fn write_sends_exact_utf8_and_platform_enter_once() {
    let backend = Arc::new(FakeBackend::default());
    let control = backend.queue_process();
    let manager = enabled_manager(Arc::clone(&backend), Arc::new(ManualClock::default())).await;
    let started = manager
        .start(
            command_request("interactive", Duration::ZERO),
            &NeverCancelled,
        )
        .await
        .expect("interactive command starts");
    let receipt = manager
        .write(
            ShellWriteRequest {
                session_id: started.session_id,
                input: "你好🙂".to_owned(),
                submit: true,
                yield_time: Duration::ZERO,
                max_output_bytes: OUTPUT_LIMIT,
            },
            &NeverCancelled,
        )
        .await
        .expect("write succeeds");
    assert_eq!(receipt.state, ShellSessionState::Running);
    let mut expected = "你好🙂".as_bytes().to_vec();
    #[cfg(target_os = "windows")]
    expected.push(b'\r');
    #[cfg(target_os = "linux")]
    expected.push(b'\n');
    assert_eq!(control.input(), expected);
    manager.shutdown().await.expect("cleanup succeeds");
}

#[tokio::test]
async fn ninth_running_session_fails_before_backend_spawn() {
    let backend = Arc::new(FakeBackend::default());
    let manager = enabled_manager(Arc::clone(&backend), Arc::new(ManualClock::default())).await;
    for index in 0..MAX_RUNNING_SHELL_SESSIONS {
        backend.queue_process();
        manager
            .start(
                command_request(&format!("long-{index}"), Duration::ZERO),
                &NeverCancelled,
            )
            .await
            .expect("session within capacity starts");
    }
    let result = manager
        .start(command_request("ninth", Duration::ZERO), &NeverCancelled)
        .await;
    assert_eq!(result, Err(ShellManagerError::SessionLimit));
    assert_eq!(backend.spawn_count(), MAX_RUNNING_SHELL_SESSIONS);
    manager.shutdown().await.expect("cleanup succeeds");
}

#[tokio::test]
async fn cancel_before_id_delivery_stops_the_spawned_tree() {
    let backend = Arc::new(FakeBackend::default());
    let control = backend.queue_process();
    let manager = enabled_manager(Arc::clone(&backend), Arc::new(ManualClock::default())).await;
    let cancellation = Arc::new(TestCancellation::default());
    let start_manager = manager.clone();
    let start_cancellation = Arc::clone(&cancellation);
    let start = tokio::spawn(async move {
        start_manager
            .start(
                command_request("cancel-me", Duration::from_secs(60)),
                start_cancellation.as_ref(),
            )
            .await
    });
    backend.wait_for_spawn_count(1).await;
    cancellation.cancel();
    assert_eq!(
        start.await.expect("start task joins"),
        Err(ShellManagerError::Cancelled)
    );
    assert!(!control.is_running());
    assert_eq!(control.interrupts(), 1);
}

#[tokio::test]
async fn poll_cancellation_preserves_the_running_session() {
    let backend = Arc::new(FakeBackend::default());
    backend.queue_process();
    let manager = enabled_manager(Arc::clone(&backend), Arc::new(ManualClock::default())).await;
    let started = manager
        .start(command_request("long", Duration::ZERO), &NeverCancelled)
        .await
        .expect("long command starts");
    let cancellation = TestCancellation::default();
    cancellation.cancel();
    let result = manager
        .poll(
            poll_request(started.session_id.clone(), Duration::from_secs(60)),
            &cancellation,
        )
        .await;
    assert_eq!(result, Err(ShellManagerError::Cancelled));
    let preserved = manager
        .poll(
            poll_request(started.session_id, Duration::ZERO),
            &NeverCancelled,
        )
        .await
        .expect("session remains pollable");
    assert_eq!(preserved.state, ShellSessionState::Running);
    manager.shutdown().await.expect("cleanup succeeds");
}

#[tokio::test]
async fn write_after_bytes_are_committed_can_report_indeterminate() {
    let backend = Arc::new(FakeBackend::default());
    let control = backend.queue_process();
    let cancellation = Arc::new(TestCancellation::default());
    let flush_hook = FlushHook::new(Arc::clone(&cancellation));
    control.set_flush_hook(flush_hook.clone());
    let manager = enabled_manager(Arc::clone(&backend), Arc::new(ManualClock::default())).await;
    let started = manager
        .start(
            command_request("interactive", Duration::ZERO),
            &NeverCancelled,
        )
        .await
        .expect("interactive command starts");
    let result = manager
        .write(
            ShellWriteRequest {
                session_id: started.session_id.clone(),
                input: "side effect".to_owned(),
                submit: false,
                yield_time: Duration::ZERO,
                max_output_bytes: OUTPUT_LIMIT,
            },
            cancellation.as_ref(),
        )
        .await;
    assert_eq!(result, Err(ShellManagerError::Indeterminate));
    assert_eq!(control.input(), b"side effect");
    flush_hook.release();
    let preserved = manager
        .poll(
            poll_request(started.session_id, Duration::ZERO),
            &NeverCancelled,
        )
        .await
        .expect("session remains pollable");
    assert_eq!(preserved.state, ShellSessionState::Running);
    control.exit(0);
}

#[tokio::test]
async fn stop_is_terminal_and_idempotent() {
    let backend = Arc::new(FakeBackend::default());
    let control = backend.queue_process();
    let manager = enabled_manager(Arc::clone(&backend), Arc::new(ManualClock::default())).await;
    let started = manager
        .start(command_request("long", Duration::ZERO), &NeverCancelled)
        .await
        .expect("long command starts");
    let first = manager
        .stop(&started.session_id)
        .await
        .expect("first stop succeeds");
    let second = manager
        .stop(&started.session_id)
        .await
        .expect("second stop is idempotent");
    assert_eq!(first.state, ShellSessionState::Stopped);
    assert_eq!(second.state, ShellSessionState::Stopped);
    assert_eq!(control.interrupts(), 1);
    assert!(!control.is_running());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn disable_rejects_new_start_and_write_then_stops_all() {
    let backend = Arc::new(FakeBackend::default());
    let first_control = backend.queue_process();
    let second_control = backend.queue_process();
    let manager = enabled_manager(Arc::clone(&backend), Arc::new(ManualClock::default())).await;
    let first = manager
        .start(command_request("first", Duration::ZERO), &NeverCancelled)
        .await
        .expect("first command starts");
    manager
        .start(command_request("second", Duration::ZERO), &NeverCancelled)
        .await
        .expect("second command starts");

    manager
        .disable_and_stop_all()
        .await
        .expect("disable cleanup succeeds");
    assert!(!first_control.is_running());
    assert!(!second_control.is_running());
    assert_eq!(
        manager
            .start(command_request("rejected", Duration::ZERO), &NeverCancelled)
            .await,
        Err(ShellManagerError::Disabled)
    );
    assert_eq!(
        manager
            .write(
                ShellWriteRequest {
                    session_id: first.session_id,
                    input: "ignored".to_owned(),
                    submit: false,
                    yield_time: Duration::ZERO,
                    max_output_bytes: OUTPUT_LIMIT,
                },
                &NeverCancelled,
            )
            .await,
        Err(ShellManagerError::Disabled)
    );
    assert_eq!(backend.spawn_count(), 2);

    let racing_backend = Arc::new(FakeBackend::default());
    let (racing_control, spawn_gate) = racing_backend.queue_blocked_process();
    let racing_manager = enabled_manager(
        Arc::clone(&racing_backend),
        Arc::new(ManualClock::default()),
    )
    .await;
    let start_manager = racing_manager.clone();
    let start = tokio::spawn(async move {
        start_manager
            .start(command_request("starting", Duration::ZERO), &NeverCancelled)
            .await
    });
    let wait_gate = spawn_gate.clone();
    tokio::task::spawn_blocking(move || wait_gate.wait_until_entered())
        .await
        .expect("spawn wait joins");

    let disable_manager = racing_manager.clone();
    let disable = tokio::spawn(async move { disable_manager.disable_and_stop_all().await });
    let unknown_id = ShellSessionId::new("shell-test-9999").expect("valid test id");
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            let result = racing_manager
                .write(
                    ShellWriteRequest {
                        session_id: unknown_id.clone(),
                        input: "blocked".to_owned(),
                        submit: false,
                        yield_time: Duration::ZERO,
                        max_output_bytes: OUTPUT_LIMIT,
                    },
                    &NeverCancelled,
                )
                .await;
            if result == Err(ShellManagerError::Disabled) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("disable marks manager before spawn is released");
    for _ in 0..100 {
        if disable.is_finished() {
            break;
        }
        tokio::task::yield_now().await;
    }
    let returned_before_spawn_settled = disable.is_finished();
    spawn_gate.release();
    assert_eq!(
        start.await.expect("start task joins"),
        Err(ShellManagerError::Disabled)
    );
    disable
        .await
        .expect("disable task joins")
        .expect("disable cleanup succeeds");
    assert!(
        !returned_before_spawn_settled,
        "disable returned before the reserved spawn settled"
    );
    assert!(!racing_control.is_running());
}

#[tokio::test]
async fn terminal_receipts_expire_by_count_and_clock() {
    let backend = Arc::new(FakeBackend::default());
    let clock = Arc::new(ManualClock::default());
    let manager = enabled_manager(Arc::clone(&backend), Arc::clone(&clock)).await;
    let mut receipts = Vec::new();
    for index in 0..=MAX_TERMINAL_SHELL_RECEIPTS {
        backend.queue_fast(&[], 0);
        receipts.push(
            manager
                .start(
                    command_request(&format!("fast-{index}"), Duration::ZERO),
                    &NeverCancelled,
                )
                .await
                .expect("fast command exits"),
        );
    }
    assert_eq!(
        manager
            .poll(
                poll_request(receipts[0].session_id.clone(), Duration::ZERO),
                &NeverCancelled,
            )
            .await,
        Err(ShellManagerError::SessionNotFound)
    );

    let newest = receipts.last().expect("newest receipt").session_id.clone();
    clock.advance(TERMINAL_RECEIPT_TTL + Duration::from_millis(1));
    assert_eq!(
        manager
            .poll(poll_request(newest, Duration::ZERO), &NeverCancelled)
            .await,
        Err(ShellManagerError::SessionNotFound)
    );
}

#[tokio::test]
async fn manager_drop_requests_best_effort_cleanup() {
    let backend = Arc::new(FakeBackend::default());
    let control = backend.queue_process();
    {
        let manager = enabled_manager(Arc::clone(&backend), Arc::new(ManualClock::default())).await;
        manager
            .start(command_request("long", Duration::ZERO), &NeverCancelled)
            .await
            .expect("long command starts");
    }
    assert!(!control.is_running());
    assert_eq!(control.interrupts(), 1);
    assert_eq!(control.kills(), 1);
}
