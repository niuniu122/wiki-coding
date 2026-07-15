use std::collections::VecDeque;
use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use minimax_core::{CancellationFuture, CancellationPort};
use minimax_protocol::{ToolCall, ToolEffect, ToolInvocation, ToolTerminalStatus};
use minimax_tools::{
    BoundedProcess, ChildEvent, ChildEventFuture, ChildStopFuture, DirectChild, GitDiffTool,
    GitStatusTool, NeverCancelled, NpmDiagnosticTool, ProcessLauncher, ProcessLimits,
    ProcessRequest, RunDiagnosticTool, WorkspaceRoot,
};
use serde_json::{Value, json};
use tempfile::TempDir;

#[tokio::test]
async fn diagnostic_actions_build_fixed_shell_free_requests() {
    let fixture = Fixture::new();
    must(std::fs::write(fixture.path("check.js"), "const value = 1;"));
    let (process, state) = fake_process(success_steps("ok", "raw-stderr-marker"));
    let tool = RunDiagnosticTool::new(process);

    let cargo = tool
        .execute(
            &fixture.workspace,
            &invocation("run_diagnostic", json!({"action": "cargo_check"})),
            &NeverCancelled,
        )
        .await;
    let cargo_test = tool
        .execute(
            &fixture.workspace,
            &invocation("run_diagnostic", json!({"action": "cargo_test"})),
            &NeverCancelled,
        )
        .await;
    let cargo_clippy = tool
        .execute(
            &fixture.workspace,
            &invocation("run_diagnostic", json!({"action": "cargo_clippy"})),
            &NeverCancelled,
        )
        .await;
    let cargo_fmt = tool
        .execute(
            &fixture.workspace,
            &invocation("run_diagnostic", json!({"action": "cargo_fmt_check"})),
            &NeverCancelled,
        )
        .await;
    let node = tool
        .execute(
            &fixture.workspace,
            &invocation(
                "run_diagnostic",
                json!({"action": "node_check", "path": "check.js"}),
            ),
            &NeverCancelled,
        )
        .await;
    let search = tool
        .execute(
            &fixture.workspace,
            &invocation(
                "run_diagnostic",
                json!({"action": "rg_search", "path": ".", "pattern": "needle", "max_results": 7}),
            ),
            &NeverCancelled,
        )
        .await;
    assert_eq!(cargo.status, ToolTerminalStatus::Succeeded);
    assert_eq!(cargo_test.status, ToolTerminalStatus::Succeeded);
    assert_eq!(cargo_clippy.status, ToolTerminalStatus::Succeeded);
    assert_eq!(cargo_fmt.status, ToolTerminalStatus::Succeeded);
    assert_eq!(node.status, ToolTerminalStatus::Succeeded);
    assert_eq!(search.status, ToolTerminalStatus::Succeeded);

    let requests = state.requests.lock().expect("requests");
    assert_eq!(requests.len(), 6);
    assert_request(
        &requests[0],
        "cargo",
        &["check", "--workspace", "--locked", "--offline"],
        fixture.workspace.as_path(),
    );
    assert_request(
        &requests[1],
        "cargo",
        &["test", "--workspace", "--locked", "--offline"],
        fixture.workspace.as_path(),
    );
    assert_request(
        &requests[2],
        "cargo",
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--locked",
            "--offline",
            "--",
            "-D",
            "warnings",
        ],
        fixture.workspace.as_path(),
    );
    assert_request(
        &requests[3],
        "cargo",
        &["fmt", "--all", "--", "--check"],
        fixture.workspace.as_path(),
    );
    assert_request(
        &requests[4],
        "node",
        &["--check", "--", "check.js"],
        fixture.workspace.as_path(),
    );
    assert_request(
        &requests[5],
        "rg",
        &[
            "--fixed-strings",
            "--line-number",
            "--no-heading",
            "--color",
            "never",
            "--max-count",
            "7",
            "--",
            "needle",
            ".",
        ],
        fixture.workspace.as_path(),
    );
    assert!(requests.iter().all(|request| {
        request.env().get("CARGO_NET_OFFLINE").map(String::as_str) == Some("true")
            && request.env().get("GIT_TERMINAL_PROMPT").map(String::as_str) == Some("0")
            && !request.env().contains_key("OPENAI_API_KEY")
            && !request.env().contains_key("MINIMAX_API_KEY")
            && !request.env().contains_key("RUSTFLAGS")
    }));
    assert!(!must_option(cargo.output.as_deref()).contains("raw-stderr-marker"));
}

#[tokio::test]
async fn malformed_or_command_like_diagnostics_launch_zero_children() {
    let fixture = Fixture::new();
    must(std::fs::write(fixture.path("check.ts"), "const value = 1;"));
    let (process, state) = fake_process(success_steps("ok", ""));
    let tool = RunDiagnosticTool::new(process);
    let cases = [
        json!({"action": "cargo_check", "path": "."}),
        json!({"action": "node_check", "path": "-e"}),
        json!({"action": "node_check", "path": "check.ts"}),
        json!({"action": "rg_search", "path": ".", "pattern": "$(calc)"}),
        json!({"action": "rg_search", "path": ".", "pattern": "--hidden"}),
        json!({"action": "rg_search", "path": ".", "pattern": "FOO=bar"}),
        json!({"action": "shell"}),
        json!({"action": "cargo_test", "flags": ["--package", "outside"]}),
    ];
    for arguments in cases {
        let result = tool
            .execute(
                &fixture.workspace,
                &invocation("run_diagnostic", arguments),
                &NeverCancelled,
            )
            .await;
        assert_ne!(result.status, ToolTerminalStatus::Succeeded);
        assert!(result.output.is_none());
    }
    assert_eq!(state.spawn_count.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn git_requests_are_non_mutating_and_disable_extension_points() {
    let fixture = Fixture::new();
    must(std::fs::write(fixture.path("tracked.txt"), "x"));
    let (process, state) = fake_process(success_steps(" M tracked.txt", "raw-git-stderr"));
    let status_tool = GitStatusTool::new(process.clone());
    let diff_tool = GitDiffTool::new(process);
    let status = status_tool
        .execute(
            &fixture.workspace,
            &invocation("git_status", json!({"path": "tracked.txt"})),
            &NeverCancelled,
        )
        .await;
    let diff = diff_tool
        .execute(
            &fixture.workspace,
            &invocation("git_diff", json!({"cached": true, "path": "."})),
            &NeverCancelled,
        )
        .await;
    assert_eq!(status.status, ToolTerminalStatus::Succeeded);
    assert_eq!(diff.status, ToolTerminalStatus::Succeeded);
    let requests = state.requests.lock().expect("requests");
    assert_eq!(requests.len(), 2);
    for request in requests.iter() {
        assert_eq!(request.program(), "git");
        assert_eq!(request.cwd(), fixture.workspace.as_path());
        let joined = request.args().join(" ");
        assert!(joined.contains("core.hooksPath="));
        assert!(joined.contains("core.pager=cat"));
        assert!(joined.contains("color.ui=false"));
        assert!(!joined.contains(" commit "));
        assert!(!joined.contains(" push "));
        assert!(!joined.contains(" checkout "));
    }
    assert_eq!(
        tail_after(requests[0].args(), "status"),
        [
            "status",
            "--short",
            "--untracked-files=all",
            "--",
            "tracked.txt"
        ]
    );
    assert_eq!(
        tail_after(requests[1].args(), "diff"),
        [
            "diff",
            "--no-color",
            "--no-ext-diff",
            "--no-textconv",
            "--cached",
            "--",
            "."
        ]
    );
    assert!(!must_option(status.output.as_deref()).contains("raw-git-stderr"));
}

#[tokio::test]
async fn npm_accepts_only_existing_safe_diagnostics_without_lifecycle_hooks() {
    let fixture = Fixture::new();
    write_package(
        &fixture,
        json!({"scripts": {"check": "tsc -p tsconfig.json --noEmit"}}),
    );
    let (process, state) = fake_process(success_steps("checked", "raw-npm-stderr"));
    let tool = NpmDiagnosticTool::new(process);
    let safe = tool
        .execute(
            &fixture.workspace,
            &invocation("npm_diagnostic", json!({"script": "check"})),
            &NeverCancelled,
        )
        .await;
    assert_eq!(safe.status, ToolTerminalStatus::Succeeded);
    {
        let requests = state.requests.lock().expect("requests");
        assert_eq!(requests.len(), 1);
        let request = &requests[0];
        let npm_tail = if request.program().to_ascii_lowercase().ends_with("node.exe") {
            assert!(
                request
                    .args()
                    .first()
                    .is_some_and(|value| value.replace('\\', "/").ends_with("npm/bin/npm-cli.js"))
            );
            &request.args()[1..]
        } else {
            assert_eq!(request.program(), "npm");
            request.args()
        };
        assert_eq!(npm_tail, ["run", "check", "--", "--no-color"]);
        assert_eq!(
            request.env().get("npm_config_offline").map(String::as_str),
            Some("true")
        );
    }

    for package in [
        json!({"scripts": {"check": "npm install"}}),
        json!({"scripts": {"check": "curl https://example.com"}}),
        json!({"scripts": {"check": "tsc --noEmit", "precheck": "echo before"}}),
        json!({"scripts": {"build": "tsc --noEmit"}}),
    ] {
        write_package(&fixture, package);
        let result = tool
            .execute(
                &fixture.workspace,
                &invocation("npm_diagnostic", json!({"script": "check"})),
                &NeverCancelled,
            )
            .await;
        assert_ne!(result.status, ToolTerminalStatus::Succeeded);
        assert!(result.output.is_none());
    }
    assert_eq!(state.spawn_count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn combined_output_is_bounded_and_raw_stderr_is_never_returned() {
    let fixture = Fixture::new();
    let (process, state) = fake_process_with(
        vec![
            FakeStep::Event(ChildEvent::Stdout(b"ok\x1b[31m".to_vec())),
            FakeStep::Event(ChildEvent::Stderr(b"RAW_SECRET_STDERR".to_vec())),
            FakeStep::Event(ChildEvent::StdoutClosed),
            FakeStep::Event(ChildEvent::StderrClosed),
            FakeStep::Event(ChildEvent::Exited(Some(0))),
        ],
        must(ProcessLimits::new(Duration::from_secs(1), 64)),
        false,
        false,
    );
    let result = RunDiagnosticTool::new(process)
        .execute(
            &fixture.workspace,
            &invocation("run_diagnostic", json!({"action": "cargo_fmt_check"})),
            &NeverCancelled,
        )
        .await;
    assert_eq!(result.status, ToolTerminalStatus::Succeeded);
    let output = must_option(result.output.as_deref());
    assert!(output.contains("ok[31m"));
    assert!(!output.contains('\x1b'));
    assert!(!output.contains("RAW_SECRET_STDERR"));
    assert_eq!(state.terminate_count.load(Ordering::SeqCst), 0);

    let (overflow_process, overflow_state) = fake_process_with(
        vec![FakeStep::Event(ChildEvent::Stderr(vec![b'x'; 9]))],
        must(ProcessLimits::new(Duration::from_secs(1), 8)),
        false,
        false,
    );
    let overflow = RunDiagnosticTool::new(overflow_process)
        .execute(
            &fixture.workspace,
            &invocation("run_diagnostic", json!({"action": "cargo_fmt_check"})),
            &NeverCancelled,
        )
        .await;
    assert_eq!(overflow.code, "output_limit");
    assert!(overflow.output.is_none());
    assert_eq!(overflow_state.terminate_count.load(Ordering::SeqCst), 1);
    assert_eq!(overflow_state.wait_count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn timeout_cancellation_nonzero_and_cleanup_failure_are_typed() {
    let fixture = Fixture::new();
    let (timeout_process, timeout_state) = fake_process_with(
        vec![FakeStep::Pending],
        must(ProcessLimits::new(Duration::from_millis(5), 64)),
        false,
        false,
    );
    let timeout = RunDiagnosticTool::new(timeout_process)
        .execute(
            &fixture.workspace,
            &invocation("run_diagnostic", json!({"action": "cargo_fmt_check"})),
            &NeverCancelled,
        )
        .await;
    assert_eq!(timeout.code, "timed_out");
    assert_eq!(timeout_state.terminate_count.load(Ordering::SeqCst), 1);
    assert_eq!(timeout_state.wait_count.load(Ordering::SeqCst), 1);

    let (cancel_process, cancel_state) = fake_process_with(
        vec![FakeStep::Pending],
        must(ProcessLimits::new(Duration::from_secs(1), 64)),
        false,
        false,
    );
    let cancelled = RunDiagnosticTool::new(cancel_process)
        .execute(
            &fixture.workspace,
            &invocation("run_diagnostic", json!({"action": "cargo_fmt_check"})),
            &TimedCancellation(Duration::from_millis(2)),
        )
        .await;
    assert_eq!(cancelled.status, ToolTerminalStatus::Cancelled);
    assert_eq!(cancel_state.terminate_count.load(Ordering::SeqCst), 1);
    assert_eq!(cancel_state.wait_count.load(Ordering::SeqCst), 1);

    let (nonzero_process, _) = fake_process_with(
        vec![
            FakeStep::Event(ChildEvent::Stdout(b"diagnostic failed".to_vec())),
            FakeStep::Event(ChildEvent::StdoutClosed),
            FakeStep::Event(ChildEvent::StderrClosed),
            FakeStep::Event(ChildEvent::Exited(Some(3))),
        ],
        ProcessLimits::default(),
        false,
        false,
    );
    let nonzero = RunDiagnosticTool::new(nonzero_process)
        .execute(
            &fixture.workspace,
            &invocation("run_diagnostic", json!({"action": "cargo_fmt_check"})),
            &NeverCancelled,
        )
        .await;
    assert_eq!(nonzero.code, "nonzero_exit");
    assert_eq!(nonzero.status, ToolTerminalStatus::Failed);
    assert!(must_option(nonzero.output.as_deref()).contains("diagnostic failed"));

    let (unknown_process, unknown_state) = fake_process_with(
        vec![FakeStep::Pending],
        must(ProcessLimits::new(Duration::from_millis(5), 64)),
        false,
        true,
    );
    let unknown = RunDiagnosticTool::new(unknown_process)
        .execute(
            &fixture.workspace,
            &invocation("run_diagnostic", json!({"action": "cargo_fmt_check"})),
            &NeverCancelled,
        )
        .await;
    assert_eq!(unknown.status, ToolTerminalStatus::Indeterminate);
    assert_eq!(unknown.code, "cleanup_unknown");
    assert_eq!(unknown_state.terminate_count.load(Ordering::SeqCst), 1);
    assert_eq!(unknown_state.wait_count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn spawn_io_binary_secret_and_preflight_cancellation_fail_closed() {
    let fixture = Fixture::new();
    for (steps, code) in [
        (vec![FakeStep::Error], "process_io"),
        (
            vec![
                FakeStep::Event(ChildEvent::Stdout(vec![0xff])),
                FakeStep::Event(ChildEvent::StdoutClosed),
                FakeStep::Event(ChildEvent::StderrClosed),
                FakeStep::Event(ChildEvent::Exited(Some(0))),
            ],
            "binary_file",
        ),
        (
            success_steps("password=abcdefghijklmnop", ""),
            "secret_content",
        ),
        (success_steps(".env", ""), "secret_content"),
    ] {
        let (process, _) = fake_process_with(steps, ProcessLimits::default(), false, false);
        let result = RunDiagnosticTool::new(process)
            .execute(
                &fixture.workspace,
                &invocation("run_diagnostic", json!({"action": "cargo_fmt_check"})),
                &NeverCancelled,
            )
            .await;
        assert_eq!(result.code, code);
        if code == "secret_content" {
            assert!(result.output.is_none());
        }
        assert!(
            !result
                .output
                .as_deref()
                .unwrap_or_default()
                .contains("password=")
        );
    }

    let (spawn_process, spawn_state) =
        fake_process_with(Vec::new(), ProcessLimits::default(), true, false);
    let spawn = RunDiagnosticTool::new(spawn_process)
        .execute(
            &fixture.workspace,
            &invocation("run_diagnostic", json!({"action": "cargo_fmt_check"})),
            &NeverCancelled,
        )
        .await;
    assert_eq!(spawn.code, "spawn_failed");
    assert_eq!(spawn_state.spawn_count.load(Ordering::SeqCst), 1);

    let (cancel_process, cancel_state) = fake_process(success_steps("ok", ""));
    let cancelled = RunDiagnosticTool::new(cancel_process)
        .execute(
            &fixture.workspace,
            &invocation("run_diagnostic", json!({"action": "cargo_fmt_check"})),
            &AlwaysCancelled,
        )
        .await;
    assert_eq!(cancelled.status, ToolTerminalStatus::Cancelled);
    assert_eq!(cancel_state.spawn_count.load(Ordering::SeqCst), 0);
}

#[derive(Clone)]
enum FakeStep {
    Event(ChildEvent),
    Error,
    Pending,
}

struct FakeState {
    requests: Mutex<Vec<ProcessRequest>>,
    spawn_count: AtomicUsize,
    terminate_count: AtomicUsize,
    wait_count: AtomicUsize,
    template: Vec<FakeStep>,
    spawn_error: bool,
    stop_error: bool,
}

struct FakeLauncher {
    state: Arc<FakeState>,
}

impl ProcessLauncher for FakeLauncher {
    fn spawn(&self, request: &ProcessRequest) -> io::Result<Box<dyn DirectChild>> {
        self.state.spawn_count.fetch_add(1, Ordering::SeqCst);
        self.state
            .requests
            .lock()
            .expect("requests")
            .push(request.clone());
        if self.state.spawn_error {
            return Err(io::Error::other("fixture spawn failure"));
        }
        Ok(Box::new(FakeChild {
            state: Arc::clone(&self.state),
            steps: self.state.template.clone().into(),
        }))
    }
}

struct FakeChild {
    state: Arc<FakeState>,
    steps: VecDeque<FakeStep>,
}

impl DirectChild for FakeChild {
    fn next_event<'a>(&'a mut self) -> ChildEventFuture<'a> {
        let step = self.steps.pop_front().unwrap_or(FakeStep::Pending);
        Box::pin(async move {
            match step {
                FakeStep::Event(event) => Ok(event),
                FakeStep::Error => Err(io::Error::other("fixture read failure")),
                FakeStep::Pending => std::future::pending().await,
            }
        })
    }

    fn terminate_and_wait<'a>(&'a mut self) -> ChildStopFuture<'a> {
        Box::pin(async move {
            self.state.terminate_count.fetch_add(1, Ordering::SeqCst);
            self.state.wait_count.fetch_add(1, Ordering::SeqCst);
            if self.state.stop_error {
                Err(io::Error::other("fixture stop failure"))
            } else {
                Ok(())
            }
        })
    }
}

struct TimedCancellation(Duration);

impl CancellationPort for TimedCancellation {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn cancelled<'a>(&'a self) -> CancellationFuture<'a> {
        Box::pin(tokio::time::sleep(self.0))
    }
}

struct AlwaysCancelled;

impl CancellationPort for AlwaysCancelled {
    fn is_cancelled(&self) -> bool {
        true
    }

    fn cancelled<'a>(&'a self) -> CancellationFuture<'a> {
        Box::pin(std::future::ready(()))
    }
}

struct Fixture {
    _directory: TempDir,
    workspace: WorkspaceRoot,
}

impl Fixture {
    fn new() -> Self {
        let directory = must(TempDir::new());
        let workspace = must(WorkspaceRoot::new(directory.path()));
        Self {
            _directory: directory,
            workspace,
        }
    }

    fn path(&self, relative: &str) -> std::path::PathBuf {
        self.workspace.as_path().join(relative)
    }
}

fn fake_process(steps: Vec<FakeStep>) -> (BoundedProcess, Arc<FakeState>) {
    fake_process_with(steps, ProcessLimits::default(), false, false)
}

fn fake_process_with(
    steps: Vec<FakeStep>,
    limits: ProcessLimits,
    spawn_error: bool,
    stop_error: bool,
) -> (BoundedProcess, Arc<FakeState>) {
    let state = Arc::new(FakeState {
        requests: Mutex::new(Vec::new()),
        spawn_count: AtomicUsize::new(0),
        terminate_count: AtomicUsize::new(0),
        wait_count: AtomicUsize::new(0),
        template: steps,
        spawn_error,
        stop_error,
    });
    let process = BoundedProcess::new(
        Arc::new(FakeLauncher {
            state: Arc::clone(&state),
        }),
        limits,
    );
    (process, state)
}

fn success_steps(stdout: &str, stderr: &str) -> Vec<FakeStep> {
    vec![
        FakeStep::Event(ChildEvent::Stdout(stdout.as_bytes().to_vec())),
        FakeStep::Event(ChildEvent::Stderr(stderr.as_bytes().to_vec())),
        FakeStep::Event(ChildEvent::StdoutClosed),
        FakeStep::Event(ChildEvent::StderrClosed),
        FakeStep::Event(ChildEvent::Exited(Some(0))),
    ]
}

fn invocation(name: &str, arguments: Value) -> ToolInvocation {
    let call = must(ToolCall::new(
        must(minimax_protocol::ToolCallId::new(format!("call-{name}"))),
        name,
        must(serde_json::to_string(&arguments)),
    ));
    must(ToolInvocation::new(call, ToolEffect::Process))
}

fn assert_request(request: &ProcessRequest, program: &str, args: &[&str], cwd: &std::path::Path) {
    assert_eq!(request.program(), program);
    assert_eq!(
        request.args(),
        args.iter()
            .map(|value| (*value).to_owned())
            .collect::<Vec<_>>()
    );
    assert_eq!(request.cwd(), cwd);
}

fn tail_after<'a>(args: &'a [String], needle: &str) -> &'a [String] {
    let index = args
        .iter()
        .position(|value| value == needle)
        .unwrap_or_else(|| panic!("missing {needle}"));
    &args[index..]
}

fn write_package(fixture: &Fixture, value: Value) {
    must(std::fs::write(
        fixture.path("package.json"),
        must(serde_json::to_vec(&value)),
    ));
}

fn must<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("unexpected error: {error:?}"),
    }
}

fn must_option<T>(value: Option<T>) -> T {
    match value {
        Some(value) => value,
        None => panic!("expected value"),
    }
}
