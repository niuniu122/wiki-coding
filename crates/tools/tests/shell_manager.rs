use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use minimax_core::{CancellationFuture, CancellationPort, Clock};
use minimax_protocol::{MAX_SHELL_UNREAD_BYTES, ShellSessionId, ShellSessionState};
use minimax_tools::{
    MAX_RUNNING_SHELL_SESSIONS, MAX_TERMINAL_SHELL_RECEIPTS, ProcessShellSessionIds, PtyBackend,
    PtyChild, PtyTerminateFuture, ReaderSpawner, ReaderTask, ShellCleanupError,
    ShellCommandRequest, ShellManagerError, ShellPollRequest, ShellSessionIdSource,
    ShellSessionManager, ShellSpawnRequest, ShellWriteRequest, SpawnedPty, TERMINAL_RECEIPT_TTL,
};

const OUTPUT_LIMIT: usize = 1024;

#[test]
fn process_shell_session_ids_are_unique_lowercase_hex_and_well_formed() {
    let ids = ProcessShellSessionIds::new().expect("process shell session ID source");
    let mut seen = HashSet::new();

    for _ in 0..256 {
        let id = ids.next_session_id().expect("next shell session ID");
        let value = id.as_str();
        assert!(
            seen.insert(value.to_owned()),
            "duplicate session ID: {value}"
        );
        let (nonce, counter) = value
            .strip_prefix("shell-")
            .and_then(|suffix| suffix.split_once('-'))
            .expect("shell-<nonce>-<counter> format");
        assert_eq!(nonce.len(), 16);
        assert_eq!(counter.len(), 16);
        assert!(
            nonce
                .bytes()
                .chain(counter.bytes())
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
            "session ID must use lowercase hexadecimal: {value}"
        );
    }
}

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
struct FailOnceIds {
    calls: AtomicUsize,
}

struct FailingReaderSpawner;

impl ReaderSpawner for FailingReaderSpawner {
    fn spawn(&self, _name: String, task: ReaderTask) -> io::Result<std::thread::JoinHandle<()>> {
        drop(task);
        Err(io::Error::other("scripted reader thread spawn failure"))
    }
}

impl ShellSessionIdSource for FailOnceIds {
    fn next_session_id(&self) -> Result<ShellSessionId, ShellManagerError> {
        let call = self.calls.fetch_add(1, Ordering::AcqRel);
        if call == 0 {
            return Err(ShellManagerError::Identifier);
        }
        ShellSessionId::new(format!("shell-retry-{call:04}"))
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
    interrupt_wait: (Mutex<usize>, Condvar),
    interrupt_flush_wait: (Mutex<usize>, Condvar),
    kills: AtomicUsize,
    try_wait_error: AtomicBool,
    kill_error: AtomicBool,
    kill_gate: Mutex<Option<KillGate>>,
    exit_on_interrupt: AtomicBool,
    flush_hook: Mutex<Option<FlushHook>>,
    flush_error: Mutex<Option<io::ErrorKind>>,
    guard_drops: AtomicUsize,
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

    fn finish_reader(&self) {
        if let Some(sender) = self
            .shared
            .reader_tx
            .lock()
            .expect("reader sender lock")
            .take()
        {
            let _ = sender.send(ReaderEvent::Eof);
        }
    }

    fn input(&self) -> Vec<u8> {
        self.shared.input.lock().expect("input lock").clone()
    }

    fn set_exit_on_interrupt(&self, enabled: bool) {
        self.shared
            .exit_on_interrupt
            .store(enabled, Ordering::Release);
    }

    fn set_try_wait_error(&self, enabled: bool) {
        self.shared.try_wait_error.store(enabled, Ordering::Release);
    }

    fn set_kill_error(&self, enabled: bool) {
        self.shared.kill_error.store(enabled, Ordering::Release);
    }

    fn set_kill_gate(&self, gate: KillGate) {
        *self.shared.kill_gate.lock().expect("kill gate lock") = Some(gate);
    }

    fn set_flush_hook(&self, hook: FlushHook) {
        *self.shared.flush_hook.lock().expect("flush hook lock") = Some(hook);
    }

    fn set_flush_error(&self, kind: io::ErrorKind) {
        *self.shared.flush_error.lock().expect("flush error lock") = Some(kind);
    }

    fn interrupts(&self) -> usize {
        self.shared.interrupts.load(Ordering::Acquire)
    }

    fn wait_for_interrupts(&self, expected: usize, timeout: Duration) -> bool {
        let (count, signal) = &self.shared.interrupt_wait;
        let count = count.lock().expect("interrupt wait lock");
        let (count, _) = signal
            .wait_timeout_while(count, timeout, |count| *count < expected)
            .expect("interrupt wait");
        *count >= expected
    }

    fn wait_for_interrupt_flushes(&self, expected: usize, timeout: Duration) -> bool {
        let (count, signal) = &self.shared.interrupt_flush_wait;
        let count = count.lock().expect("interrupt flush wait lock");
        let (count, _) = signal
            .wait_timeout_while(count, timeout, |count| *count < expected)
            .expect("interrupt flush wait");
        *count >= expected
    }

    fn kills(&self) -> usize {
        self.shared.kills.load(Ordering::Acquire)
    }

    fn is_running(&self) -> bool {
        self.shared.exit_code.lock().expect("exit lock").is_none()
    }

    fn guard_drops(&self) -> usize {
        self.shared.guard_drops.load(Ordering::Acquire)
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

#[derive(Clone)]
struct TerminationGate {
    entries: Arc<AtomicUsize>,
    entered: Arc<tokio::sync::Notify>,
    released: Arc<AtomicBool>,
    release: Arc<tokio::sync::Notify>,
}

#[derive(Clone)]
struct KillGate {
    entered: Arc<AtomicBool>,
    entered_notify: Arc<tokio::sync::Notify>,
    released: Arc<(Mutex<bool>, Condvar)>,
}

impl KillGate {
    fn new() -> Self {
        Self {
            entered: Arc::new(AtomicBool::new(false)),
            entered_notify: Arc::new(tokio::sync::Notify::new()),
            released: Arc::new((Mutex::new(false), Condvar::new())),
        }
    }

    fn block(&self) {
        self.entered.store(true, Ordering::Release);
        self.entered_notify.notify_waiters();
        let (released, signal) = &*self.released;
        let mut released = released.lock().expect("kill release lock");
        while !*released {
            released = signal.wait(released).expect("kill release wait");
        }
    }

    async fn wait_until_entered(&self) {
        while !self.entered.load(Ordering::Acquire) {
            self.entered_notify.notified().await;
        }
    }

    fn is_entered(&self) -> bool {
        self.entered.load(Ordering::Acquire)
    }

    fn release(&self) {
        let (released, signal) = &*self.released;
        *released.lock().expect("kill release lock") = true;
        signal.notify_all();
    }
}

impl TerminationGate {
    fn new() -> Self {
        Self {
            entries: Arc::new(AtomicUsize::new(0)),
            entered: Arc::new(tokio::sync::Notify::new()),
            released: Arc::new(AtomicBool::new(false)),
            release: Arc::new(tokio::sync::Notify::new()),
        }
    }

    async fn enter(&self) {
        self.entries.fetch_add(1, Ordering::AcqRel);
        self.entered.notify_waiters();
        while !self.released.load(Ordering::Acquire) {
            self.release.notified().await;
        }
    }

    async fn wait_for_entries(&self, expected: usize) {
        while self.entries.load(Ordering::Acquire) < expected {
            self.entered.notified().await;
        }
    }

    fn entry_count(&self) -> usize {
        self.entries.load(Ordering::Acquire)
    }

    fn release(&self) {
        self.released.store(true, Ordering::Release);
        self.release.notify_waiters();
    }
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
    termination_error: AtomicBool,
    termination_gate: Mutex<Option<TerminationGate>>,
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
            interrupt_wait: (Mutex::new(0), Condvar::new()),
            interrupt_flush_wait: (Mutex::new(0), Condvar::new()),
            kills: AtomicUsize::new(0),
            try_wait_error: AtomicBool::new(false),
            kill_error: AtomicBool::new(false),
            kill_gate: Mutex::new(None),
            exit_on_interrupt: AtomicBool::new(true),
            flush_hook: Mutex::new(None),
            flush_error: Mutex::new(None),
            guard_drops: AtomicUsize::new(0),
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

    fn set_termination_error(&self, enabled: bool) {
        self.termination_error.store(enabled, Ordering::Release);
    }

    fn set_termination_gate(&self, gate: TerminationGate) {
        *self.termination_gate.lock().expect("termination gate lock") = Some(gate);
    }

    fn termination_count(&self) -> usize {
        self.terminations.load(Ordering::Acquire)
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
            guard: Box::new(FakeGuard {
                shared: Arc::clone(&plan.shared),
            }),
        })
    }

    fn terminate_tree<'a>(&'a self, process_id: u32) -> PtyTerminateFuture<'a> {
        Box::pin(async move {
            self.terminations.fetch_add(1, Ordering::AcqRel);
            let gate = self
                .termination_gate
                .lock()
                .expect("termination gate lock")
                .clone();
            if let Some(gate) = gate {
                gate.enter().await;
            }
            if self.termination_error.load(Ordering::Acquire) {
                return Err(io::Error::other("scripted tree termination failure"));
            }
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

struct FakeGuard {
    shared: Arc<FakeShared>,
}

impl Drop for FakeGuard {
    fn drop(&mut self) {
        self.shared.guard_drops.fetch_add(1, Ordering::AcqRel);
    }
}

impl PtyChild for FakeChild {
    fn process_id(&self) -> u32 {
        self.shared.process_id
    }

    fn try_wait(&mut self) -> io::Result<Option<i32>> {
        if self.shared.try_wait_error.load(Ordering::Acquire) {
            return Err(io::Error::other("scripted try_wait failure"));
        }
        Ok(*self.shared.exit_code.lock().expect("exit lock"))
    }

    fn kill(&mut self) -> io::Result<()> {
        self.shared.kills.fetch_add(1, Ordering::AcqRel);
        let gate = self
            .shared
            .kill_gate
            .lock()
            .expect("kill gate lock")
            .clone();
        if let Some(gate) = gate {
            gate.block();
        }
        if self.shared.kill_error.load(Ordering::Acquire) {
            return Err(io::Error::other("scripted kill failure"));
        }
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
            let (count, signal) = &self.shared.interrupt_wait;
            *count.lock().expect("interrupt wait lock") += 1;
            signal.notify_all();
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
        let result = if let Some(kind) = *self.shared.flush_error.lock().expect("flush error lock")
        {
            Err(io::Error::new(kind, "scripted flush failure"))
        } else {
            Ok(())
        };
        let interrupt_count = self.shared.interrupts.load(Ordering::Acquire);
        let (flushed, signal) = &self.shared.interrupt_flush_wait;
        let mut flushed = flushed.lock().expect("interrupt flush wait lock");
        if *flushed < interrupt_count {
            *flushed = interrupt_count;
            signal.notify_all();
        }
        result
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

#[tokio::test(start_paused = true)]
async fn aborted_unpublished_start_settles_reservation() {
    let backend = Arc::new(FakeBackend::default());
    let control = backend.queue_process();
    control.set_exit_on_interrupt(false);
    let kill_gate = KillGate::new();
    control.set_kill_gate(kill_gate.clone());
    let manager = ShellSessionManager::new(
        backend.clone(),
        Arc::new(FailOnceIds::default()),
        Arc::new(ManualClock::default()),
    );
    manager.enable().await;

    let start_manager = manager.clone();
    let start = tokio::spawn(async move {
        start_manager
            .start(
                command_request("identifier-failure", Duration::ZERO),
                &NeverCancelled,
            )
            .await
    });
    let wait_control = control.clone();
    assert!(
        tokio::task::spawn_blocking(move || {
            wait_control.wait_for_interrupt_flushes(1, Duration::from_millis(100))
        })
        .await
        .expect("interrupt waiter joins")
    );
    for _ in 0..=400 {
        if kill_gate.is_entered() {
            break;
        }
        tokio::time::advance(Duration::from_millis(5)).await;
    }
    assert!(
        kill_gate.is_entered(),
        "unpublished cleanup reaches child kill"
    );
    start.abort();
    assert!(
        start
            .await
            .expect_err("unpublished start caller is aborted")
            .is_cancelled()
    );

    kill_gate.release();
    tokio::time::timeout(Duration::from_millis(500), manager.disable_and_stop_all())
        .await
        .expect("disable must not wait on an abandoned starting reservation")
        .expect("unpublished cleanup succeeds");
    assert!(!control.is_running());
    assert_eq!(control.interrupts(), 1);
    assert_eq!(control.kills(), 1);

    manager.enable().await;
    for index in 0..MAX_RUNNING_SHELL_SESSIONS {
        backend.queue_process();
        manager
            .start(
                command_request(&format!("replacement-{index}"), Duration::ZERO),
                &NeverCancelled,
            )
            .await
            .expect("every running slot is reusable");
    }
    manager
        .shutdown()
        .await
        .expect("replacement cleanup succeeds");
}

#[tokio::test(start_paused = true)]
async fn reader_spawn_failure_keeps_partial_ownership_until_cleanup_confirms_exit() {
    let backend = Arc::new(FakeBackend::default());
    let control = backend.queue_process();
    control.set_exit_on_interrupt(false);
    let termination_gate = TerminationGate::new();
    backend.set_termination_gate(termination_gate.clone());
    let manager = ShellSessionManager::new_with_reader_spawner(
        backend.clone(),
        Arc::new(TestIds::default()),
        Arc::new(ManualClock::default()),
        Arc::new(FailingReaderSpawner),
    );
    manager.enable().await;

    let start_manager = manager.clone();
    let start = tokio::spawn(async move {
        start_manager
            .start(
                command_request("reader-spawn-failure", Duration::ZERO),
                &NeverCancelled,
            )
            .await
    });
    let wait_control = control.clone();
    assert!(
        tokio::task::spawn_blocking(move || {
            wait_control.wait_for_interrupt_flushes(1, Duration::from_millis(100))
        })
        .await
        .expect("interrupt waiter joins"),
        "partial cleanup writes ETX before its natural wait"
    );
    for _ in 0..=400 {
        if termination_gate.entry_count() == 1 {
            break;
        }
        tokio::time::advance(Duration::from_millis(5)).await;
    }
    assert_eq!(
        termination_gate.entry_count(),
        1,
        "reader spawn failure must retain the child for tree cleanup"
    );
    assert!(control.is_running());
    assert!(!start.is_finished());

    let disable_manager = manager.clone();
    let disable = tokio::spawn(async move { disable_manager.disable_and_stop_all().await });
    tokio::task::yield_now().await;
    assert!(
        !disable.is_finished(),
        "reservation released before exit was confirmed"
    );

    termination_gate.release();
    assert_eq!(
        start.await.expect("failed start caller joins"),
        Err(ShellManagerError::Io)
    );
    disable
        .await
        .expect("disable caller joins")
        .expect("partial cleanup succeeds");
    assert!(!control.is_running());
    assert_eq!(control.interrupts(), 1);
    assert_eq!(backend.termination_count(), 1);
    assert_eq!(control.kills(), 1);
    assert_eq!(control.guard_drops(), 1);
}

#[tokio::test(start_paused = true)]
async fn identifier_failure_uses_tree_termination_and_exit_confirmation() {
    let backend = Arc::new(FakeBackend::default());
    let control = backend.queue_process();
    control.set_exit_on_interrupt(false);
    let manager = ShellSessionManager::new(
        backend.clone(),
        Arc::new(FailOnceIds::default()),
        Arc::new(ManualClock::default()),
    );
    manager.enable().await;

    let start_manager = manager.clone();
    let start = tokio::spawn(async move {
        start_manager
            .start(
                command_request("identifier-failure", Duration::ZERO),
                &NeverCancelled,
            )
            .await
    });
    tokio::time::advance(Duration::from_secs(2)).await;
    assert_eq!(
        start.await.expect("identifier failure caller joins"),
        Err(ShellManagerError::Identifier)
    );
    assert!(!control.is_running());
    assert_eq!(control.interrupts(), 1);
    assert_eq!(backend.termination_count(), 1);
    assert_eq!(control.kills(), 1);
    assert_eq!(control.guard_drops(), 1);
}

#[tokio::test(start_paused = true)]
async fn unpublished_cleanup_failure_retains_slot_and_is_reported_by_disable() {
    let backend = Arc::new(FakeBackend::default());
    let control = backend.queue_process();
    control.set_exit_on_interrupt(false);
    backend.set_termination_error(true);
    let manager = ShellSessionManager::new(
        backend.clone(),
        Arc::new(FailOnceIds::default()),
        Arc::new(ManualClock::default()),
    );
    manager.enable().await;

    let start_manager = manager.clone();
    let start = tokio::spawn(async move {
        start_manager
            .start(
                command_request("uncertain-identifier-failure", Duration::ZERO),
                &NeverCancelled,
            )
            .await
    });
    tokio::time::advance(Duration::from_secs(2)).await;
    assert_eq!(
        start.await.expect("uncertain start caller joins"),
        Err(ShellManagerError::Indeterminate)
    );
    assert!(!control.is_running());
    assert_eq!(backend.termination_count(), 1);

    let internal_id =
        ShellSessionId::new("shell-unpublished-0001").expect("valid internal session id");
    assert_eq!(
        manager.stop(&internal_id).await,
        Err(ShellManagerError::SessionNotFound)
    );
    let expected = ShellCleanupError {
        session_ids: vec![internal_id],
    };
    assert_eq!(manager.disable_and_stop_all().await, Err(expected.clone()));

    manager.enable().await;
    for index in 0..(MAX_RUNNING_SHELL_SESSIONS - 1) {
        backend.queue_process();
        manager
            .start(
                command_request(&format!("replacement-{index}"), Duration::ZERO),
                &NeverCancelled,
            )
            .await
            .expect("capacity excluding retained unpublished slot starts");
    }
    assert_eq!(
        manager
            .start(
                command_request("over-retained-capacity", Duration::ZERO),
                &NeverCancelled,
            )
            .await,
        Err(ShellManagerError::SessionLimit)
    );
    assert_eq!(manager.shutdown().await, Err(expected));
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
async fn real_interrupted_flush_without_cancellation_is_not_cancelled() {
    let backend = Arc::new(FakeBackend::default());
    let control = backend.queue_process();
    control.set_flush_error(io::ErrorKind::Interrupted);
    let manager = enabled_manager(Arc::clone(&backend), Arc::new(ManualClock::default())).await;
    let started = manager
        .start(
            command_request("interactive", Duration::ZERO),
            &NeverCancelled,
        )
        .await
        .expect("interactive command starts");

    assert_eq!(
        manager
            .write(
                ShellWriteRequest {
                    session_id: started.session_id,
                    input: "committed".to_owned(),
                    submit: false,
                    yield_time: Duration::ZERO,
                    max_output_bytes: OUTPUT_LIMIT,
                },
                &NeverCancelled,
            )
            .await,
        Err(ShellManagerError::Io)
    );
    assert_eq!(control.input(), b"committed");
}

#[tokio::test]
async fn write_observed_exit_is_cleaned_before_return() {
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
    control.exit(0);

    assert_eq!(
        manager
            .write(
                ShellWriteRequest {
                    session_id: started.session_id,
                    input: "too late".to_owned(),
                    submit: false,
                    yield_time: Duration::ZERO,
                    max_output_bytes: OUTPUT_LIMIT,
                },
                &NeverCancelled,
            )
            .await,
        Err(ShellManagerError::SessionNotFound)
    );
    assert_eq!(control.guard_drops(), 1);

    manager
        .shutdown()
        .await
        .expect("shutdown accepts already-cleaned exit");
    assert_eq!(control.guard_drops(), 1);
}

#[tokio::test(start_paused = true)]
async fn repeated_write_observed_exits_are_bounded_by_count() {
    let backend = Arc::new(FakeBackend::default());
    let manager = enabled_manager(Arc::clone(&backend), Arc::new(ManualClock::default())).await;
    let mut session_ids = Vec::new();
    for index in 0..=MAX_TERMINAL_SHELL_RECEIPTS {
        let control = backend.queue_process();
        let started = manager
            .start(
                command_request(&format!("write-exit-{index}"), Duration::ZERO),
                &NeverCancelled,
            )
            .await
            .expect("interactive command starts");
        control.exit(0);
        assert_eq!(
            manager
                .write(
                    ShellWriteRequest {
                        session_id: started.session_id.clone(),
                        input: "too late".to_owned(),
                        submit: false,
                        yield_time: Duration::ZERO,
                        max_output_bytes: OUTPUT_LIMIT,
                    },
                    &NeverCancelled,
                )
                .await,
            Err(ShellManagerError::SessionNotFound)
        );
        session_ids.push(started.session_id);
    }

    assert_eq!(
        manager
            .poll(
                poll_request(session_ids[0].clone(), Duration::ZERO),
                &NeverCancelled,
            )
            .await,
        Err(ShellManagerError::SessionNotFound)
    );
}

#[tokio::test(start_paused = true)]
async fn write_observed_exit_expires_by_clock() {
    let backend = Arc::new(FakeBackend::default());
    let clock = Arc::new(ManualClock::default());
    let manager = enabled_manager(Arc::clone(&backend), Arc::clone(&clock)).await;
    let control = backend.queue_process();
    let started = manager
        .start(
            command_request("write-exit", Duration::ZERO),
            &NeverCancelled,
        )
        .await
        .expect("interactive command starts");
    control.exit(0);
    assert_eq!(
        manager
            .write(
                ShellWriteRequest {
                    session_id: started.session_id.clone(),
                    input: "too late".to_owned(),
                    submit: false,
                    yield_time: Duration::ZERO,
                    max_output_bytes: OUTPUT_LIMIT,
                },
                &NeverCancelled,
            )
            .await,
        Err(ShellManagerError::SessionNotFound)
    );

    clock.advance(TERMINAL_RECEIPT_TTL + Duration::from_millis(1));
    assert_eq!(
        manager
            .poll(
                poll_request(started.session_id, Duration::ZERO),
                &NeverCancelled,
            )
            .await,
        Err(ShellManagerError::SessionNotFound)
    );
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

#[tokio::test(start_paused = true)]
async fn concurrent_stop_has_one_cleanup_owner() {
    let backend = Arc::new(FakeBackend::default());
    let control = backend.queue_process();
    control.set_exit_on_interrupt(false);
    let termination_gate = TerminationGate::new();
    backend.set_termination_gate(termination_gate.clone());
    let manager = enabled_manager(Arc::clone(&backend), Arc::new(ManualClock::default())).await;
    let started = manager
        .start(command_request("long", Duration::ZERO), &NeverCancelled)
        .await
        .expect("long command starts");

    let first_manager = manager.clone();
    let first_id = started.session_id.clone();
    let first = tokio::spawn(async move { first_manager.stop(&first_id).await });
    tokio::time::advance(Duration::from_secs(2)).await;
    termination_gate.wait_for_entries(1).await;

    let second_manager = manager.clone();
    let second_id = started.session_id;
    let second = tokio::spawn(async move { second_manager.stop(&second_id).await });
    let wait_control = control.clone();
    let duplicate_interrupt = tokio::task::spawn_blocking(move || {
        wait_control.wait_for_interrupts(2, Duration::from_millis(100))
    })
    .await
    .expect("interrupt wait joins");
    let termination_count_before_release = backend.termination_count();
    termination_gate.release();

    assert_eq!(
        first.await.expect("first stop joins").expect("first stop"),
        second
            .await
            .expect("second stop joins")
            .expect("second stop")
    );
    assert!(!duplicate_interrupt, "both stop callers wrote ETX");
    assert_eq!(control.interrupts(), 1);
    assert_eq!(termination_count_before_release, 1);
    assert_eq!(control.kills(), 1);
}

#[tokio::test(start_paused = true)]
async fn aborted_cleanup_owner_does_not_strand_waiters() {
    let backend = Arc::new(FakeBackend::default());
    let control = backend.queue_process();
    control.set_exit_on_interrupt(false);
    let termination_gate = TerminationGate::new();
    backend.set_termination_gate(termination_gate.clone());
    let manager = enabled_manager(Arc::clone(&backend), Arc::new(ManualClock::default())).await;
    let started = manager
        .start(command_request("long", Duration::ZERO), &NeverCancelled)
        .await
        .expect("long command starts");

    let owner_manager = manager.clone();
    let owner_id = started.session_id.clone();
    let owner = tokio::spawn(async move { owner_manager.stop(&owner_id).await });
    tokio::time::advance(Duration::from_secs(2)).await;
    termination_gate.wait_for_entries(1).await;
    owner.abort();
    assert!(
        owner
            .await
            .expect_err("owner caller is aborted")
            .is_cancelled()
    );

    termination_gate.release();
    let receipt = tokio::time::timeout(
        Duration::from_millis(100),
        manager.stop(&started.session_id),
    )
    .await
    .expect("aborted owner must not strand later cleanup waiters")
    .expect("manager-owned cleanup succeeds");
    assert_eq!(receipt.state, ShellSessionState::Stopped);
    assert_eq!(control.interrupts(), 1);
    assert_eq!(backend.termination_count(), 1);
    assert_eq!(control.kills(), 1);
}

#[tokio::test(start_paused = true)]
async fn cleanup_failure_is_sticky_for_stop_disable_and_shutdown() {
    let backend = Arc::new(FakeBackend::default());
    backend.set_termination_error(true);
    let clock = Arc::new(ManualClock::default());
    let manager = enabled_manager(Arc::clone(&backend), Arc::clone(&clock)).await;
    let mut controls = Vec::new();
    let mut session_ids = Vec::new();
    for index in 0..MAX_RUNNING_SHELL_SESSIONS {
        let control = backend.queue_process();
        control.set_exit_on_interrupt(false);
        control.emit(vec![b'x'; MAX_SHELL_UNREAD_BYTES]);
        let started = manager
            .start(
                command_request(&format!("sticky-{index}"), Duration::ZERO),
                &NeverCancelled,
            )
            .await
            .expect("sticky command starts");
        assert_eq!(
            manager.stop(&started.session_id).await,
            Err(ShellManagerError::Indeterminate)
        );
        controls.push(control);
        session_ids.push(started.session_id);
    }

    assert_eq!(
        manager.stop(&session_ids[0]).await,
        Err(ShellManagerError::Indeterminate)
    );
    let expected = ShellCleanupError {
        session_ids: session_ids.clone(),
    };
    assert_eq!(manager.disable_and_stop_all().await, Err(expected.clone()));
    assert_eq!(manager.shutdown().await, Err(expected));
    assert!(controls.iter().all(|control| control.interrupts() == 1));
    assert_eq!(backend.termination_count(), MAX_RUNNING_SHELL_SESSIONS);
    assert!(controls.iter().all(|control| control.kills() == 1));

    clock.advance(TERMINAL_RECEIPT_TTL + Duration::from_millis(1));
    assert_eq!(
        manager.stop(&session_ids[0]).await,
        Err(ShellManagerError::SessionNotFound)
    );

    manager.enable().await;
    backend.queue_fast(&vec![b'f'; 2 * OUTPUT_LIMIT], 0);
    let fresh = manager
        .start(command_request("fresh", Duration::ZERO), &NeverCancelled)
        .await
        .expect("fresh command exits after failed tombstone GC");
    assert_eq!(fresh.output, "f".repeat(OUTPUT_LIMIT));
    assert!(!fresh.output_truncated);
}

#[tokio::test(start_paused = true)]
async fn failed_cleanup_tombstones_are_bounded_by_count() {
    let backend = Arc::new(FakeBackend::default());
    backend.set_termination_error(true);
    let manager = enabled_manager(Arc::clone(&backend), Arc::new(ManualClock::default())).await;
    let mut session_ids = Vec::new();
    for index in 0..=MAX_TERMINAL_SHELL_RECEIPTS {
        let control = backend.queue_process();
        control.set_exit_on_interrupt(false);
        let started = manager
            .start(
                command_request(&format!("failed-{index}"), Duration::ZERO),
                &NeverCancelled,
            )
            .await
            .expect("failed-cleanup command starts");
        assert_eq!(
            manager.stop(&started.session_id).await,
            Err(ShellManagerError::Indeterminate)
        );
        session_ids.push(started.session_id);
    }

    assert_eq!(
        manager.stop(&session_ids[0]).await,
        Err(ShellManagerError::SessionNotFound)
    );
    assert_eq!(
        manager
            .stop(session_ids.last().expect("newest failed tombstone"))
            .await,
        Err(ShellManagerError::Indeterminate)
    );
}

#[tokio::test(start_paused = true)]
async fn try_wait_error_keeps_live_child_owned_and_capacity_reserved() {
    let backend = Arc::new(FakeBackend::default());
    let control = backend.queue_process();
    let manager = enabled_manager(Arc::clone(&backend), Arc::new(ManualClock::default())).await;
    let started = manager
        .start(
            command_request("probe-error", Duration::ZERO),
            &NeverCancelled,
        )
        .await
        .expect("command starts before probe failure");
    control.finish_reader();
    control.set_exit_on_interrupt(false);
    control.set_try_wait_error(true);
    control.set_kill_error(true);
    backend.set_termination_error(true);

    assert_eq!(
        manager
            .poll(
                poll_request(started.session_id.clone(), Duration::ZERO),
                &NeverCancelled,
            )
            .await,
        Err(ShellManagerError::Indeterminate)
    );
    assert!(control.is_running());

    for index in 1..MAX_RUNNING_SHELL_SESSIONS {
        backend.queue_process();
        manager
            .start(
                command_request(&format!("long-{index}"), Duration::ZERO),
                &NeverCancelled,
            )
            .await
            .expect("remaining capacity starts");
    }
    assert_eq!(
        manager
            .start(
                command_request("over-capacity", Duration::ZERO),
                &NeverCancelled,
            )
            .await,
        Err(ShellManagerError::SessionLimit)
    );
    assert_eq!(backend.spawn_count(), MAX_RUNNING_SHELL_SESSIONS);
    assert_eq!(
        manager.disable_and_stop_all().await,
        Err(ShellCleanupError {
            session_ids: vec![started.session_id],
        })
    );
}

#[tokio::test]
async fn blocking_child_kill_timeout_does_not_relock_child() {
    let backend = Arc::new(FakeBackend::default());
    let control = backend.queue_process();
    control.set_exit_on_interrupt(false);
    let kill_gate = KillGate::new();
    control.set_kill_gate(kill_gate.clone());
    let manager = enabled_manager(Arc::clone(&backend), Arc::new(ManualClock::default())).await;
    let started = manager
        .start(
            command_request("blocking-kill", Duration::ZERO),
            &NeverCancelled,
        )
        .await
        .expect("command starts");

    let stop_manager = manager.clone();
    let stop_id = started.session_id.clone();
    let (result_tx, result_rx) = std::sync::mpsc::sync_channel(1);
    let advance_gate = kill_gate.clone();
    let stop_thread = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("stop runtime builds");
        runtime.block_on(async move {
            tokio::time::pause();
            tokio::spawn(async move {
                tokio::time::advance(Duration::from_secs(3)).await;
                advance_gate.wait_until_entered().await;
                tokio::time::advance(Duration::from_secs(2) + Duration::from_millis(1)).await;
            });
            let _ = result_tx.send(stop_manager.stop(&stop_id).await);
        });
    });
    kill_gate.wait_until_entered().await;
    let result_rx = Arc::new(Mutex::new(result_rx));
    let bounded_rx = Arc::clone(&result_rx);
    let bounded = tokio::task::spawn_blocking(move || {
        bounded_rx
            .lock()
            .expect("stop result lock")
            .recv_timeout(Duration::from_millis(500))
    })
    .await
    .expect("bounded stop wait joins");
    let returned_before_release = bounded.is_ok();
    kill_gate.release();
    let result = match bounded {
        Ok(result) => result,
        Err(_) => {
            let released_rx = Arc::clone(&result_rx);
            tokio::task::spawn_blocking(move || {
                released_rx
                    .lock()
                    .expect("stop result lock")
                    .recv_timeout(Duration::from_secs(1))
            })
            .await
            .expect("released stop wait joins")
            .expect("stop returns after releasing blocked kill")
        }
    };
    tokio::task::spawn_blocking(move || stop_thread.join().expect("stop thread joins"))
        .await
        .expect("stop thread join task");

    assert!(
        returned_before_release,
        "stop relocked child after kill timed out"
    );
    assert_eq!(result, Err(ShellManagerError::Indeterminate));
    assert_eq!(
        manager.stop(&started.session_id).await,
        Err(ShellManagerError::Indeterminate)
    );
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
    let output = vec![b'x'; 2 * OUTPUT_LIMIT];
    for index in 0..=MAX_TERMINAL_SHELL_RECEIPTS {
        backend.queue_fast(&output, 0);
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
async fn terminal_gc_releases_evicted_unread_output_budget() {
    let backend = Arc::new(FakeBackend::default());
    let clock = Arc::new(ManualClock::default());
    let manager = enabled_manager(Arc::clone(&backend), Arc::clone(&clock)).await;
    let saturated_output = vec![b'x'; MAX_SHELL_UNREAD_BYTES];
    let mut receipts = Vec::new();
    for index in 0..MAX_RUNNING_SHELL_SESSIONS {
        backend.queue_fast(&saturated_output, 0);
        receipts.push(
            manager
                .start(
                    command_request(&format!("saturate-{index}"), Duration::ZERO),
                    &NeverCancelled,
                )
                .await
                .expect("saturating command exits"),
        );
    }

    clock.advance(TERMINAL_RECEIPT_TTL + Duration::from_millis(1));
    assert_eq!(
        manager
            .poll(
                poll_request(receipts[0].session_id.clone(), Duration::ZERO),
                &NeverCancelled,
            )
            .await,
        Err(ShellManagerError::SessionNotFound)
    );

    backend.queue_fast(&vec![b'f'; 2 * OUTPUT_LIMIT], 0);
    let fresh = manager
        .start(command_request("fresh", Duration::ZERO), &NeverCancelled)
        .await
        .expect("fresh command exits after GC");
    assert_eq!(fresh.output, "f".repeat(OUTPUT_LIMIT));
    assert!(!fresh.output_truncated);
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
