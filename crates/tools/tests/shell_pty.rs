use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use minimax_protocol::{
    MAX_SHELL_OUTPUT_BYTES, ShellReceipt, ShellSessionState, ToolCall, ToolCallId, ToolEffect,
    ToolInvocation,
};
use minimax_tools::{
    NativePtyBackend, NeverCancelled, ProcessShellSessionIds, ShellCommandRequest,
    ShellCommandTool, ShellManagerError, ShellPollRequest, ShellSessionManager, ShellWriteRequest,
    SystemShellClock, WorkspaceRoot,
};

const TEST_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone, Copy)]
enum FixtureKind {
    Fast,
    Nonzero,
    Long,
    Prompt,
    Tty,
}

#[cfg(windows)]
fn command_fixture(kind: FixtureKind) -> &'static str {
    match kind {
        FixtureKind::Fast => "Write-Output 'fast-ready'",
        FixtureKind::Nonzero => "Write-Output 'failed'; exit 7",
        FixtureKind::Long => "Write-Output 'first'; Start-Sleep -Seconds 2; Write-Output 'second'",
        FixtureKind::Prompt => "$v = Read-Host 'value'; Write-Output \"got:$v\"",
        FixtureKind::Tty => {
            "Write-Output \"in=$(-not [Console]::IsInputRedirected);out=$(-not [Console]::IsOutputRedirected)\""
        }
    }
}

#[cfg(target_os = "linux")]
fn command_fixture(kind: FixtureKind) -> &'static str {
    match kind {
        FixtureKind::Fast => "printf 'fast-ready\\n'",
        FixtureKind::Nonzero => "printf 'failed\\n'; exit 7",
        FixtureKind::Long => "printf 'first\\n'; sleep 2; printf 'second\\n'",
        FixtureKind::Prompt => "printf 'value: '; read v; printf 'got:%s\\n' \"$v\"",
        FixtureKind::Tty => "test -t 0 && test -t 1 && printf 'in=true;out=true\\n'",
    }
}

fn native_manager() -> ShellSessionManager {
    ShellSessionManager::new(
        Arc::new(NativePtyBackend),
        Arc::new(ProcessShellSessionIds::new().expect("process shell session IDs")),
        Arc::new(SystemShellClock),
    )
}

fn command_request(
    command: impl Into<String>,
    cwd: &Path,
    yield_time: Duration,
) -> ShellCommandRequest {
    ShellCommandRequest {
        command: command.into(),
        cwd: PathBuf::from(cwd),
        yield_time,
        max_output_bytes: MAX_SHELL_OUTPUT_BYTES,
    }
}

async fn start_command(
    manager: &ShellSessionManager,
    command: impl Into<String>,
    cwd: &Path,
    yield_time: Duration,
) -> Result<minimax_protocol::ShellReceipt, String> {
    tokio::time::timeout(
        TEST_TIMEOUT,
        manager.start(command_request(command, cwd, yield_time), &NeverCancelled),
    )
    .await
    .map_err(|_| "shell start timed out".to_owned())?
    .map_err(|error| format!("shell start failed: {error:?}"))
}

async fn poll_session(
    manager: &ShellSessionManager,
    session_id: minimax_protocol::ShellSessionId,
    yield_time: Duration,
) -> Result<minimax_protocol::ShellReceipt, String> {
    tokio::time::timeout(
        TEST_TIMEOUT,
        manager.poll(
            ShellPollRequest {
                session_id,
                yield_time,
                max_output_bytes: MAX_SHELL_OUTPUT_BYTES,
            },
            &NeverCancelled,
        ),
    )
    .await
    .map_err(|_| "shell poll timed out".to_owned())?
    .map_err(|error| format!("shell poll failed: {error:?}"))
}

async fn settle_session(
    manager: &ShellSessionManager,
    first: minimax_protocol::ShellReceipt,
) -> Result<(minimax_protocol::ShellReceipt, String), String> {
    let mut receipt = first;
    let mut output = receipt.output.clone();
    let deadline = tokio::time::Instant::now() + TEST_TIMEOUT;
    while receipt.state == ShellSessionState::Running {
        if tokio::time::Instant::now() >= deadline {
            return Err("shell session did not reach a terminal state".to_owned());
        }
        receipt = poll_session(
            manager,
            receipt.session_id.clone(),
            Duration::from_millis(500),
        )
        .await?;
        output.push_str(&receipt.output);
    }
    Ok((receipt, output))
}

async fn cleanup(manager: &ShellSessionManager) -> Result<(), String> {
    tokio::time::timeout(TEST_TIMEOUT, manager.shutdown())
        .await
        .map_err(|_| "shell cleanup timed out".to_owned())?
        .map_err(|error| format!("shell cleanup failed: {error:?}"))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fast_command_returns_terminal_output_and_exit_zero() {
    let manager = native_manager();
    manager.enable().await;
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("repository root");

    let receipt = start_command(
        &manager,
        command_fixture(FixtureKind::Fast),
        root,
        Duration::from_secs(5),
    )
    .await;
    let cleanup = cleanup(&manager).await;

    cleanup.expect("cleanup succeeds");
    let receipt = receipt.expect("fast command launches");
    assert_eq!(receipt.state, ShellSessionState::Exited);
    assert_eq!(receipt.exit_code, Some(0));
    assert!(receipt.output.contains("fast-ready"), "{receipt:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn nonzero_command_preserves_exit_seven_and_output() {
    let manager = native_manager();
    manager.enable().await;
    let root = repository_root();

    let first = start_command(
        &manager,
        command_fixture(FixtureKind::Nonzero),
        &root,
        Duration::from_secs(5),
    )
    .await;
    let settled = match first {
        Ok(receipt) => settle_session(&manager, receipt).await,
        Err(error) => Err(error),
    };
    let cleanup = cleanup(&manager).await;

    cleanup.expect("cleanup succeeds");
    let (receipt, output) = settled.expect("nonzero command settles");
    assert_eq!(receipt.state, ShellSessionState::Exited);
    assert_eq!(receipt.exit_code, Some(7));
    assert!(output.contains("failed"), "{output:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn long_command_polling_delivers_only_incremental_output() {
    let manager = native_manager();
    manager.enable().await;
    let root = repository_root();

    let first = start_command(
        &manager,
        command_fixture(FixtureKind::Long),
        &root,
        Duration::from_millis(250),
    )
    .await;
    let first_snapshot = first.as_ref().ok().cloned();
    let settled = match first {
        Ok(receipt) => settle_session(&manager, receipt).await,
        Err(error) => Err(error),
    };
    let cleanup = cleanup(&manager).await;

    cleanup.expect("cleanup succeeds");
    let first = first_snapshot.expect("long command launches");
    let (terminal, output) = settled.expect("long command settles");
    assert_eq!(first.state, ShellSessionState::Running);
    assert_eq!(terminal.state, ShellSessionState::Exited);
    assert_eq!(terminal.exit_code, Some(0));
    assert_eq!(output.matches("first").count(), 1, "{output:?}");
    assert_eq!(output.matches("second").count(), 1, "{output:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn prompt_receives_write_and_submit_then_exits() {
    let manager = native_manager();
    manager.enable().await;
    let root = repository_root();

    let first = start_command(
        &manager,
        command_fixture(FixtureKind::Prompt),
        &root,
        Duration::from_millis(500),
    )
    .await;
    let written = match first {
        Ok(receipt) => tokio::time::timeout(
            TEST_TIMEOUT,
            manager.write(
                ShellWriteRequest {
                    session_id: receipt.session_id,
                    input: "codex-input".to_owned(),
                    submit: true,
                    yield_time: Duration::from_secs(3),
                    max_output_bytes: MAX_SHELL_OUTPUT_BYTES,
                },
                &NeverCancelled,
            ),
        )
        .await
        .map_err(|_| "shell write timed out".to_owned())
        .and_then(|result| result.map_err(|error| format!("shell write failed: {error:?}"))),
        Err(error) => Err(error),
    };
    let settled = match written {
        Ok(receipt) => settle_session(&manager, receipt).await,
        Err(error) => Err(error),
    };
    let cleanup = cleanup(&manager).await;

    cleanup.expect("cleanup succeeds");
    let (terminal, output) = settled.expect("prompt command settles");
    assert_eq!(terminal.state, ShellSessionState::Exited);
    assert_eq!(terminal.exit_code, Some(0));
    assert!(output.contains("got:codex-input"), "{output:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_shell_observes_terminal_stdin_and_stdout() {
    let manager = native_manager();
    manager.enable().await;
    let root = repository_root();

    let first = start_command(
        &manager,
        command_fixture(FixtureKind::Tty),
        &root,
        Duration::from_secs(5),
    )
    .await;
    let settled = match first {
        Ok(receipt) => settle_session(&manager, receipt).await,
        Err(error) => Err(error),
    };
    let cleanup = cleanup(&manager).await;

    cleanup.expect("cleanup succeeds");
    let (terminal, output) = settled.expect("TTY command settles");
    assert_eq!(terminal.exit_code, Some(0));
    assert!(output.contains(tty_expected_output()), "{output:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unicode_emoji_and_native_pipe_redirection_round_trip() {
    let manager = native_manager();
    manager.enable().await;
    let root = repository_root();
    let fixture = tempfile::tempdir().expect("redirection fixture");
    let redirected = fixture.path().join("native pipe output.txt");

    let unicode = start_command(&manager, unicode_command(), &root, Duration::from_secs(5)).await;
    let unicode = match unicode {
        Ok(receipt) => settle_session(&manager, receipt).await,
        Err(error) => Err(error),
    };
    let redirected_output = start_command(
        &manager,
        redirect_command(&redirected),
        &root,
        Duration::from_secs(5),
    )
    .await;
    let redirected_output = match redirected_output {
        Ok(receipt) => settle_session(&manager, receipt).await,
        Err(error) => Err(error),
    };
    let cleanup = cleanup(&manager).await;

    cleanup.expect("cleanup succeeds");
    let (unicode_terminal, unicode_output) = unicode.expect("Unicode command settles");
    assert_eq!(unicode_terminal.exit_code, Some(0));
    assert!(unicode_output.contains("中文🙂"), "{unicode_output:?}");
    assert!(!unicode_output.contains("[6n"), "{unicode_output:?}");
    let (redirect_terminal, redirect_output) =
        redirected_output.expect("pipe and redirect command settles");
    assert_eq!(redirect_terminal.exit_code, Some(0));
    assert!(redirect_output.contains("beta"), "{redirect_output:?}");
    assert!(redirected.is_file());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shell_command_uses_default_relative_and_outside_working_directories() {
    let manager = native_manager();
    manager.enable().await;
    let root = repository_root();
    let workspace = WorkspaceRoot::new(&root).expect("workspace root");
    let relative_fixture = tempfile::Builder::new()
        .prefix("shell-pty-relative-")
        .tempdir_in(root.join("target"))
        .expect("relative cwd fixture");
    let outside_fixture = tempfile::tempdir().expect("outside cwd fixture");
    let relative = relative_fixture
        .path()
        .strip_prefix(&root)
        .expect("relative fixture belongs to workspace")
        .to_string_lossy()
        .into_owned();
    let tool = ShellCommandTool::new(manager.clone());

    let default = execute_shell_command_tool(&tool, &workspace, "default", None).await;
    let relative_result =
        execute_shell_command_tool(&tool, &workspace, "relative", Some(relative)).await;
    let outside = execute_shell_command_tool(
        &tool,
        &workspace,
        "outside",
        Some(outside_fixture.path().to_string_lossy().into_owned()),
    )
    .await;
    let cleanup = cleanup(&manager).await;

    cleanup.expect("cleanup succeeds");
    assert_output_path(default.expect("default cwd command"), &root);
    assert_output_path(
        relative_result.expect("relative cwd command"),
        relative_fixture.path(),
    );
    assert_output_path(
        outside.expect("outside cwd command"),
        outside_fixture.path(),
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unread_output_over_one_mib_is_truncated_and_result_stays_bounded() {
    let manager = native_manager();
    manager.enable().await;
    let root = repository_root();

    let first = start_command(&manager, oversized_output_command(), &root, Duration::ZERO).await;
    let receipt = match first {
        Ok(first) => {
            tokio::time::sleep(Duration::from_secs(2)).await;
            poll_session(&manager, first.session_id, Duration::ZERO).await
        }
        Err(error) => Err(error),
    };
    let stopped = match receipt.as_ref() {
        Ok(receipt) => tokio::time::timeout(
            TEST_TIMEOUT,
            manager.stop(&receipt.session_id, MAX_SHELL_OUTPUT_BYTES),
        )
        .await
        .map_err(|_| "oversized output stop timed out".to_owned())
        .and_then(|result| result.map_err(|error| format!("stop failed: {error:?}"))),
        Err(error) => Err(error.clone()),
    };
    let cleanup = cleanup(&manager).await;

    cleanup.expect("cleanup succeeds");
    stopped.expect("oversized output session stops");
    let receipt = receipt.expect("oversized output remains observable");
    assert!(receipt.output_truncated, "{receipt:?}");
    assert!(
        receipt.output.len() <= MAX_SHELL_OUTPUT_BYTES,
        "{receipt:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn explicit_stop_terminates_the_reported_parent_and_child() {
    assert_tree_cleanup(TreeCleanup::Stop).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn permission_downgrade_terminates_the_reported_parent_and_child() {
    assert_tree_cleanup(TreeCleanup::Downgrade).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn normal_shutdown_terminates_the_reported_parent_and_child() {
    assert_tree_cleanup(TreeCleanup::Shutdown).await;
}

#[derive(Clone, Copy, Debug)]
enum TreeCleanup {
    Stop,
    Downgrade,
    Shutdown,
}

async fn assert_tree_cleanup(action: TreeCleanup) {
    let manager = native_manager();
    manager.enable().await;
    let root = repository_root();
    let started = start_command(
        &manager,
        process_tree_command(),
        &root,
        Duration::from_secs(1),
    )
    .await;
    let ready = match started {
        Ok(receipt) => wait_for_process_ids(&manager, receipt).await,
        Err(error) => Err(error),
    };
    let process_ids = ready
        .as_ref()
        .map(|(_, process_ids)| process_ids.clone())
        .map_err(Clone::clone);
    if let Ok(process_ids) = &process_ids {
        eprintln!("{action:?} process ids: {process_ids:?}");
    }

    let action_result = match ready.as_ref() {
        Ok((receipt, _)) => match action {
            TreeCleanup::Stop => tokio::time::timeout(
                TEST_TIMEOUT,
                manager.stop(&receipt.session_id, MAX_SHELL_OUTPUT_BYTES),
            )
            .await
            .map_err(|_| "explicit stop timed out".to_owned())
            .and_then(|result| result.map_err(|error| format!("stop failed: {error:?}")))
            .and_then(|receipt| {
                if receipt.state == ShellSessionState::Stopped {
                    Ok(())
                } else {
                    Err(format!("explicit stop returned {receipt:?}"))
                }
            }),
            TreeCleanup::Downgrade => {
                tokio::time::timeout(TEST_TIMEOUT, manager.disable_and_stop_all())
                    .await
                    .map_err(|_| "permission downgrade timed out".to_owned())
                    .and_then(|result| {
                        result.map_err(|error| format!("downgrade failed: {error:?}"))
                    })
            }
            TreeCleanup::Shutdown => tokio::time::timeout(TEST_TIMEOUT, manager.shutdown())
                .await
                .map_err(|_| "normal shutdown timed out".to_owned())
                .and_then(|result| result.map_err(|error| format!("shutdown failed: {error:?}"))),
        },
        Err(error) => Err(error.clone()),
    };
    let final_cleanup = cleanup(&manager).await;
    let survivors = match process_ids.as_ref() {
        Ok(process_ids) => wait_for_processes_to_exit(process_ids).await,
        Err(error) => Err(error.clone()),
    };

    final_cleanup.expect("final cleanup succeeds");
    action_result.expect("tree cleanup action succeeds");
    survivors.expect("reported parent and child both exit");

    if matches!(action, TreeCleanup::Downgrade) {
        let rejected = manager
            .start(
                command_request(
                    command_fixture(FixtureKind::Fast),
                    &root,
                    Duration::from_millis(250),
                ),
                &NeverCancelled,
            )
            .await;
        assert_eq!(rejected, Err(ShellManagerError::Disabled));
    }
}

async fn wait_for_process_ids(
    manager: &ShellSessionManager,
    mut receipt: ShellReceipt,
) -> Result<(ShellReceipt, Vec<u32>), String> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let mut output = receipt.output.clone();
    loop {
        if let Ok(process_ids) = parse_process_ids(&output) {
            return Ok((receipt, process_ids));
        }
        if receipt.state != ShellSessionState::Running || tokio::time::Instant::now() >= deadline {
            return Err(format!("missing deterministic process IDs in {output:?}"));
        }
        receipt = poll_session(
            manager,
            receipt.session_id.clone(),
            Duration::from_millis(250),
        )
        .await?;
        output.push_str(&receipt.output);
    }
}

async fn execute_shell_command_tool(
    tool: &ShellCommandTool,
    workspace: &WorkspaceRoot,
    call_suffix: &str,
    cwd: Option<String>,
) -> Result<ShellReceipt, String> {
    let arguments = match cwd {
        Some(cwd) => serde_json::json!({
            "command": current_directory_command(),
            "cwd": cwd,
            "yield_time_ms": 5000,
            "max_output_bytes": MAX_SHELL_OUTPUT_BYTES,
        }),
        None => serde_json::json!({
            "command": current_directory_command(),
            "yield_time_ms": 5000,
            "max_output_bytes": MAX_SHELL_OUTPUT_BYTES,
        }),
    };
    let call = ToolCall::new(
        ToolCallId::new(format!("call-cwd-{call_suffix}"))
            .map_err(|error| format!("call ID: {error:?}"))?,
        "shell_command",
        serde_json::to_string(&arguments).map_err(|error| format!("arguments: {error}"))?,
    )
    .map_err(|error| format!("tool call: {error:?}"))?;
    let invocation = ToolInvocation::new(call, ToolEffect::Process)
        .map_err(|error| format!("invocation: {error:?}"))?;
    let result = tokio::time::timeout(
        TEST_TIMEOUT,
        tool.execute(workspace, &invocation, &NeverCancelled),
    )
    .await
    .map_err(|_| "shell tool timed out".to_owned())?;
    let output = match result.output {
        Some(output) => output,
        None => return Err(format!("shell tool produced no receipt: {result:?}")),
    };
    serde_json::from_str(&output).map_err(|error| format!("shell receipt: {error}"))
}

fn assert_output_path(receipt: ShellReceipt, expected: &Path) {
    assert_eq!(receipt.state, ShellSessionState::Exited, "{receipt:?}");
    assert_eq!(receipt.exit_code, Some(0), "{receipt:?}");
    let expected = std::fs::canonicalize(expected).expect("canonical expected cwd");
    let actual_text = receipt
        .output
        .trim()
        .strip_prefix("Microsoft.PowerShell.Core\\FileSystem::")
        .unwrap_or(receipt.output.trim());
    let actual = std::fs::canonicalize(actual_text).expect("canonical actual cwd");
    let expected = normalize_path_text(&expected);
    let actual = normalize_path_text(&actual);
    assert_eq!(actual, expected);
}

fn normalize_path_text(path: &Path) -> String {
    let value = path
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();
    let value = value
        .strip_prefix("microsoft.powershell.core/filesystem::")
        .unwrap_or(&value);
    value
        .strip_prefix("//?/")
        .unwrap_or(value)
        .trim_end_matches('/')
        .to_owned()
}

fn parse_process_ids(output: &str) -> Result<Vec<u32>, String> {
    let mut parent = None;
    let mut child = None;
    for field in output.split([';', '\r', '\n']) {
        let field = field.trim();
        if let Some(value) = field.strip_prefix("parent=") {
            parent = value.parse::<u32>().ok();
        }
        if let Some(value) = field.strip_prefix("child=") {
            child = value.parse::<u32>().ok();
        }
    }
    match (parent, child) {
        (Some(parent), Some(child)) => Ok(vec![parent, child]),
        _ => Err(format!("missing deterministic process IDs in {output:?}")),
    }
}

async fn wait_for_processes_to_exit(process_ids: &[u32]) -> Result<(), String> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let survivors = process_ids
            .iter()
            .copied()
            .filter(|process_id| process_is_alive(*process_id))
            .collect::<Vec<_>>();
        if survivors.is_empty() {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!("surviving process IDs: {survivors:?}"));
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[cfg(windows)]
fn process_is_alive(process_id: u32) -> bool {
    std::process::Command::new(
        Path::new(&std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into()))
            .join("System32")
            .join("WindowsPowerShell")
            .join("v1.0")
            .join("powershell.exe"),
    )
    .args([
        "-NoLogo",
        "-NoProfile",
        "-Command",
        &format!(
            "if (Get-Process -Id {process_id} -ErrorAction SilentlyContinue) {{ exit 0 }} else {{ exit 1 }}"
        ),
    ])
    .status()
    .is_ok_and(|status| status.success())
}

#[cfg(target_os = "linux")]
fn process_is_alive(process_id: u32) -> bool {
    Path::new("/proc").join(process_id.to_string()).exists()
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("repository root")
        .to_owned()
}

#[cfg(windows)]
fn unicode_command() -> &'static str {
    "[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false); Write-Output '中文🙂'"
}

#[cfg(windows)]
fn current_directory_command() -> &'static str {
    "Write-Output (Get-Location).Path"
}

#[cfg(target_os = "linux")]
fn current_directory_command() -> &'static str {
    "pwd"
}

#[cfg(windows)]
fn oversized_output_command() -> &'static str {
    "[Console]::Write(('x' * 1100000)); Start-Sleep -Seconds 5"
}

#[cfg(target_os = "linux")]
fn oversized_output_command() -> &'static str {
    "dd if=/dev/zero bs=1100000 count=1 2>/dev/null | tr '\\0' x; sleep 5"
}

#[cfg(windows)]
fn process_tree_command() -> &'static str {
    "$exe = Join-Path $PSHOME 'powershell.exe'; $child = Start-Process -FilePath $exe -ArgumentList @('-NoLogo','-NoProfile','-Command','Start-Sleep -Seconds 120') -NoNewWindow -PassThru; Write-Output \"parent=$PID;child=$($child.Id)\"; Start-Sleep -Seconds 120"
}

#[cfg(target_os = "linux")]
fn process_tree_command() -> &'static str {
    "sleep 120 & child=$!; printf 'parent=%s;child=%s\\n' \"$$\" \"$child\"; wait \"$child\""
}

#[cfg(windows)]
fn tty_expected_output() -> &'static str {
    "in=True;out=True"
}

#[cfg(target_os = "linux")]
fn tty_expected_output() -> &'static str {
    "in=true;out=true"
}

#[cfg(target_os = "linux")]
fn unicode_command() -> &'static str {
    "printf '中文🙂\\n'"
}

#[cfg(windows)]
fn redirect_command(path: &Path) -> String {
    let path = path.to_string_lossy().replace('\'', "''");
    format!(
        "'alpha','beta' | Set-Content -Encoding UTF8 '{path}'; Get-Content '{path}' | Where-Object {{ $_ -eq 'beta' }}"
    )
}

#[cfg(target_os = "linux")]
fn redirect_command(path: &Path) -> String {
    let path = path.to_string_lossy().replace('\'', "'\"'\"'");
    format!("printf 'alpha\\nbeta\\n' | tee '{path}' | grep '^beta$'")
}
