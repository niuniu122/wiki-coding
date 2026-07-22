use std::collections::VecDeque;
use std::io::{self, Cursor, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use minimax_core::{
    FixedClock, InvocationEffect, InvocationInput, InvocationMachine, PermissionMode,
    ToolExecutionContext, ToolPort,
};
use minimax_protocol::{
    MAX_SHELL_COMMAND_BYTES, MAX_SHELL_OUTPUT_BYTES, MAX_TOOL_RESULT_BYTES, ShellReceipt,
    ShellSessionId, ShellSessionState, ToolCall, ToolEffect, ToolInvocation, ToolTerminalStatus,
};
use minimax_tools::{
    BoundedProcess, BuiltinToolPort, MAX_RUNNING_SHELL_SESSIONS, NeverCancelled, Preflight,
    ShellBackend, ShellChild, ShellIoMode, ShellManagerError, ShellSessionIdSource,
    ShellSessionManager, ShellSpawnRequest, ShellTerminateFuture, SpawnedShell,
};
use serde_json::{Value, json};
use tempfile::TempDir;

#[tokio::test]
async fn confirm_forged_shell_is_rejected_before_approval_or_process_work() {
    let fixture = Fixture::new().await;
    let invocation = invocation(
        "shell_command",
        ToolEffect::Process,
        json!({"command": "Write-Output forged"}),
    );
    let result = must_error(fixture.port.preflight(
        &invocation,
        context(PermissionMode::Confirm),
        &NeverCancelled,
    ));

    assert_eq!(result.status, ToolTerminalStatus::Rejected);
    assert_eq!(result.code, "shell_requires_full_access");
    assert_eq!(fixture.backend.spawn_count(), 0);

    let (mut machine, _) = InvocationMachine::request(invocation);
    let effects = must(machine.apply(InvocationInput::PreflightDenied { result }));
    assert!(effects.iter().all(|effect| {
        !matches!(
            effect,
            InvocationEffect::RequestApproval(_) | InvocationEffect::Execute { .. }
        )
    }));
    assert_eq!(fixture.backend.spawn_count(), 0);

    let direct = fixture
        .port
        .execute(
            machine.invocation(),
            context(PermissionMode::Confirm),
            &NeverCancelled,
        )
        .await;
    assert_eq!(direct.status, ToolTerminalStatus::Rejected);
    assert_eq!(direct.code, "shell_requires_full_access");
    assert_eq!(fixture.backend.request_count(), 0);
}

#[test]
fn full_access_shell_preflight_keeps_schema_and_effect_but_skips_old_content_gates() {
    let outside = tempfile::tempdir().expect("outside cwd fixture");
    let dangerous = invocation(
        "shell_command",
        ToolEffect::Process,
        json!({
            "command": "Get-Content .env | Set-Content redirected.txt; Write-Output 'password=abcdefghijklmnop'; curl https://example.com > response.txt",
            "cwd": outside.path().to_string_lossy(),
            "yield_time_ms": 250,
            "max_output_bytes": 1024
        }),
    );
    assert!(
        Preflight::check_with_context(
            &dangerous,
            context(PermissionMode::FullAccess),
            &NeverCancelled,
        )
        .is_ok()
    );

    let wrong_effect = invocation(
        "shell_command",
        ToolEffect::Read,
        json!({"command": "Write-Output harmless"}),
    );
    assert_eq!(
        must_error(Preflight::check_with_context(
            &wrong_effect,
            context(PermissionMode::FullAccess),
            &NeverCancelled,
        ))
        .code()
        .as_str(),
        "effect_mismatch"
    );
}

#[tokio::test]
async fn shell_command_rejects_blank_and_oversized_commands_with_exact_codes() {
    let fixture = Fixture::new().await;
    let blank = invoke(
        &fixture.port,
        invocation(
            "shell_command",
            ToolEffect::Process,
            json!({"command": "   "}),
        ),
    )
    .await;
    assert_eq!(blank.status, ToolTerminalStatus::Rejected);
    assert_eq!(blank.code, "invalid_arguments");
    assert_eq!(fixture.backend.spawn_count(), 0);

    let oversized = invocation(
        "shell_command",
        ToolEffect::Process,
        json!({"command": "x".repeat(MAX_SHELL_COMMAND_BYTES + 1)}),
    );
    let result = must_error(fixture.port.preflight(
        &oversized,
        context(PermissionMode::FullAccess),
        &NeverCancelled,
    ));
    assert_eq!(result.status, ToolTerminalStatus::Rejected);
    assert_eq!(result.code, "input_limit");
    assert_eq!(fixture.backend.spawn_count(), 0);
}

#[tokio::test]
async fn shell_command_defaults_to_pipe_and_accepts_explicit_terminal_mode() {
    let fixture = Fixture::new().await;
    fixture.backend.queue_exited(b"pipe".to_vec(), 0);
    fixture.backend.queue_exited(b"terminal".to_vec(), 0);

    let first = invoke(
        &fixture.port,
        invocation(
            "shell_command",
            ToolEffect::Process,
            json!({"command": "first"}),
        ),
    )
    .await;
    let second = invoke(
        &fixture.port,
        invocation(
            "shell_command",
            ToolEffect::Process,
            json!({"command": "second", "tty": true}),
        ),
    )
    .await;

    assert_eq!(first.code, "shell_exited");
    assert_eq!(second.code, "shell_exited");
    assert_eq!(
        fixture.backend.request_modes(),
        vec![
            ShellIoMode::Pipe,
            ShellIoMode::Terminal {
                cols: 120,
                rows: 30,
            },
        ]
    );
}

#[tokio::test]
async fn shell_command_rejects_non_boolean_tty_before_manager_work() {
    let fixture = Fixture::new().await;
    let result = invoke(
        &fixture.port,
        invocation(
            "shell_command",
            ToolEffect::Process,
            json!({"command": "never", "tty": "yes"}),
        ),
    )
    .await;

    assert_eq!(result.status, ToolTerminalStatus::Rejected);
    assert_eq!(result.code, "invalid_arguments");
    assert_eq!(fixture.backend.spawn_count(), 0);
}

#[tokio::test]
async fn shell_session_action_combinations_are_strict_before_manager_work() {
    let fixture = Fixture::new().await;
    for arguments in [
        json!({"session_id": "shell-test-0001", "action": "poll", "input": "ignored"}),
        json!({"session_id": "shell-test-0001", "action": "poll", "submit": false}),
        json!({"session_id": "shell-test-0001", "action": "write"}),
        json!({"session_id": "shell-test-0001", "action": "write", "input": "", "submit": false}),
        json!({"session_id": "shell-test-0001", "action": "stop", "input": "ignored"}),
        json!({"session_id": "shell-test-0001", "action": "stop", "submit": false}),
        json!({"session_id": "shell-test-0001", "action": "stop", "yield_time_ms": 1}),
    ] {
        let result = invoke(
            &fixture.port,
            invocation("shell_session", ToolEffect::Process, arguments),
        )
        .await;
        assert_eq!(result.status, ToolTerminalStatus::Rejected);
        assert_eq!(result.code, "invalid_arguments");
    }
    assert_eq!(fixture.backend.spawn_count(), 0);
    assert_eq!(fixture.backend.termination_count(), 0);
}

#[tokio::test]
async fn command_cwd_defaults_to_workspace_resolves_relative_and_allows_outside_absolute() {
    let fixture = Fixture::new().await;
    let relative = fixture.root.path().join("nested");
    std::fs::create_dir(&relative).expect("relative cwd fixture");

    for arguments in [
        json!({"command": "default"}),
        json!({"command": "relative", "cwd": "nested"}),
        json!({"command": "outside", "cwd": fixture.outside.path().to_string_lossy()}),
    ] {
        fixture.backend.queue_exited("done", 0);
        let result = invoke(
            &fixture.port,
            invocation("shell_command", ToolEffect::Process, arguments),
        )
        .await;
        assert_eq!(result.code, "shell_exited");
    }

    assert_eq!(
        fixture.backend.request_cwds(),
        [
            canonical(fixture.root.path()),
            canonical(&relative),
            canonical(fixture.outside.path()),
        ]
    );
}

#[tokio::test]
async fn command_and_session_receipts_map_state_and_exit_exactly() {
    let fixture = Fixture::new().await;

    fixture.backend.queue_running("ready");
    let running = invoke(
        &fixture.port,
        invocation(
            "shell_command",
            ToolEffect::Process,
            json!({"command": "long", "yield_time_ms": 250}),
        ),
    )
    .await;
    assert_receipt(
        &running,
        ToolTerminalStatus::Succeeded,
        "shell_running",
        ShellSessionState::Running,
        None,
    );

    fixture.backend.queue_exited("ok", 0);
    let exited = invoke(
        &fixture.port,
        invocation(
            "shell_command",
            ToolEffect::Process,
            json!({"command": "fast"}),
        ),
    )
    .await;
    assert_receipt(
        &exited,
        ToolTerminalStatus::Succeeded,
        "shell_exited",
        ShellSessionState::Exited,
        Some(0),
    );

    fixture.backend.queue_exited("failed", 7);
    let nonzero = invoke(
        &fixture.port,
        invocation(
            "shell_command",
            ToolEffect::Process,
            json!({"command": "fail"}),
        ),
    )
    .await;
    assert_receipt(
        &nonzero,
        ToolTerminalStatus::Failed,
        "shell_nonzero_exit",
        ShellSessionState::Exited,
        Some(7),
    );

    fixture.backend.queue_launch_failure();
    let launch = invoke(
        &fixture.port,
        invocation(
            "shell_command",
            ToolEffect::Process,
            json!({"command": "missing-shell"}),
        ),
    )
    .await;
    assert_eq!(launch.status, ToolTerminalStatus::Failed);
    assert_eq!(launch.code, "shell_launch_failed");
    assert!(launch.output.is_none());

    let running_receipt: ShellReceipt =
        must(serde_json::from_str(must_option(running.output.as_deref())));
    let stopped = invoke(
        &fixture.port,
        invocation(
            "shell_session",
            ToolEffect::Process,
            json!({"session_id": running_receipt.session_id, "action": "stop"}),
        ),
    )
    .await;
    assert_receipt(
        &stopped,
        ToolTerminalStatus::Succeeded,
        "shell_stopped",
        ShellSessionState::Stopped,
        Some(-2),
    );
}

#[tokio::test]
async fn unknown_and_ninth_sessions_use_exact_codes_without_extra_spawn() {
    let fixture = Fixture::new().await;
    let unknown = invoke(
        &fixture.port,
        invocation(
            "shell_session",
            ToolEffect::Process,
            json!({"session_id": "shell-old-0001", "action": "poll", "yield_time_ms": 0}),
        ),
    )
    .await;
    assert_eq!(unknown.status, ToolTerminalStatus::Rejected);
    assert_eq!(unknown.code, "shell_session_not_found");

    for index in 0..MAX_RUNNING_SHELL_SESSIONS {
        fixture.backend.queue_running(format!("ready-{index}"));
        let result = invoke(
            &fixture.port,
            invocation(
                "shell_command",
                ToolEffect::Process,
                json!({"command": format!("long-{index}"), "yield_time_ms": 250}),
            ),
        )
        .await;
        assert_eq!(result.code, "shell_running");
    }

    let ninth = invoke(
        &fixture.port,
        invocation(
            "shell_command",
            ToolEffect::Process,
            json!({"command": "ninth", "yield_time_ms": 250}),
        ),
    )
    .await;
    assert_eq!(ninth.status, ToolTerminalStatus::Rejected);
    assert_eq!(ninth.code, "shell_session_limit");
    assert_eq!(fixture.backend.spawn_count(), MAX_RUNNING_SHELL_SESSIONS);
}

#[tokio::test]
async fn maximum_shell_output_serializes_within_the_protocol_result_limit() {
    let fixture = Fixture::new().await;
    fixture
        .backend
        .queue_exited("x".repeat(MAX_SHELL_OUTPUT_BYTES), 0);
    let result = invoke(
        &fixture.port,
        invocation(
            "shell_command",
            ToolEffect::Process,
            json!({"command": "maximum-output", "max_output_bytes": MAX_SHELL_OUTPUT_BYTES}),
        ),
    )
    .await;

    assert_eq!(result.code, "shell_exited");
    assert!(must_option(result.output.as_ref()).len() <= MAX_TOOL_RESULT_BYTES);
    must(result.validate());
}

#[tokio::test]
async fn json_escaped_output_is_chunked_without_losing_unreturned_bytes() {
    let fixture = Fixture::new().await;
    let expected = "\n\r\t\"\\".repeat(20_000);
    fixture.backend.queue_running(expected.clone());
    let started = invoke(
        &fixture.port,
        invocation(
            "shell_command",
            ToolEffect::Process,
            json!({
                "command": "escaped-output",
                "yield_time_ms": 250,
                "max_output_bytes": MAX_SHELL_OUTPUT_BYTES
            }),
        ),
    )
    .await;

    assert_eq!(started.code, "shell_running");
    must(started.clone().validate());
    let first = receipt(&started);
    assert!(!first.output_truncated);
    let mut delivered = first.output;

    for _ in 0..20 {
        if delivered.len() == expected.len() {
            break;
        }
        let polled = invoke(
            &fixture.port,
            invocation(
                "shell_session",
                ToolEffect::Process,
                json!({
                    "session_id": first.session_id,
                    "action": "poll",
                    "yield_time_ms": 100,
                    "max_output_bytes": MAX_SHELL_OUTPUT_BYTES
                }),
            ),
        )
        .await;
        assert_eq!(polled.code, "shell_running");
        must(polled.clone().validate());
        let receipt = receipt(&polled);
        assert!(!receipt.output_truncated);
        delivered.push_str(&receipt.output);
    }

    assert_eq!(delivered, expected);
}

#[tokio::test]
async fn stop_honors_explicit_output_limit_and_preserves_the_remainder_for_poll() {
    let fixture = Fixture::new().await;
    fixture.backend.queue_running("x".repeat(50_000));
    let started = invoke(
        &fixture.port,
        invocation(
            "shell_command",
            ToolEffect::Process,
            json!({"command": "explicit-stop-limit", "max_output_bytes": 1024}),
        ),
    )
    .await;
    let started = receipt(&started);
    assert_eq!(started.output.len(), 1024);

    let stopped = invoke(
        &fixture.port,
        invocation(
            "shell_session",
            ToolEffect::Process,
            json!({
                "session_id": started.session_id,
                "action": "stop",
                "max_output_bytes": 1024
            }),
        ),
    )
    .await;
    assert_eq!(stopped.code, "shell_stopped");
    let stopped = receipt(&stopped);
    assert_eq!(stopped.output.len(), 1024);

    let remaining = invoke(
        &fixture.port,
        invocation(
            "shell_session",
            ToolEffect::Process,
            json!({
                "session_id": stopped.session_id,
                "action": "poll",
                "yield_time_ms": 0,
                "max_output_bytes": MAX_SHELL_OUTPUT_BYTES
            }),
        ),
    )
    .await;
    assert_eq!(remaining.code, "shell_stopped");
    assert_eq!(receipt(&remaining).output.len(), 50_000 - 2 * 1024);
}

#[tokio::test]
async fn stop_uses_the_default_sixteen_kib_output_limit() {
    let fixture = Fixture::new().await;
    fixture.backend.queue_running("d".repeat(50_000));
    let started = invoke(
        &fixture.port,
        invocation(
            "shell_command",
            ToolEffect::Process,
            json!({"command": "default-stop-limit", "max_output_bytes": 1024}),
        ),
    )
    .await;
    let started = receipt(&started);

    let stopped = invoke(
        &fixture.port,
        invocation(
            "shell_session",
            ToolEffect::Process,
            json!({"session_id": started.session_id, "action": "stop"}),
        ),
    )
    .await;
    assert_eq!(stopped.code, "shell_stopped");
    assert_eq!(receipt(&stopped).output.len(), 16 * 1024);
}

struct Fixture {
    root: TempDir,
    outside: TempDir,
    backend: Arc<FakeBackend>,
    port: BuiltinToolPort,
}

impl Fixture {
    async fn new() -> Self {
        let root = tempfile::tempdir().expect("workspace fixture");
        let outside = tempfile::tempdir().expect("outside fixture");
        let backend = Arc::new(FakeBackend::default());
        let manager = ShellSessionManager::new(
            backend.clone(),
            Arc::new(TestIds::default()),
            Arc::new(FixedClock::new(0)),
        );
        manager.enable().await;
        let port = must(BuiltinToolPort::with_shell_manager(
            root.path(),
            BoundedProcess::production(),
            manager,
        ));
        Self {
            root,
            outside,
            backend,
            port,
        }
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

enum Plan {
    Process {
        output: Vec<u8>,
        exit_code: Option<i32>,
    },
    LaunchFailure,
}

#[derive(Default)]
struct FakeBackend {
    plans: Mutex<VecDeque<Plan>>,
    requests: Mutex<Vec<ShellSpawnRequest>>,
    next_process_id: AtomicU32,
    spawns: AtomicUsize,
    terminations: Arc<AtomicUsize>,
}

impl FakeBackend {
    fn queue_running(&self, output: impl Into<Vec<u8>>) {
        self.plans
            .lock()
            .expect("plans lock")
            .push_back(Plan::Process {
                output: output.into(),
                exit_code: None,
            });
    }

    fn queue_exited(&self, output: impl Into<Vec<u8>>, exit_code: i32) {
        self.plans
            .lock()
            .expect("plans lock")
            .push_back(Plan::Process {
                output: output.into(),
                exit_code: Some(exit_code),
            });
    }

    fn queue_launch_failure(&self) {
        self.plans
            .lock()
            .expect("plans lock")
            .push_back(Plan::LaunchFailure);
    }

    fn spawn_count(&self) -> usize {
        self.spawns.load(Ordering::Acquire)
    }

    fn termination_count(&self) -> usize {
        self.terminations.load(Ordering::Acquire)
    }

    fn request_cwds(&self) -> Vec<PathBuf> {
        self.requests
            .lock()
            .expect("requests lock")
            .iter()
            .map(|request| request.cwd.clone())
            .collect()
    }

    fn request_modes(&self) -> Vec<ShellIoMode> {
        self.requests
            .lock()
            .expect("requests lock")
            .iter()
            .map(|request| request.io_mode)
            .collect()
    }

    fn request_count(&self) -> usize {
        self.requests.lock().expect("requests lock").len()
    }
}

impl ShellBackend for FakeBackend {
    fn spawn(&self, request: &ShellSpawnRequest) -> io::Result<SpawnedShell> {
        self.requests
            .lock()
            .expect("requests lock")
            .push(request.clone());
        let plan = self
            .plans
            .lock()
            .expect("plans lock")
            .pop_front()
            .ok_or_else(|| io::Error::other("no scripted process"))?;
        let Plan::Process { output, exit_code } = plan else {
            return Err(io::Error::other("scripted launch failure"));
        };
        let process_id = self.next_process_id.fetch_add(1, Ordering::AcqRel) + 1;
        let state = Arc::new(Mutex::new(exit_code));
        self.spawns.fetch_add(1, Ordering::AcqRel);
        Ok(SpawnedShell {
            child: Box::new(FakeChild {
                process_id,
                state: Arc::clone(&state),
            }),
            reader: Box::new(Cursor::new(output)),
            writer: Box::new(FakeWriter {
                state: Arc::clone(&state),
            }),
            guard: Box::new(FakeGuard {
                state,
                terminations: Arc::clone(&self.terminations),
            }),
        })
    }
}

struct FakeGuard {
    state: Arc<Mutex<Option<i32>>>,
    terminations: Arc<AtomicUsize>,
}

impl minimax_tools::ShellGuard for FakeGuard {
    fn terminate<'a>(&'a mut self) -> ShellTerminateFuture<'a> {
        Box::pin(async move {
            self.terminations.fetch_add(1, Ordering::AcqRel);
            *self.state.lock().expect("process state lock") = Some(-15);
            Ok(())
        })
    }

    fn confirm<'a>(&'a mut self) -> ShellTerminateFuture<'a> {
        Box::pin(async { Ok(()) })
    }
}

struct FakeChild {
    process_id: u32,
    state: Arc<Mutex<Option<i32>>>,
}

impl ShellChild for FakeChild {
    fn process_id(&self) -> u32 {
        self.process_id
    }

    fn try_wait(&mut self) -> io::Result<Option<i32>> {
        Ok(*self.state.lock().expect("process state lock"))
    }

    fn kill(&mut self) -> io::Result<()> {
        *self.state.lock().expect("process state lock") = Some(-9);
        Ok(())
    }
}

struct FakeWriter {
    state: Arc<Mutex<Option<i32>>>,
}

impl Write for FakeWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes == b"\x03" {
            *self.state.lock().expect("process state lock") = Some(-2);
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn context(mode: PermissionMode) -> ToolExecutionContext {
    ToolExecutionContext::for_permission_mode(mode)
}

async fn invoke(
    port: &BuiltinToolPort,
    invocation: ToolInvocation,
) -> minimax_protocol::ToolResult {
    let context = context(PermissionMode::FullAccess);
    if let Err(result) = port.preflight(&invocation, context, &NeverCancelled) {
        return result;
    }
    port.execute(&invocation, context, &NeverCancelled).await
}

fn invocation(name: &str, effect: ToolEffect, arguments: Value) -> ToolInvocation {
    let call = must(ToolCall::new(
        must(minimax_protocol::ToolCallId::new("call-shell")),
        name,
        must(serde_json::to_string(&arguments)),
    ));
    must(ToolInvocation::new(call, effect))
}

fn assert_receipt(
    result: &minimax_protocol::ToolResult,
    status: ToolTerminalStatus,
    code: &str,
    state: ShellSessionState,
    exit_code: Option<i32>,
) {
    assert_eq!(result.status, status);
    assert_eq!(result.code, code);
    let receipt: ShellReceipt = must(serde_json::from_str(must_option(result.output.as_deref())));
    assert_eq!(receipt.state, state);
    assert_eq!(receipt.exit_code, exit_code);
}

fn receipt(result: &minimax_protocol::ToolResult) -> ShellReceipt {
    must(serde_json::from_str(must_option(result.output.as_deref())))
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).expect("canonical fixture path")
}

fn must<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("unexpected error: {error:?}"),
    }
}

fn must_error<T, E: std::fmt::Debug>(result: Result<T, E>) -> E {
    match result {
        Ok(_) => panic!("expected error"),
        Err(error) => error,
    }
}

fn must_option<T>(value: Option<T>) -> T {
    match value {
        Some(value) => value,
        None => panic!("expected value"),
    }
}
