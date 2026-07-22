# Codex-Aligned Shell I/O Repair Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make ordinary full-access Shell commands use lossless pipe I/O by default, retain explicit PTY/ConPTY execution with `tty: true`, and make the complete 32 KiB Windows command contract launchable.

**Architecture:** Add one explicit `ShellIoMode` boundary from JSON parsing through the session manager to a renamed generic Shell backend. The native backend launches the existing authenticated host over either OS pipes or a terminal while preserving the same Job/process-group containment. Linux terminal mode remains on `portable-pty`; Windows terminal mode crosses a new safe `windows-conpty` API whose isolated implementation owns the raw ConPTY, pipe, thread, process, and Job handles needed for deterministic teardown and pre-activation Job assignment. The native Windows parent stages command text outside `CreateProcess` arguments and retains its `TempPath` owner outside the Job. The trusted host verifies that payload against the authenticated command and launches a fixed PowerShell bootstrap that strict-decodes and deletes the payload before an authenticated loopback acknowledgement permits host `Ready`.

**Tech Stack:** Rust 1.97/edition 2024, Tokio 1.52.3, `portable-pty` 0.9.0 on Linux, direct `windows-sys` 0.61.2 plus `win32job` behind the isolated Windows boundary, direct `filedescriptor` 0.8.3, `tempfile` 3.27.0, PowerShell/pwsh, Windows Job Objects, Linux process groups/subreaper, JSON Schema fixtures, Cargo and Node verification harnesses.

## Global Constraints

- Work only in `E:\wiki-coding\.worktrees\full-access-shell` on `codex/full-access-shell`; do not mutate `main`.
- Follow `docs/superpowers/specs/2026-07-22-codex-aligned-shell-io-design.md` and preserve the previously approved full-access permission and session contracts.
- `shell_command.tty` is optional, defaults to `false`, and is the only new public field.
- `tty: false` uses OS pipes; `tty: true` uses a fixed 120x30 PTY/ConPTY.
- `MAX_SHELL_COMMAND_BYTES` stays exactly `32 * 1_024` UTF-8 bytes; `+1` remains `input_limit`.
- Both modes retain poll, write, submit, stop, cancellation, permission-downgrade, shutdown, bounded output, and complete process-tree cleanup.
- Each session retains at most 1 MiB unread output, all sessions retain at most 8 MiB, and one Shell receipt contains at most 49,152 output bytes.
- Windows Job assignment remains before host activation; Linux host containment remains fail closed.
- Command, cwd, stdin, and output bodies remain absent from safe trace.
- Payload staging, validation, decoding, deletion acknowledgement, or child-spawn failure occurs before session publication. Post-spawn failure transfers the complete payload/tree/I/O ownership to an internal unpublished manager entry; its running slot and resources remain owned until cleanup succeeds. Immediate cleanup returns `shell_launch_failed`; failed cleanup remains fail closed and retryable with the existing indeterminate cleanup result.
- Do not add Pi, Node/TypeScript Agent runtime, tmux, browser control, external terminal runtime, `cmd.exe` fallback, or macOS enablement.
- Do not refresh hosted evidence, push, merge, tag, release, or publish.
- Use TDD for every behavioral change: observe the focused test fail for the intended reason, implement the smallest coherent change, then rerun the focused and neighboring suites.
- Use `apply_patch` for edits; preserve unrelated working-tree changes if any appear.
- Every existing workspace crate, including `crates/tools`, retains `unsafe_code = "forbid"`. The only authorized exception is the new Windows-only `crates/windows-conpty` crate, which may contain the minimum reviewed `unsafe` needed to call Win32 ConPTY, pipe, thread, process, and Job APIs.
- `crates/windows-conpty` exposes only safe Rust APIs and never exports raw handles. It must retain ConPTY-side pipe handles through `ClosePseudoConsole`, start a dedicated output drain before host activation, preserve the exact final output tail through pipe EOF, assign the Job before host activation, and join or cancel its owned blocking reader within the caller's single shared two-second cleanup budget.

## File and Responsibility Map

- `crates/tools/src/shell/backend.rs`: generic Shell I/O mode and backend/resource contracts.
- `crates/tools/src/shell/manager.rs`: map `tty` to an I/O mode and retain lifecycle/output ownership.
- `crates/tools/src/shell/native.rs`: pipe versus terminal host launch, outer containment, and native resource wrappers.
- `crates/windows-conpty/src/lib.rs`: Windows-only safe ConPTY boundary; owns raw handles, pre-activation Job assignment, output draining, exact-tail delivery, and bounded deterministic teardown.
- `crates/windows-conpty/tests/teardown.rs`: deterministic lowest-boundary blocked-reader cancellation and output-tail regressions.
- `crates/tools/src/shell/host.rs`: Windows payload validation, PowerShell bootstrap, and authenticated deletion acknowledgement.
- `crates/tools/src/shell/command.rs`: strict JSON parsing and default `tty: false` behavior.
- `crates/tools/src/policy.rs`: Provider-visible `tty` schema.
- `crates/tools/tests/shell_manager.rs`: deterministic manager mode and cursor-handshake tests.
- `crates/tools/tests/shell_tools.rs`: tool parsing, schema, and fake-backend request tests.
- `crates/tools/tests/shell_pty.rs`: real Windows/Linux pipe and terminal behavior before its final evidence rename.
- `crates/cli/tests/tool_loop.rs`: driver fake backend and real cancellation/session regressions.
- `fixtures/compat/tools/full-access-schemas.v1.json`: versioned full-access tool definition.
- `.github/workflows/ci.yml` and `crates/compat-harness/{src,tests}/source_authority.rs`: mandatory two-platform native Shell I/O gate.
- `fixtures/compat/{public-contract.v1.json,report.expected.json}`: exact Shell evidence paths and contract fingerprint.
- `README.md`, `docs/release/subprocess-sandbox.md`, `docs/verification/coding-agent-execution-plane.md`, `.planning/REQUIREMENTS.md`, `.planning/ROADMAP.md`, `.planning/STATE.md`: user and project truth.

---

### Task 1: Introduce Pipe-by-Default and Explicit Terminal Mode

**Files:**

- Modify: `Cargo.toml`
- Modify: `crates/tools/Cargo.toml`
- Create: `crates/windows-conpty/Cargo.toml`
- Create: `crates/windows-conpty/src/lib.rs`
- Create: `crates/windows-conpty/tests/teardown.rs`
- Modify: `crates/tools/src/shell/backend.rs`
- Modify: `crates/tools/src/shell/mod.rs`
- Modify: `crates/tools/src/lib.rs`
- Modify: `crates/tools/src/shell/command.rs`
- Modify: `crates/tools/src/shell/manager.rs`
- Modify: `crates/tools/src/shell/native.rs`
- Modify: `crates/tools/src/policy.rs`
- Modify: `crates/tools/src/adapter.rs`
- Modify: `crates/tools/tests/shell_manager.rs`
- Modify: `crates/tools/tests/shell_tools.rs`
- Modify: `crates/tools/tests/shell_pty.rs`
- Modify: `crates/cli/tests/tool_loop.rs`
- Modify: `fixtures/compat/tools/full-access-schemas.v1.json`

**Interfaces:**

- Consumes: the existing authenticated host protocol, `ShellSessionManager`, one merged reader/writer resource, and the unchanged 32 KiB command validation.
- Produces: `ShellIoMode`, `ShellBackend`, `ShellChild`, `ShellGuard`, `ShellTerminateFuture`, `SpawnedShell`, and `NativeShellBackend`; a `tty: bool` on `ShellCommandRequest`; native pipe and terminal launch paths.

The locked model is:

```rust
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

pub trait ShellBackend: Send + Sync {
    fn requires_startup_cursor_handshake(&self) -> bool {
        false
    }

    fn spawn(&self, request: &ShellSpawnRequest) -> io::Result<SpawnedShell>;
}
```

- [ ] **Step 1: Write tool-contract and manager-mode tests before production edits**

Add fake-backend request inspection and these cases to `shell_tools.rs`:

```rust
fn request_modes(&self) -> Vec<ShellIoMode> {
    self.requests
        .lock()
        .expect("requests lock")
        .iter()
        .map(|request| request.io_mode)
        .collect()
}

#[tokio::test]
async fn shell_command_defaults_to_pipe_and_accepts_explicit_terminal_mode() {
    let fixture = Fixture::new().await;
    fixture.backend.queue_exited(b"pipe".to_vec(), 0);
    fixture.backend.queue_exited(b"terminal".to_vec(), 0);

    let first = invoke(
        &fixture.port,
        invocation("shell_command", ToolEffect::Process, json!({"command": "first"})),
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
            ShellIoMode::Terminal { cols: 120, rows: 30 },
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
```

In `shell_manager.rs`, record each `ShellSpawnRequest` and add a terminal-only cursor-handshake test: pipe mode must publish without waiting for `ESC[6n`; terminal mode must preserve the existing bounded handshake behavior when the injected backend requires it.

- [ ] **Step 2: Write real native I/O regressions**

Change the existing `command_request` helper to accept `tty: bool`, pass `true` from existing PTY behavior tests, and add default-pipe tests using `false`:

```rust
#[cfg(windows)]
fn redirected_fixture() -> &'static str {
    "Write-Output \"in=$([Console]::IsInputRedirected);out=$([Console]::IsOutputRedirected)\""
}

#[cfg(target_os = "linux")]
fn redirected_fixture() -> &'static str {
    "test ! -t 0 && test ! -t 1 && printf 'in=true;out=true\\n'"
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_shell_defaults_to_redirected_pipe_io() {
    let manager = native_manager();
    manager.enable().await;
    let first = start_command(&manager, redirected_fixture(), &repository_root(), false, Duration::from_secs(5))
        .await
        .expect("pipe command");
    let (_, output) = settle_session(&manager, first).await.expect("pipe settles");
    assert!(output.contains("in=true;out=true"), "{output:?}");
    cleanup(&manager).await.expect("cleanup");
}
```

Create a nested temporary cwd whose canonical display path exceeds 160 bytes, run the platform current-directory command in pipe mode, and assert that one normalized output line equals the complete canonical path. Add a pipe prompt test that writes `alpha` through `ShellWriteRequest` and receives `got:alpha`. Parameterize the existing stop/downgrade/shutdown tree helper over `tty: bool` and run each action once in pipe mode and once in terminal mode.

- [ ] **Step 3: Run the focused tests and record the RED evidence**

```powershell
cargo test -p minimax-tools --test shell_tools shell_command_defaults_to_pipe_and_accepts_explicit_terminal_mode --locked
cargo test -p minimax-tools --test shell_manager cursor_handshake --locked -- --nocapture
cargo test -p minimax-tools --test shell_pty native_shell_defaults_to_redirected_pipe_io --locked -- --nocapture
cargo test -p minimax-tools --test shell_pty long_pipe_output_preserves_one_logical_working_directory_line --locked -- --nocapture
```

Expected RED:

- the first test does not compile because `ShellIoMode`/`io_mode` do not exist;
- after test-only mechanical compilation fixes, explicit `tty` is rejected as an unknown field;
- the native default reports terminal I/O and the long path contains a ConPTY-inserted line break;
- pipe cursor-handshake behavior still follows the backend-wide PTY flag.

Keep the failure output in the temporary agent log; do not weaken the assertions.

- [ ] **Step 4: Add the public field and explicit mode mapping**

Implement strict parsing in `shell/command.rs`:

```rust
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ShellCommandArguments {
    command: String,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    tty: bool,
    #[serde(default)]
    yield_time_ms: Option<u64>,
    #[serde(default)]
    max_output_bytes: Option<usize>,
}
```

Copy `arguments.tty` into `ShellCommandRequest`. In `ShellSessionManager::start`, map it exactly:

```rust
let io_mode = if request.tty {
    ShellIoMode::Terminal { cols: 120, rows: 30 }
} else {
    ShellIoMode::Pipe
};
let spawn_request = ShellSpawnRequest {
    command: request.command,
    cwd: request.cwd,
    io_mode,
};
```

Pass `io_mode` into resource ownership so `requires_startup_cursor_handshake` is considered only for `ShellIoMode::Terminal`. Add the optional boolean to `policy.rs` and the full-access schema fixture:

```rust
"tty": {
    "description": "Use a real PTY/ConPTY terminal; defaults to false.",
    "type": "boolean"
}
```

- [ ] **Step 5: Rename the generic PTY abstractions without changing lifecycle semantics**

Rename the workspace-visible APIs and every fake/adapter reference:

```text
PtyBackend         -> ShellBackend
PtyChild           -> ShellChild
PtyGuard           -> ShellGuard
PtyTerminateFuture -> ShellTerminateFuture
SpawnedPty         -> SpawnedShell
NativePtyBackend   -> NativeShellBackend
NativePtyChild     -> NativeShellChild
NativePtyGuard     -> NativeShellGuard
```

Keep `portable_pty::MasterPty`, `PtySize`, and terminal-specific local names where they describe actual PTY resources. Do not add compatibility aliases; update the internal callers in `adapter.rs`, tools tests, manager unit tests, and CLI tool-loop tests in the same commit.

- [ ] **Step 6: Implement the native pipe host path**

Declare the already-locked dependency directly:

```toml
# Cargo.toml [workspace.dependencies]
filedescriptor = "=0.8.3"

# crates/tools/Cargo.toml [dependencies]
filedescriptor.workspace = true
```

Split `NativeShellBackend::spawn` by mode:

```rust
fn spawn(&self, request: &ShellSpawnRequest) -> io::Result<SpawnedShell> {
    match request.io_mode {
        ShellIoMode::Pipe => self.spawn_pipe(request),
        ShellIoMode::Terminal { cols, rows } => self.spawn_terminal(request, cols, rows),
    }
}
```

Move the current platform PTY bodies into `spawn_terminal`. Implement `spawn_pipe` with two `filedescriptor::Pipe` objects. The host receives the input read end as stdin and duplicated output write ends as stdout/stderr; the manager receives the input write end and output read end. Drop the child-side parent copies immediately after spawn.

Use `std::process::Command` for the pipe host and add a `NativePipeChild` implementing `ShellChild` around `std::process::Child`. On Windows, expose its raw process handle to the existing `WindowsJobBoundary` and assign the Job before `start_host_protocol`. On Linux, retain the host's exclusive cleanup ownership and reap the pipe host with the same bounded semantics as the PTY host. Generalize `start_host_protocol` to receive a secret-free `try_wait` probe closure so both portable-PTY and standard-process children use the identical authenticated sequence.

The returned resources remain exactly:

```rust
SpawnedShell {
    child: Box<dyn ShellChild>,
    reader: Box<dyn Read + Send>,
    writer: Box<dyn Write + Send>,
    guard: Box<dyn ShellGuard>,
}
```

On Linux, `NativeShellGuard` owns `Option<Box<dyn MasterPty + Send>>`; it is `Some` only for terminal mode. On Windows, terminal mode is created by the safe `windows-conpty` boundary. That boundary starts its dedicated output drain before activation, assigns the host to the kill-on-close Job before returning activation authority, retains ConPTY-side pipe handles until `ClosePseudoConsole`, and closes input then ConPTY then drains to EOF. `CancelSynchronousIo` is permitted only as a bounded fallback against the boundary-owned drain thread, followed by a join, and all close/drain/cancel/join work consumes the manager's one shared two-second cleanup deadline. Pipe mode, authentication, permission downgrade, output caps, fixed 120x30 geometry, and Linux `portable-pty` behavior remain unchanged.

- [ ] **Step 7: Run the focused GREEN and neighboring suites**

```powershell
cargo test -p minimax-tools --test shell_tools --locked
cargo test -p minimax-tools --test shell_manager --locked -- --nocapture
cargo test -p minimax-tools --test shell_pty --locked -- --nocapture
cargo test -p minimax-cli --test tool_loop --locked
cargo test -p minimax-tools --test tool_schemas --locked
cargo check -p minimax-tools --all-targets --all-features --locked
cargo fmt --all -- --check
```

Expected GREEN: all pass; default pipe reports redirected I/O and preserves the long path, explicit terminal still reports a TTY, both modes accept session input and clean every tested process tree, and the schema fixture contains optional `tty`.

- [ ] **Step 8: Commit the I/O-mode slice**

```powershell
git add Cargo.toml Cargo.lock crates/tools/Cargo.toml crates/tools/src/shell crates/tools/src/lib.rs crates/tools/src/adapter.rs crates/tools/src/policy.rs crates/tools/tests/shell_manager.rs crates/tools/tests/shell_tools.rs crates/tools/tests/shell_pty.rs crates/cli/tests/tool_loop.rs fixtures/compat/tools/full-access-schemas.v1.json
git diff --cached --check
git commit -m "feat(shell): split pipe and terminal execution"
```

#### Task 1 review-fix addendum: deterministic Windows ConPTY teardown

Implementation commit `08748a9` passed its controller tests but review rejected the blanket native-test semaphore because it hid a real Windows reader-EOF race. The user authorized the minimum necessary `unsafe` only in the new Windows-only `crates/windows-conpty` boundary; no existing crate receives an exception.

- [ ] **Review-fix Step A: Lock the boundary with deterministic RED tests**

Add lowest-boundary tests before implementation. One test must leave an anonymous-pipe writer open and prove that a boundary-owned blocked drain can be cancelled and joined within the supplied deadline. A second must deliver multiple chunks plus an exact terminal tail, close normally, and prove every byte is delivered exactly once through EOF. Retain the un-serialized native regressions in `shell_pty.rs`, strengthening their payload to an exact multi-chunk body and unique tail. Run eight sessions through one manager and at least eight independent managers.

Expected RED: the boundary tests fail to compile because the safe boundary API does not yet exist; the existing un-serialized native concurrency regressions expose the Windows reader-EOF hang or timeout without a test-only semaphore.

- [ ] **Review-fix Step B: Implement the isolated safe boundary**

Add pinned `windows-sys = "=0.61.2"` APIs only to `crates/windows-conpty`. Keep raw `HPCON`, pipe, process, thread, attribute-list, and Job handles private. Spawn the drain before activation, assign the Job before activation is exposed, and make explicit close order `input -> ClosePseudoConsole -> drain to EOF -> bounded CancelSynchronousIo fallback -> join -> remaining handle release`. Use explicit ownership rather than field-drop order. Copy no reference code without retaining its license notice and attribution.

- [ ] **Review-fix Step C: Integrate one shared cleanup deadline**

Change the safe guard contract only as needed to pass a remaining deadline into the boundary. The manager owns one `CLEANUP_WAIT` deadline; Windows close/drain/cancel and both reader joins all consume its remaining time rather than receiving independent two-second windows. Preserve authentication, permission downgrade, output caps, process-tree containment, fixed 120x30 geometry, pipe mode, and Linux `portable-pty`.

- [ ] **Review-fix Step D: Prove GREEN and the unsafe boundary**

```powershell
cargo test -p minimax-windows-conpty --all-targets --locked -- --nocapture
cargo test -p minimax-tools --test shell_pty native_terminal_concurrent --locked -- --nocapture
cargo test -p minimax-tools --test shell_pty --locked -- --nocapture
cargo test -p minimax-tools --all-targets --all-features --locked
rg -n "unsafe" crates --glob "*.rs" --glob "!windows-conpty/**"
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo fmt --all -- --check
```

Expected GREEN: exact final tails are present, all sixteen concurrency shapes settle without serialization or survivors, every test remains inside the single two-second cleanup budget, and Rust `unsafe` occurs only in `crates/windows-conpty`.

---

### Task 2: Move Windows Commands Out of the Process Command Line

**Files:**

- Modify: `crates/tools/src/shell/native.rs`
- Modify: `crates/tools/src/shell/host.rs`
- Modify: `crates/tools/src/shell/manager.rs`
- Modify: `crates/tools/tests/shell_io.rs` (named `shell_pty.rs` during the Task 2 RED cycle and renamed by Task 3)
- Modify: `crates/tools/tests/shell_host_process.rs`
- Modify: `crates/tools/tests/shell_manager.rs`
- Modify: `crates/tools/tests/shell_tools.rs`
- Modify: `crates/cli/tests/tool_loop.rs`

**Interfaces:**

- Consumes: the native parent's command, the authenticated host's independently delivered command string, and existing `tempfile`/`getrandom` dependencies.
- Produces: parent-owned `WindowsCommandPayload::stage`, `WINDOWS_COMMAND_PATH_ENV`, a constant PowerShell bootstrap, a fresh 256-bit loopback acknowledgement, and retryable unpublished-start cleanup ownership.

- [ ] **Step 1: Write Windows payload unit tests**

Under `#[cfg(windows)]` in `host.rs`, add tests for staging, argv secrecy, and guard cleanup:

```rust
#[test]
fn windows_command_payload_is_utf8_exclusive_and_guard_deleted() {
    let command = "Write-Output '雪-32k'";
    let payload = super::WindowsCommandPayload::stage(command).expect("stage command");
    let path = payload.path().to_owned();
    assert_eq!(std::fs::read_to_string(&path).expect("read payload"), command);
    drop(payload);
    assert!(!path.exists(), "payload guard must delete its path");
}

#[test]
fn windows_powershell_arguments_are_one_bounded_constant_bootstrap() {
    let shell = super::resolve_windows_process_shell().expect("PowerShell");
    assert!(shell.args.iter().any(|argument| argument == super::WINDOWS_COMMAND_BOOTSTRAP));
    assert!(shell.args.iter().map(String::len).sum::<usize>() < 1_024);
}
```

Add a direct bootstrap test that runs a staged Unicode command, asserts its output, and asserts that `$env:MINIMAX_SHELL_COMMAND_PATH` is empty inside user code. Add a real command containing the literal `process-list-secret-marker` that reads `[Environment]::CommandLine`; it must print `argv-clean` and never report that marker in the process command line.

- [ ] **Step 2: Write the exact 32 KiB integration regression**

In the native Shell integration test (then named `shell_pty.rs`), build a syntactically valid command whose UTF-8 byte length is exact:

```rust
#[cfg(windows)]
fn exact_maximum_command() -> String {
    let prefix = "Write-Output 'max-command-ok'; #";
    let command = format!(
        "{prefix}{}",
        "x".repeat(MAX_SHELL_COMMAND_BYTES - prefix.len())
    );
    assert_eq!(command.len(), MAX_SHELL_COMMAND_BYTES);
    command
}
```

Run it in default pipe mode and assert `shell_exited`, exit code `0`, and one `max-command-ok` marker. Retain the existing tool-level `MAX_SHELL_COMMAND_BYTES + 1` rejection and assert zero backend spawns.

- [ ] **Step 3: Run the Windows tests and record the RED evidence**

These RED commands intentionally retain the pre-Task 3 target name that existed
when the failures were captured:

```powershell
cargo test -p minimax-tools windows_command_payload --locked -- --nocapture
cargo test -p minimax-tools --test shell_pty exact_maximum_windows_command_launches --locked -- --nocapture
```

Expected RED: payload types/constants do not compile; after adding only the test scaffolding, the exact maximum command returns `ShellManagerError::Launch` because the current `-Command <command>` argv exceeds the Windows limit.

- [ ] **Step 4: Implement parent-owned guarded payload staging and acknowledged deletion**

Add these Windows-only constants and owner:

```rust
#[cfg(windows)]
const WINDOWS_COMMAND_PATH_ENV: &str = "MINIMAX_SHELL_COMMAND_PATH";

#[cfg(windows)]
const WINDOWS_ACK_ADDRESS_ENV: &str = "MINIMAX_SHELL_ACK_ADDRESS";

#[cfg(windows)]
const WINDOWS_ACK_TOKEN_ENV: &str = "MINIMAX_SHELL_ACK_TOKEN";

#[cfg(windows)]
const WINDOWS_COMMAND_BOOTSTRAP: &str = "$p=$env:MINIMAX_SHELL_COMMAND_PATH;$a=$env:MINIMAX_SHELL_ACK_ADDRESS;$t=$env:MINIMAX_SHELL_ACK_TOKEN;Remove-Item Env:MINIMAX_SHELL_COMMAND_PATH -ErrorAction SilentlyContinue;Remove-Item Env:MINIMAX_SHELL_ACK_ADDRESS -ErrorAction SilentlyContinue;Remove-Item Env:MINIMAX_SHELL_ACK_TOKEN -ErrorAction SilentlyContinue;try{$c=[IO.File]::ReadAllText($p,[Text.UTF8Encoding]::new($false,$true))}catch{throw 'shell command payload decode failed'}finally{Remove-Item -LiteralPath $p -Force -ErrorAction SilentlyContinue};if([IO.File]::Exists($p)){throw 'shell command payload cleanup failed'};$s=[Net.Sockets.TcpClient]::new();try{$h,$o=$a.Split(':');$s.Connect($h,[int]$o);$b=[Text.Encoding]::ASCII.GetBytes($t);$n=$s.GetStream();$n.Write($b,0,$b.Length)}finally{$s.Dispose()};Remove-Variable p,a,t,s,h,o,b,n -ErrorAction SilentlyContinue;Invoke-Expression $c";

#[cfg(windows)]
struct WindowsCommandPayload {
    path: tempfile::TempPath,
}

#[cfg(windows)]
impl WindowsCommandPayload {
    fn stage(command: &str) -> io::Result<Self> {
        let mut file = tempfile::Builder::new()
            .prefix("minimax-shell-")
            .suffix(".ps1")
            .tempfile()?;
        file.write_all(command.as_bytes())?;
        file.flush()?;
        Ok(Self { path: file.into_temp_path() })
    }

    fn path(&self) -> &Path {
        self.path.as_ref()
    }
}
```

The native Windows pipe and terminal paths stage this payload before starting
the trusted host, pass only its path in `WINDOWS_COMMAND_PATH_ENV`, and retain
the `TempPath` owner in `NativeShellGuard` outside the Job/process tree. Change
`resolve_windows_process_shell` so it takes no command and returns only:

```rust
vec![
    "-NoLogo".to_owned(),
    "-NoProfile".to_owned(),
    "-Command".to_owned(),
    WINDOWS_COMMAND_BOOTSTRAP.to_owned(),
]
```

`WindowsProcessSupervisor` must not own or create the payload. After the host
receives the authenticated command, read the staged path and require exact text
equality. Bind a loopback listener with a fresh 256-bit token, remove the four
internal host-bootstrap variables from PowerShell, and pass only the payload
path plus acknowledgement address/token. The fixed bootstrap removes those
three variables, strict-decodes and deletes the payload, verifies that the path
is absent, and returns the token before invoking user code. The supervisor waits
for the loopback token before spawn succeeds; only then may the host send
`Ready`.

On any post-spawn protocol/bootstrap failure, return `ShellSpawnError` with the
child, reader, writer, Job/ConPTY control, and native-side payload guard. The
manager converts those resources into an internal unpublished session. Keep its
running slot, payload, and contained tree owned across a failed cleanup attempt;
remove the unpublished entry and release the slot only after a retry confirms
cleanup.

- [ ] **Step 5: Prove cleanup and execution semantics**

Run the exact-boundary test, Unicode/nonterminating-error tests, prompt write
test, failed-start retention tests, process-tree cleanup tests, and internal host
process test. Task 3 renamed the final integration target to `shell_io`; unlike
the Step 3 RED record, these final commands use its current name:

```powershell
cargo test -p minimax-tools windows_command_payload --locked -- --nocapture
cargo test -p minimax-tools --test shell_io exact_maximum_windows_command_launches --locked -- --nocapture
cargo test -p minimax-tools --test shell_io windows_trusted_host --locked -- --nocapture
cargo test -p minimax-tools --test shell_io windows_failed_ --locked -- --nocapture
cargo test -p minimax-tools --test shell_io prompt_receives_write_and_submit_then_exits --locked -- --nocapture
cargo test -p minimax-tools --test shell_io terminates_the_reported_parent_and_child --locked -- --nocapture
cargo test -p minimax-tools --test shell_host_process --locked -- --nocapture
```

Expected GREEN: every test passes, payload/ack variables are absent from user
code, command bytes are not present in PowerShell argv, exact 32 KiB runs,
nonzero/Unicode/prompt semantics remain stable, failed-start payload and slot
ownership survives a blocked first cleanup, the next retry succeeds, and no
process or payload survives confirmed cleanup.

- [ ] **Step 6: Commit the Windows boundary slice**

```powershell
git add crates/tools/src/shell/host.rs crates/tools/tests/shell_pty.rs crates/tools/tests/shell_host_process.rs
git diff --cached --check
git commit -m "fix(shell): preserve windows command boundary"
```

#### Task 2 review-fix addendum: parent ownership, readiness, and failed-start retention

The Step 6 command records the original `8cbc77e` implementation commit and its
then-current `shell_pty` filename. Review found three lifecycle gaps and closed
them without rewriting that historical TDD evidence:

- `f4f3a10` moved payload staging and `TempPath` ownership out of the trusted
  host and into the native parent/guard outside the Job;
- `874c3ca` required exact payload/authenticated-command equality plus strict
  PowerShell decode/delete and a fresh 256-bit loopback acknowledgement before
  host `Ready`;
- `8fd76f8` returned all failed-start resources to the manager and retained the
  unpublished session, running slot, payload, and contained tree until cleanup
  retry succeeded.

The current final target is `shell_io`; Step 5 uses that name. Step 3 and the
Step 6 historical commit command deliberately retain `shell_pty` because Task 3
had not renamed the evidence target at those points.

---

### Task 3: Align CI, Contracts, Documentation, and GSD Truth

**Files:**

- Rename: `crates/tools/tests/shell_pty.rs` to `crates/tools/tests/shell_io.rs`
- Modify: `.github/workflows/ci.yml`
- Modify: `crates/compat-harness/src/source_authority.rs`
- Modify: `crates/compat-harness/tests/source_authority.rs`
- Modify: `fixtures/compat/public-contract.v1.json`
- Modify: `fixtures/compat/report.expected.json`
- Modify: `README.md`
- Modify: `docs/release/subprocess-sandbox.md`
- Modify: `docs/verification/coding-agent-execution-plane.md`
- Modify: `.planning/REQUIREMENTS.md`
- Modify: `.planning/ROADMAP.md`
- Modify: `.planning/STATE.md`

**Interfaces:**

- Consumes: passing Task 1/2 native tests and the existing SHELL-01 through SHELL-07 evidence rows.
- Produces: one truthful `shell_io` test target required on Windows and Linux, updated schema/evidence paths and fingerprint, and project state that distinguishes local implementation from pending hosted evidence.

- [ ] **Step 1: Rename native evidence around both modes**

Rename the integration target with `git mv`, then replace exact authoritative strings:

```text
crates/tools/tests/shell_pty.rs
-> crates/tools/tests/shell_io.rs

Run native PTY Shell integration
-> Run native Shell I/O integration

cargo test -p minimax-tools --test shell_pty --locked -- --nocapture
-> cargo test -p minimax-tools --test shell_io --locked -- --nocapture
```

Update the source-authority validator and all its mutations so the new step must appear exactly once, after Rust checks, on both matrix platforms, without `if:`, `continue-on-error:`, `env:`, alternate `shell:`, or a detached lookalike command.

- [ ] **Step 2: Update evidence fixtures and write failing authority tests first**

Change SHELL-04 and SHELL-06 evidence paths in `public-contract.v1.json` and the corresponding report rows to `crates/tools/tests/shell_io.rs`. Add/adjust source-authority mutations that remove the I/O step, restore the obsolete PTY command, make it Linux-only, allow failure, or add an environment override.

Run before updating the validator:

```powershell
cargo test -p minimax-compat-harness --test source_authority ci_keeps_rust_authority_ahead_of_packaging_and_fails_closed --locked -- --nocapture
cargo test -p minimax-compat-harness --test compat_report report_matches --locked -- --nocapture
```

Expected RED: the old validator rejects the renamed real CI step or accepts at least one obsolete/mutated form, and the compatibility report detects stale evidence paths/fingerprint.

- [ ] **Step 3: Update public documentation and planning wording**

State consistently:

- ordinary `shell_command` calls default to lossless pipe capture;
- `tty: true` opts into 120x30 PTY/ConPTY and terminal wrapping;
- both modes retain sessions and bounded stdin/output/cleanup;
- Windows commands up to 32 KiB are delivered outside PowerShell argv;
- confirm still advertises neither Shell tool;
- full access still grants arbitrary host file/network/environment access;
- macOS, resizing, browser control, Pi, Node Agent runtime, tmux, push, and release remain outside this change.

Update SHELL-02/03/07 and Phase 15 success criteria to say pipe-or-terminal rather than claiming every command uses PTY/ConPTY. In `STATE.md`, record the local repair as implemented and under local verification while keeping Phase 15 incomplete and hosted Windows/Linux evidence pending.

- [ ] **Step 4: Recompute the contract fingerprint and report fixture**

Compute the exact manifest fingerprint:

```powershell
node -e "const fs=require('fs'),c=JSON.parse(fs.readFileSync('fixtures/compat/public-contract.v1.json','utf8')),h=require('crypto').createHash('sha256'),i={schemaVersion:c.schemaVersion,contractVersion:c.contractVersion,provenanceCommit:c.provenanceCommit,productEntry:c.productEntry,requiredItemIds:c.requiredItemIds,items:c.items}; console.log('sha256:'+h.update(JSON.stringify(i)).digest('hex'))"
```

Apply that value to `contentFingerprint`. Generate the current report without redirecting into repository files:

```powershell
cargo run -p minimax-compat-harness --locked -- report --format json
```

Use `apply_patch` to change only the contract fingerprint and renamed/updated Shell rows in `fixtures/compat/report.expected.json`. Do not alter unrelated historical evidence or hosted records.

- [ ] **Step 5: Run the renamed evidence and contract suites**

```powershell
cargo test -p minimax-tools --test shell_io --locked -- --nocapture
cargo test -p minimax-tools --test tool_schemas --locked
cargo test -p minimax-compat-harness --test source_authority --locked
cargo test -p minimax-compat-harness --test compat_report --locked -- --skip hosted_cutover_evidence_matches_current_product --skip hosted_candidate_evidence_matches_current_product
npm run verify:rust-contracts:candidate
git diff --check
```

Expected GREEN: local I/O/schema/source/contract checks pass. Candidate verification may identify only the two explicitly known hosted fingerprint freshness gaps; it must not report source, schema, evidence-path, or architecture drift.

- [ ] **Step 6: Commit truth and authority updates**

```powershell
git add .github/workflows/ci.yml crates/compat-harness/src/source_authority.rs crates/compat-harness/tests/source_authority.rs crates/tools/tests/shell_io.rs fixtures/compat/public-contract.v1.json fixtures/compat/report.expected.json README.md docs/release/subprocess-sandbox.md docs/verification/coding-agent-execution-plane.md .planning/REQUIREMENTS.md .planning/ROADMAP.md .planning/STATE.md
git diff --cached --check
git commit -m "docs(shell): verify pipe and terminal authority"
```

---

### Task 4: Run Complete Verification and Review the Resulting Architecture

**Files:**

- Modify only if a verified defect requires it: files already listed in Tasks 1-3
- Modify: `E:\Agenc\agent-logs\temporary\2026-07-22.md` through a separate workspace-log patch, never in the repository commit

**Interfaces:**

- Consumes: the three implementation commits and every unchanged permission, lifecycle, trace, compatibility, Provider, retrieval, and packaging gate.
- Produces: fresh verification evidence, a clean diff, an architecture/code review, and an explicit hosted-evidence handoff without pushing.

- [ ] **Step 1: Run focused Shell and CLI suites from a fresh process**

```powershell
cargo test -p minimax-tools --test shell_manager --locked -- --nocapture
cargo test -p minimax-tools --test shell_tools --locked
cargo test -p minimax-tools --test shell_host_process --locked -- --nocapture
cargo test -p minimax-tools --test shell_io --locked -- --nocapture
cargo test -p minimax-tools --all-targets --all-features --locked
cargo test -p minimax-cli --test headless --locked
cargo test -p minimax-cli --test restart --locked
cargo test -p minimax-cli --test tool_loop --locked -- --nocapture
```

Expected: all pass with zero reported process survivors and no payload files bearing the test prefix after the relevant cases complete.

- [ ] **Step 2: Run cross-platform compilation and static checks**

```powershell
cargo check -p minimax-tools --tests --target x86_64-unknown-linux-gnu --locked
cargo clippy -p minimax-tools --all-targets --all-features --locked -- -D warnings
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo fmt --all -- --check
```

Expected: all exit `0`; `unsafe` is confined to the reviewed `crates/windows-conpty` boundary, every other crate remains `unsafe_code = "forbid"`, and both platform cfg branches compile.

- [ ] **Step 3: Run the full local product gates**

```powershell
cargo test --workspace --all-targets --all-features --locked -- --skip hosted_cutover_evidence_matches_current_product --skip hosted_candidate_evidence_matches_current_product
npm run eval:provider
npm run eval:retrieval
npm run verify:rust-contracts:candidate
npm run test:package
git diff --check
```

Expected: Rust workspace, provider, retrieval, local candidate contracts, and 41 package tests pass. The only non-green external status allowed is stale/missing hosted evidence bound to the changed product fingerprint.

- [ ] **Step 4: Review the complete branch diff against both Shell designs**

Review from the Phase 15 base and inspect these invariants explicitly:

```powershell
git diff --stat 5351d32...HEAD
git diff 5351d32...HEAD -- crates/tools/src/shell crates/tools/src/policy.rs crates/tools/src/adapter.rs crates/tools/tests crates/cli/tests/tool_loop.rs .github/workflows/ci.yml crates/compat-harness README.md docs/release docs/verification .planning
rg -n "NativePtyBackend|PtyBackend|PtyChild|PtyGuard|SpawnedPty|shell_pty" crates .github fixtures README.md docs/verification docs/release
rg -n "@earendil-works|pi-coding-agent|node_modules|tmux|cmd\.exe" Cargo.toml Cargo.lock crates package.json package-lock.json
git status --short --branch
```

Expected:

- no obsolete generic PTY type or `shell_pty` evidence target remains;
- only `portable_pty` terminal-specific names remain;
- permission snapshots, confirm-mode hiding/preflight, host authentication, Job/process-group containment, bounded buffers, trace secrecy, and cleanup ownership remain separate enforceable layers;
- the Windows command never appears in child argv or internal-host bootstrap metadata;
- no unrelated dependency/runtime/fallback enters the product;
- the worktree is clean after any necessary defect-fix commit and rerun.

Classify findings by severity. Fix every Critical or Important issue with a new failing regression and rerun the proportional focused suite plus Steps 2-3. Record low-severity follow-ups only when they are outside the approved design.

- [ ] **Step 5: Record final evidence without closing external gates**

Append exact command outcomes, test counts, commit IDs, process-survivor checks, residual risks, and hosted-evidence status to `E:\Agenc\agent-logs\temporary\2026-07-22.md` using `apply_patch`.

Do not mark Phase 15 complete, refresh hosted evidence, merge, or push. Hand the clean feature branch back with the local result and the explicit next authorization required for hosted Windows/Linux evidence and integration.

---

## Final Acceptance Checklist

- [ ] Omitted `tty` is pipe mode in schema parsing, manager requests, real Windows, and real Linux.
- [ ] `tty: true` is a real 120x30 PTY/ConPTY and existing interactive behavior remains green.
- [ ] The long audit-path working directory returns as one exact logical line in default mode.
- [ ] Exact 32 KiB Windows command text launches; 32 KiB plus one byte is rejected before spawn.
- [ ] Windows PowerShell argv and inherited user environment contain no command body or payload variable.
- [ ] Pipe and terminal sessions both poll, accept stdin, stop, downgrade, and shut down with no process-tree survivors.
- [ ] Confirm mode advertises and executes neither Shell tool; full access still advertises exactly ten tools.
- [ ] Output/session/global/result limits and trace secrecy are unchanged.
- [ ] CI requires the complete native Shell I/O target on Windows and Linux without conditional or allowed failure.
- [ ] Schema, public contract, report, documentation, and GSD state describe pipe-by-default plus explicit terminal mode consistently.
- [ ] Full Rust, Clippy, fmt, Provider, retrieval, candidate-contract, and package gates pass locally.
- [ ] Hosted evidence, push, merge, tag, release, and publication remain pending separate authorization.
