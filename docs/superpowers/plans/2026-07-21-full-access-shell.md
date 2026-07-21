# Full-Access Shell Sessions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add full-access-only native Shell execution with fast command completion, persistent PTY/ConPTY sessions, incremental output, interactive input, explicit stop, permission-downgrade cleanup, and no Pi/Node runtime dependency.

**Architecture:** Preserve the existing Provider-neutral Agent loop and terminal ToolInvocation state machine. Add an immutable permission-aware tool execution context, a bounded Rust `ShellSessionManager`, and two adapters (`shell_command`, `shell_session`) backed by `portable-pty` 0.9.0 on Windows and Linux. Expose the schemas only in `full-access`, reject forged calls before approval, and make the driver own permission transition and shutdown cleanup.

**Tech Stack:** Rust 1.97.0, Tokio 1.52.3, `portable-pty` 0.9.0, `getrandom` 0.4.3, PowerShell/ConPTY on Windows, POSIX Shell/PTY on Linux, existing JSON ToolInvocation/ToolResult protocol, existing npm/native release contracts.

## Global Constraints

- Preserve the approved design in `docs/superpowers/specs/2026-07-21-full-access-shell-design.md`.
- Shell tools execute only under the immutable context `PermissionMode::FullAccess + ToolSandboxPolicy::Disabled`; `confirm` must reject a forged Shell call before approval and execute zero processes.
- `full-access` performs no per-command confirmation. Restart returns to `confirm`.
- Keep the existing eight tools and their schema/path/secret/sandbox contracts unchanged.
- `shell_command` and `shell_session` may use ordinary host files, network, working directories outside the project, and the process environment; do not route their command/cwd/input fields through the fixed-tool workspace or secret scanner.
- Keep `MAX_TOOL_RESULT_BYTES = 64 * 1024`; Shell output per result is at most 49152 bytes, command input is at most 32 KiB, cwd is at most 4 KiB, and interactive write input is at most 16 KiB.
- Keep at most 8 running sessions, 1 MiB unread output per session, and 8 MiB unread output in total. Keep at most 32 drained terminal receipts or 5 minutes, whichever limit is reached first.
- Running sessions have no idle timeout. They end only through natural exit, explicit stop, permission downgrade, cancellation before ID delivery, or application shutdown.
- Initial terminal size is exactly 120 columns by 30 rows. v1 has no resize tool and no full-screen terminal emulator.
- Windows shell order is `pwsh.exe`, then `powershell.exe`, with `-NoLogo -NoProfile -Command`; do not fall back to `cmd.exe`.
- Linux shell order is an absolute executable `$SHELL`, then `/bin/bash`, then `/bin/sh`, with `-lc`.
- Stop sends an interrupt, waits at most 2 seconds, terminates the process tree, waits at most 2 more seconds, closes PTY handles, and reports indeterminate cleanup instead of claiming success when exit cannot be confirmed.
- Use only Rust product code. Do not add Pi, Node.js, TypeScript, tmux, an external terminal window, piped-stdio fallback, or automatic runtime installation.
- Before Task 1 execution, use the GSD new-milestone workflow to register milestone `v3.1` named `Full Access Shell`, phase 15, with requirements `SHELL-01` through `SHELL-07` matching protocol, permissions, PTY sessions, resource bounds, cleanup, cross-platform verification, and documentation. Do not hand-edit GSD state around that workflow.
- Product-authority changes intentionally make hosted release evidence stale. Run candidate/local gates during implementation; do not refresh hosted evidence, tag, push, publish, or weaken freshness checks without a separate user instruction.

---

## File and Responsibility Map

### New files

- `crates/protocol/src/shell.rs`: stable Shell session ID, state, receipt, constants, and validation.
- `crates/protocol/tests/shell_roundtrip.rs`: protocol serialization, invalid receipt, and size-boundary tests.
- `crates/tools/src/shell/mod.rs`: module exports and production constructor.
- `crates/tools/src/shell/buffer.rs`: incremental UTF-8/ANSI normalization and 1 MiB unread ring buffer.
- `crates/tools/src/shell/backend.rs`: testable `PtyBackend`, `PtyChild`, spawn request, and process guard contracts.
- `crates/tools/src/shell/manager.rs`: session registry, start/poll/write/stop, receipt retention, cancellation, and shutdown.
- `crates/tools/src/shell/native.rs`: `portable-pty` backend, platform Shell resolution, ConPTY/PTY launch, and process-tree termination hook.
- `crates/tools/src/shell/command.rs`: `shell_command` argument parsing, preflight, manager call, and ToolResult mapping.
- `crates/tools/src/shell/session.rs`: `shell_session` action parsing, preflight, manager call, and ToolResult mapping.
- `crates/tools/tests/shell_buffer.rs`: normalization, Unicode chunking, ring bounds, and truncation tests.
- `crates/tools/tests/shell_manager.rs`: fake-backend lifecycle, capacity, cancellation, write, stop, and GC tests.
- `crates/tools/tests/shell_tools.rs`: schema/preflight/adapter result tests.
- `crates/tools/tests/shell_pty.rs`: real Windows/Linux PTY end-to-end tests.
- `fixtures/compat/tools/full-access-schemas.v1.json`: exact ten-tool full-access schema fixture.

### Modified files

- `Cargo.toml`, `Cargo.lock`, `crates/tools/Cargo.toml`: exact Rust PTY/RNG dependencies.
- `crates/protocol/src/lib.rs`, `crates/protocol/src/tool.rs`, `crates/protocol/src/session.rs`: exports, versioned base/full-access tool-name constants, and safe tool-completion trace code.
- `crates/core/src/tool.rs`, `crates/core/src/ports.rs`, `crates/core/src/lib.rs`, `crates/core/src/trace.rs`: immutable execution context, tool lifecycle hooks, and Shell-safe trace allowlist.
- `crates/core/tests/tool_machine.rs`, `crates/core/tests/compaction_trace.rs`: context snapshot, terminal-invocation, and no-tool-body trace tests.
- `crates/tools/src/error.rs`, `crates/tools/src/policy.rs`, `crates/tools/src/process.rs`, `crates/tools/src/adapter.rs`, `crates/tools/src/lib.rs`: Shell denial codes, schemas, shared tree kill, dispatch, and exports.
- `crates/tools/tests/tool_schemas.rs`, `crates/tools/tests/process_tools.rs`: base-contract preservation and shared process cleanup regression.
- `crates/cli/src/driver.rs`, `crates/cli/src/main.rs`, `crates/cli/src/doctor.rs`: dynamic definitions, async permission transitions, shutdown, warning text, and output flow.
- `crates/cli/tests/tool_loop.rs`, `crates/cli/tests/headless.rs`, `crates/cli/tests/restart.rs`: Agent, permission, forged-call, restart, and shutdown regression tests.
- `crates/tui/src/render.rs`, `crates/tui/tests/command_render.rs`: readable Shell receipt rendering.
- `README.md`, `docs/release/subprocess-sandbox.md`, `docs/verification/coding-agent-execution-plane.md`, `.planning/phases/MMX-08-codex-style-subprocess-sandbox-hardening/08-SPEC.md`: new full-access contract and explicit Phase 8 supersession note.
- `.github/workflows/ci.yml`: explicit real PTY smoke on both hosted platforms.
- `crates/compat-harness/src/source_authority.rs`, `crates/compat-harness/tests/source_authority.rs`: require the cross-platform PTY CI gate and reject its removal or platform skip.
- `crates/compat-harness/src/manifest.rs`, `crates/compat-harness/src/baseline.rs`, `crates/compat-harness/tests/compat_report.rs`: register and execute the seven Shell public contracts.
- `fixtures/compat/public-contract.v1.json`, `fixtures/compat/report.expected.json`: bind `SHELL-01` through `SHELL-07` to exact Rust tests, schemas, CI, and documentation evidence.

---

### Task 1: Make Tool Execution Use One Immutable Permission Context

**Files:**

- Modify: `crates/core/src/tool.rs`
- Modify: `crates/core/src/ports.rs`
- Modify: `crates/core/src/lib.rs`
- Modify: `crates/core/tests/tool_machine.rs`
- Modify: `crates/tools/src/adapter.rs`
- Modify: `crates/tools/src/policy.rs`
- Modify: `crates/tools/tests/tool_schemas.rs`
- Modify: `crates/cli/src/driver.rs`
- Modify: `crates/cli/tests/tool_loop.rs`

**Interfaces:**

- Produces:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ToolExecutionContext {
    permission_mode: PermissionMode,
    sandbox_policy: ToolSandboxPolicy,
}

impl ToolExecutionContext {
    #[must_use]
    pub const fn for_permission_mode(permission_mode: PermissionMode) -> Self {
        Self {
            permission_mode,
            sandbox_policy: permission_mode.sandbox_policy(),
        }
    }

    #[must_use]
    pub const fn permission_mode(self) -> PermissionMode {
        self.permission_mode
    }

    #[must_use]
    pub const fn sandbox_policy(self) -> ToolSandboxPolicy {
        self.sandbox_policy
    }
}
```

```rust
pub trait ToolPort: Send + Sync {
    fn preflight(
        &self,
        invocation: &ToolInvocation,
        context: ToolExecutionContext,
        cancellation: &dyn CancellationPort,
    ) -> Result<(), ToolResult>;

    fn execute<'a>(
        &'a self,
        invocation: &'a ToolInvocation,
        context: ToolExecutionContext,
        cancellation: &'a dyn CancellationPort,
    ) -> ToolFuture<'a>;
}
```

- Changes `InvocationEffect::Execute` to carry `context: ToolExecutionContext` instead of only `sandbox_policy`.
- Existing adapters consume `context.sandbox_policy()` and otherwise behave byte-for-byte as before.

- [ ] **Step 1: Add failing core tests for the immutable context**

Add tests named `execution_context_maps_permission_to_sandbox_once` and `approved_invocation_executes_with_the_recorded_permission_snapshot`:

```rust
#[test]
fn execution_context_maps_permission_to_sandbox_once() {
    let confirm = ToolExecutionContext::for_permission_mode(PermissionMode::Confirm);
    assert_eq!(confirm.permission_mode(), PermissionMode::Confirm);
    assert_eq!(confirm.sandbox_policy(), ToolSandboxPolicy::Restricted);

    let full = ToolExecutionContext::for_permission_mode(PermissionMode::FullAccess);
    assert_eq!(full.permission_mode(), PermissionMode::FullAccess);
    assert_eq!(full.sandbox_policy(), ToolSandboxPolicy::Disabled);
}

#[test]
fn approved_invocation_executes_with_the_recorded_permission_snapshot() {
    let invocation = process_invocation("context-snapshot");
    let (mut machine, _) = InvocationMachine::request(invocation);
    let context = ToolExecutionContext::for_permission_mode(PermissionMode::Confirm);
    let effects = must(machine.apply(InvocationInput::PreflightAllowed {
        permission_mode: context.permission_mode(),
    }));
    assert!(matches!(effects.as_slice(), [InvocationEffect::RequestApproval(_)]));
    let effects = must(machine.apply(InvocationInput::Decision {
        decision: approved_decision("context-snapshot"),
        permission_mode: context.permission_mode(),
    }));
    assert!(matches!(effects.as_slice(), [InvocationEffect::PersistDecision(_)]));
    let effects = must(machine.apply(InvocationInput::Start));
    assert!(matches!(
        effects.as_slice(),
        [InvocationEffect::PersistStarted(_), InvocationEffect::Execute { context: actual, .. }]
            if *actual == context
    ));
}
```

- [ ] **Step 2: Run the focused tests and verify red**

Run:

```powershell
cargo test -p minimax-core --test tool_machine execution_context --locked
```

Expected: compilation fails because `ToolExecutionContext` and the `context` effect field do not exist.

- [ ] **Step 3: Add the context and update the state machine**

Add the interface above to `crates/core/src/tool.rs`, export it from `crates/core/src/lib.rs`, and replace:

```rust
InvocationEffect::Execute {
    invocation: self.invocation.clone(),
    sandbox_policy,
}
```

with:

```rust
let context = ToolExecutionContext::for_permission_mode(snapshot.permission_mode);
InvocationEffect::Execute {
    invocation: self.invocation.clone(),
    context,
}
```

- [ ] **Step 4: Update ToolPort and all existing implementations**

Use the exact trait signature above. In `BuiltinToolPort`, pass the context through dispatch and use only `context.sandbox_policy()` for the existing process-backed adapters:

```rust
async fn dispatch(
    &self,
    invocation: &ToolInvocation,
    context: ToolExecutionContext,
    cancellation: &dyn CancellationPort,
) -> ToolResult {
    let sandbox_policy = context.sandbox_policy();
    match invocation.call.name.as_str() {
        // existing eight arms remain unchanged apart from receiving sandbox_policy
        _ => unreachable!("common preflight rejects tools outside the registry"),
    }
}
```

Update every test `ToolPort` implementation to record `ToolExecutionContext`, not a standalone sandbox enum.

- [ ] **Step 5: Snapshot context before preflight in RuntimeDriver**

At the start of `complete_invocation`, create exactly one context:

```rust
let context = ToolExecutionContext::for_permission_mode(self.permission_mode);
```

Pass that same value to `tools.preflight`, `InvocationInput::PreflightAllowed`, approval decisions, `apply_invocation_effects`, and `tools.execute`. Do not read `self.permission_mode` again while completing that invocation.

- [ ] **Step 6: Run focused and workspace regression tests**

Run:

```powershell
cargo test -p minimax-core --test tool_machine --locked
cargo test -p minimax-tools --test tool_schemas --locked
cargo test -p minimax-cli --test tool_loop --locked
cargo test --workspace --all-targets --locked
```

Expected: all tests pass; the existing confirm/full-access approval and sandbox assertions remain unchanged.

- [ ] **Step 7: Commit the immutable-context slice**

```powershell
git add crates/core/src/tool.rs crates/core/src/ports.rs crates/core/src/lib.rs crates/core/tests/tool_machine.rs crates/tools/src/adapter.rs crates/tools/src/policy.rs crates/tools/tests/tool_schemas.rs crates/cli/src/driver.rs crates/cli/tests/tool_loop.rs
git commit -m "refactor(tools): snapshot execution permission context"
```

---

### Task 2: Define Shell Protocol Receipts and the Bounded Output Buffer

**Files:**

- Create: `crates/protocol/src/shell.rs`
- Create: `crates/protocol/tests/shell_roundtrip.rs`
- Modify: `crates/protocol/src/lib.rs`
- Create: `crates/tools/src/shell/mod.rs`
- Create: `crates/tools/src/shell/buffer.rs`
- Create: `crates/tools/tests/shell_buffer.rs`
- Modify: `crates/tools/src/lib.rs`

**Interfaces:**

```rust
pub const MAX_SHELL_COMMAND_BYTES: usize = 32 * 1_024;
pub const MAX_SHELL_CWD_BYTES: usize = 4 * 1_024;
pub const MAX_SHELL_INPUT_BYTES: usize = 16 * 1_024;
pub const MAX_SHELL_OUTPUT_BYTES: usize = 49_152;
pub const MAX_SHELL_SESSION_ID_BYTES: usize = 128;
pub const MAX_SHELL_UNREAD_BYTES: usize = 1_024 * 1_024;

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ShellSessionId(String);

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShellSessionState {
    Running,
    Exited,
    Stopped,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ShellReceipt {
    pub session_id: ShellSessionId,
    pub state: ShellSessionState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    pub output: String,
    pub output_truncated: bool,
}
```

```rust
pub struct ShellOutputBuffer {
    unread: VecDeque<u8>,
    truncated: bool,
    normalizer: TerminalNormalizer,
    budget: Arc<ShellOutputBudget>,
}

pub struct ShellOutputBudget {
    used: AtomicUsize,
    limit: usize,
}

pub struct ShellOutputChunk {
    pub output: String,
    pub truncated: bool,
}

impl ShellOutputBuffer {
    pub fn append(&mut self, bytes: &[u8]);
    pub fn finish(&mut self);
    pub fn take(&mut self, max_bytes: usize) -> ShellOutputChunk;
    pub fn unread_bytes(&self) -> usize;
}
```

- [ ] **Step 1: Add failing protocol round-trip and boundary tests**

Test exact camelCase JSON, unknown-field rejection, invalid IDs, output above 49152 bytes, and each state:

```rust
#[test]
fn shell_receipt_round_trips_with_exact_shape() {
    let receipt = ShellReceipt::new(
        must(ShellSessionId::new("shell-abcd-0001")),
        ShellSessionState::Running,
        None,
        "ready\n".to_owned(),
        false,
    )
    .expect("valid receipt");
    let value = must(serde_json::to_value(&receipt));
    assert_eq!(
        value,
        json!({
            "session_id": "shell-abcd-0001",
            "state": "running",
            "output": "ready\n",
            "output_truncated": false
        })
    );
    assert_eq!(must(serde_json::from_value::<ShellReceipt>(value)), receipt);
}
```

- [ ] **Step 2: Run protocol tests and verify red**

```powershell
cargo test -p minimax-protocol --test shell_roundtrip --locked
```

Expected: compilation fails because the Shell protocol module does not exist.

- [ ] **Step 3: Implement validated protocol types**

`ShellSessionId::new` must reject empty, non-ASCII, IDs without `shell-`, bytes outside `[a-zA-Z0-9-]`, and IDs longer than 128 bytes. `ShellReceipt::new` and deserialization validation must reject output larger than 49152 bytes or containing NUL. Add `ToolValidationError::InvalidShellReceipt` rather than mapping invalid receipts to an unrelated path error.

- [ ] **Step 4: Add failing buffer tests for chunk boundaries and truncation**

Add these exact cases:

```rust
#[test]
fn split_utf8_and_split_ansi_sequences_normalize_once() {
    let mut buffer = ShellOutputBuffer::default();
    buffer.append(&[0xe4, 0xbd]);
    buffer.append(&[0xa0, 0xe5, 0xa5, 0xbd]);
    buffer.append(b"\x1b[31");
    buffer.append(b"m red\x1b[0m\n");
    buffer.finish();
    assert_eq!(buffer.take(1024).output, "你好 red\n");
}

#[test]
fn unread_ring_drops_oldest_bytes_and_reports_truncation_once() {
    let mut buffer = ShellOutputBuffer::default();
    buffer.append(&vec![b'a'; MAX_SHELL_UNREAD_BYTES]);
    buffer.append(b"tail");
    let first = buffer.take(MAX_SHELL_OUTPUT_BYTES);
    assert!(first.truncated);
    assert_eq!(buffer.unread_bytes(), MAX_SHELL_UNREAD_BYTES - MAX_SHELL_OUTPUT_BYTES);
    let second = buffer.take(MAX_SHELL_OUTPUT_BYTES);
    assert!(!second.truncated);
}
```

Also test NUL/control removal, OSC sequences split across chunks, `\r`, `\n`, and `\t` preservation, output limits, and that `take` never splits a UTF-8 code point.

Add a multi-buffer case using one shared `ShellOutputBudget::new(8 * 1_024 * 1_024)`. Fill eight buffers to 1 MiB, append to a ninth, and assert total `used()` never exceeds 8 MiB, the ninth buffer marks truncation, and draining any buffer releases exactly the drained byte count.

- [ ] **Step 5: Run buffer tests and verify red**

```powershell
cargo test -p minimax-tools --test shell_buffer --locked
```

Expected: compilation fails because `ShellOutputBuffer` does not exist.

- [ ] **Step 6: Implement the stateful terminal normalizer and ring buffer**

Use a state machine with `Ground`, `Escape`, `Csi`, `Osc`, and `OscEscape` states. Retain incomplete UTF-8 bytes between append calls, convert invalid completed sequences with `String::from_utf8_lossy`, discard ANSI/VT control sequences, retain printable characters plus newline/carriage-return/tab, and cap the normalized unread bytes at 1 MiB by dropping from the front on UTF-8 boundaries.

Every normalized append reserves bytes from the shared `ShellOutputBudget`. When the global 8 MiB budget is exhausted, first discard this buffer's oldest unread bytes; if other buffers still own the entire budget, discard the incoming prefix and set `truncated=true`. `take`, terminal cleanup, and Drop release the exact owned count. No buffer may reserve more than 1 MiB.

The `take` implementation must use:

```rust
let requested = max_bytes.min(self.unread.len());
let boundary = utf8_floor_boundary(&self.unread, requested);
let bytes = self.unread.drain(..boundary).collect::<Vec<_>>();
let output = match String::from_utf8(bytes) {
    Ok(output) => output,
    Err(error) => String::from_utf8_lossy(error.as_bytes()).into_owned(),
};
ShellOutputChunk {
    output,
    truncated: std::mem::take(&mut self.truncated),
}
```

- [ ] **Step 7: Run focused tests and formatting**

```powershell
cargo test -p minimax-protocol --test shell_roundtrip --locked
cargo test -p minimax-tools --test shell_buffer --locked
cargo fmt --all -- --check
```

Expected: all tests pass and formatting is clean.

- [ ] **Step 8: Commit protocol and buffer**

```powershell
git add crates/protocol/src/shell.rs crates/protocol/src/lib.rs crates/protocol/src/tool.rs crates/protocol/tests/shell_roundtrip.rs crates/tools/src/shell/mod.rs crates/tools/src/shell/buffer.rs crates/tools/src/lib.rs crates/tools/tests/shell_buffer.rs
git commit -m "feat(shell): define bounded session receipts"
```

---

### Task 3: Build the Session Manager Against a Fake PTY Backend

**Files:**

- Create: `crates/tools/src/shell/backend.rs`
- Create: `crates/tools/src/shell/manager.rs`
- Create: `crates/tools/tests/shell_manager.rs`
- Modify: `crates/tools/src/shell/mod.rs`
- Modify: `crates/tools/src/lib.rs`
- Modify: `crates/tools/Cargo.toml`

**Interfaces:**

```rust
pub const MAX_RUNNING_SHELL_SESSIONS: usize = 8;
pub const MAX_TERMINAL_SHELL_RECEIPTS: usize = 32;
pub const TERMINAL_RECEIPT_TTL: Duration = Duration::from_secs(5 * 60);
pub const DEFAULT_COMMAND_YIELD: Duration = Duration::from_secs(10);
pub const DEFAULT_POLL_YIELD: Duration = Duration::from_secs(1);
pub const DEFAULT_WRITE_YIELD: Duration = Duration::from_millis(250);

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShellSpawnRequest {
    pub command: String,
    pub cwd: PathBuf,
    pub cols: u16,
    pub rows: u16,
}

pub trait PtyChild: Send {
    fn process_id(&self) -> u32;
    fn try_wait(&mut self) -> io::Result<Option<i32>>;
    fn kill(&mut self) -> io::Result<()>;
}

pub trait PtyGuard: Send {}
impl<T: Send> PtyGuard for T {}

pub struct SpawnedPty {
    pub child: Box<dyn PtyChild>,
    pub reader: Box<dyn Read + Send>,
    pub writer: Box<dyn Write + Send>,
    pub guard: Box<dyn PtyGuard>,
}

pub type PtyTerminateFuture<'a> =
    Pin<Box<dyn Future<Output = io::Result<()>> + Send + 'a>>;

pub trait PtyBackend: Send + Sync {
    fn spawn(&self, request: &ShellSpawnRequest) -> io::Result<SpawnedPty>;
    fn terminate_tree<'a>(&'a self, process_id: u32) -> PtyTerminateFuture<'a>;
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
            .map_or(0, |duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
    }
}
```

```rust
#[derive(Clone)]
pub struct ShellSessionManager {
    inner: Arc<tokio::sync::Mutex<ShellSessionRegistry>>,
    backend: Arc<dyn PtyBackend>,
    ids: Arc<dyn ShellSessionIdSource>,
    clock: Arc<dyn Clock + Send + Sync>,
    output_budget: Arc<ShellOutputBudget>,
}

impl ShellSessionManager {
    pub async fn enable(&self);
    pub async fn start(
        &self,
        request: ShellCommandRequest,
        cancellation: &dyn CancellationPort,
    ) -> Result<ShellReceipt, ShellManagerError>;
    pub async fn poll(
        &self,
        request: ShellPollRequest,
        cancellation: &dyn CancellationPort,
    ) -> Result<ShellReceipt, ShellManagerError>;
    pub async fn write(
        &self,
        request: ShellWriteRequest,
        cancellation: &dyn CancellationPort,
    ) -> Result<ShellReceipt, ShellManagerError>;
    pub async fn stop(&self, session_id: &ShellSessionId)
        -> Result<ShellReceipt, ShellManagerError>;
    pub async fn disable_and_stop_all(&self) -> Result<(), ShellCleanupError>;
    pub async fn shutdown(&self) -> Result<(), ShellCleanupError>;
}
```

- [ ] **Step 1: Write fake-backend lifecycle tests**

Create a fake backend whose spawned process has shared input/output queues and a controllable exit code. Add tests named:

- `fast_command_returns_terminal_receipt_without_running_slot`
- `long_command_returns_id_and_poll_delivers_only_new_output`
- `write_sends_exact_utf8_and_platform_enter_once`
- `ninth_running_session_fails_before_backend_spawn`
- `cancel_before_id_delivery_stops_the_spawned_tree`
- `poll_cancellation_preserves_the_running_session`
- `write_after_bytes_are_committed_can_report_indeterminate`
- `stop_is_terminal_and_idempotent`
- `disable_rejects_new_start_and_write_then_stops_all`
- `terminal_receipts_expire_by_count_and_clock`
- `manager_drop_requests_best_effort_cleanup`

Use deterministic IDs `shell-test-0001`, `shell-test-0002` and a manually advanced clock.

- [ ] **Step 2: Run manager tests and verify red**

```powershell
cargo test -p minimax-tools --test shell_manager --locked
```

Expected: compilation fails because the backend and manager contracts do not exist.

- [ ] **Step 3: Implement registry ownership and capacity checks**

Each running entry must own:

```rust
struct ShellSession {
    id: ShellSessionId,
    process_id: u32,
    child: Arc<std::sync::Mutex<Box<dyn PtyChild>>>,
    writer: Option<Arc<std::sync::Mutex<Box<dyn Write + Send>>>>,
    output: Arc<std::sync::Mutex<ShellOutputBuffer>>,
    reader: Option<std::thread::JoinHandle<()>>,
    reader_done: std::sync::mpsc::Receiver<()>,
    guard: Option<Box<dyn PtyGuard>>,
    state: ShellSessionState,
    exit_code: Option<i32>,
    terminal_at_unix_ms: Option<u64>,
}
```

Reserve a running slot before spawn, release it on every spawn failure, and publish the ID only after child/reader/writer/guard ownership is complete. The reader thread loops on blocking `read`, appends into the shared bounded buffer, calls `finish` on EOF, and never stores unbounded chunks.

- [ ] **Step 4: Implement start/poll/write**

`start` spawns with 120×30, waits until exit, cancellation, or `yield_time_ms`, and returns either terminal state or `running`. If cancellation wins before returning a running receipt, call the common stop path.

`poll` refreshes child state with `try_wait`, waits only up to the request yield, drains at most `max_output_bytes`, and does not mutate the process on cancellation.

`write` validates running state before spawning a blocking writer operation. Write `input.as_bytes()` exactly; when `submit` is true append `b"\r"` on Windows and `b"\n"` on Linux. Flush once after the combined write. Track whether the write crossed the side-effect boundary so cancellation maps to cancelled before write and indeterminate after write.

- [ ] **Step 5: Implement common stop, disable, shutdown, and receipt GC**

Stop sequence:

```rust
write_interrupt(session);                  // ETX: b"\x03"
wait_for_exit(session, Duration::from_secs(2)).await;
if session_is_running(session) {
    backend.terminate_tree(session.process_id).await?;
    child_kill_as_fallback(session);
}
confirm_exit(session, Duration::from_secs(2)).await?;
close_handles_and_join_reader(session);
```

`close_handles_and_join_reader` takes and drops writer/guard ownership, waits at most 2 seconds for `reader_done`, then joins only after the done signal proves the join is non-blocking. A missing done signal returns indeterminate cleanup and detaches the std thread instead of hanging shutdown. `disable_and_stop_all` sets `accepting=false` under the registry lock before collecting IDs. It stops sessions without holding the registry lock across await points, aggregates IDs whose exit cannot be confirmed, and returns one `ShellCleanupError { session_ids }`. `shutdown` delegates to the same function. GC runs before every public operation and retains terminal metadata only under both count and age limits.

- [ ] **Step 6: Run manager tests including Tokio paused-time cases**

```powershell
cargo test -p minimax-tools --test shell_manager --locked -- --nocapture
```

Expected: all lifecycle, cancellation, capacity, and GC cases pass with no sleeping test longer than the fake clock advances.

- [ ] **Step 7: Commit the fake-backed manager**

```powershell
git add crates/tools/Cargo.toml crates/tools/src/shell/backend.rs crates/tools/src/shell/manager.rs crates/tools/src/shell/mod.rs crates/tools/src/lib.rs crates/tools/tests/shell_manager.rs
git commit -m "feat(shell): manage bounded interactive sessions"
```

---

### Task 4: Add the Native Windows/Linux PTY Backend

**Files:**

- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Modify: `crates/tools/Cargo.toml`
- Create: `crates/tools/src/shell/native.rs`
- Modify: `crates/tools/src/shell/mod.rs`
- Modify: `crates/tools/src/process.rs`
- Modify: `crates/tools/tests/process_tools.rs`
- Modify: `crates/tools/tests/shell_manager.rs`

**Interfaces:**

```rust
#[derive(Clone, Copy, Debug, Default)]
pub struct NativePtyBackend;

impl PtyBackend for NativePtyBackend {
    fn spawn(&self, request: &ShellSpawnRequest) -> io::Result<SpawnedPty>;
    fn terminate_tree<'a>(&'a self, process_id: u32) -> PtyTerminateFuture<'a>;
}

pub(crate) async fn terminate_process_tree(process_id: u32) -> io::Result<()>;
```

- [ ] **Step 1: Add exact workspace dependencies and verify the lock diff**

Add:

```toml
[workspace.dependencies]
getrandom = "=0.4.3"
portable-pty = "=0.9.0"
```

and in `crates/tools/Cargo.toml`:

```toml
getrandom.workspace = true
portable-pty.workspace = true
```

Run:

```powershell
cargo update -p portable-pty --precise 0.9.0
cargo update -p getrandom@0.4.3 --precise 0.4.3
cargo tree -p minimax-tools
```

Expected: `portable-pty 0.9.0` and `getrandom 0.4.3` appear under `minimax-tools`; no Node/Pi package is introduced.

- [ ] **Step 2: Add failing pure Shell-resolution tests**

Refactor resolution into pure helpers that receive candidate paths and an `is_executable` callback. Test:

```rust
#[test]
fn windows_prefers_pwsh_then_powershell_and_never_cmd() { /* exact temp executables */ }

#[test]
fn linux_prefers_absolute_executable_shell_then_bash_then_sh() { /* exact temp executables */ }
```

Also assert the argv contracts:

```text
Windows: pwsh.exe -NoLogo -NoProfile -Command <command>
Linux:   /resolved/shell -lc <command>
```

- [ ] **Step 3: Run resolver tests and verify red**

```powershell
cargo test -p minimax-tools --test shell_manager native_shell_resolution --locked
```

Expected: compilation fails because `NativePtyBackend` and resolver helpers do not exist.

- [ ] **Step 4: Implement `portable-pty` spawn without a piped fallback**

Use:

```rust
let pair = portable_pty::native_pty_system().openpty(PtySize {
    rows: request.rows,
    cols: request.cols,
    pixel_width: 0,
    pixel_height: 0,
})?;
let mut command = CommandBuilder::new(resolved.program);
command.args(resolved.args);
command.cwd(&request.cwd);
let child = pair.slave.spawn_command(command)?;
drop(pair.slave);
let reader = pair.master.try_clone_reader()?;
let writer = pair.master.take_writer()?;
```

Wrap the `portable_pty::Child` trait object in the plan's `PtyChild` adapter, require `process_id()` before returning success, and keep the master object inside the guard. Do not call `env_clear`; the command inherits the application environment. Implement `ProcessShellSessionIds` with one process nonce from `getrandom::fill`, encode it as lowercase hex, and combine it with an atomic counter through `ShellSessionIdSource::next_session_id`.

- [ ] **Step 5: Share the existing process-tree termination function**

Change only visibility and focused helpers in `process.rs`; both `TokioDirectChild` and `NativePtyBackend` must call one `terminate_process_tree(process_id)` implementation. Keep Linux TERM→50 ms→KILL and Windows `System32/taskkill.exe /PID <pid> /T /F` behavior. Add a regression proving the old bounded diagnostic child still invokes the same cleanup path.

- [ ] **Step 6: Run build, unit, license, and unsafe-source checks**

```powershell
cargo check --workspace --all-targets --locked
cargo test -p minimax-tools --test process_tools --locked
cargo test -p minimax-tools --test shell_manager --locked
cargo tree -p minimax-tools -e features
$metadata = cargo metadata --format-version 1 --locked | ConvertFrom-Json
$metadata.packages | Where-Object { $_.name -in @('portable-pty','getrandom') } | Select-Object name,version,license,rust_version
rg -n "unsafe\s*\{" crates Cargo.toml
```

Expected: all Rust checks pass; `portable-pty` is exactly 0.9.0 with an MIT-compatible license, `getrandom` is exactly 0.4.3 with MIT/Apache-2.0-compatible licensing, both satisfy Rust 1.97, and first-party `crates/**` contains no unsafe block.

- [ ] **Step 7: Commit the native backend**

```powershell
git add Cargo.toml Cargo.lock crates/tools/Cargo.toml crates/tools/src/process.rs crates/tools/src/shell/native.rs crates/tools/src/shell/mod.rs crates/tools/tests/process_tools.rs crates/tools/tests/shell_manager.rs
git commit -m "feat(shell): launch native PTY processes"
```

---

### Task 5: Register the Two Shell Tools and Enforce Full-Access Preflight

**Files:**

- Modify: `crates/protocol/src/tool.rs`
- Modify: `crates/protocol/src/lib.rs`
- Modify: `crates/tools/src/error.rs`
- Modify: `crates/tools/src/policy.rs`
- Modify: `crates/tools/src/adapter.rs`
- Modify: `crates/tools/src/lib.rs`
- Create: `crates/tools/src/shell/command.rs`
- Create: `crates/tools/src/shell/session.rs`
- Create: `crates/tools/tests/shell_tools.rs`
- Modify: `crates/tools/tests/tool_schemas.rs`
- Create: `fixtures/compat/tools/full-access-schemas.v1.json`

**Interfaces:**

Keep the existing public base list unchanged and add versioned full-access lists:

```rust
pub const V1_TOOL_NAMES: [&str; 8] = [/* existing exact order */];
pub const SHELL_TOOL_NAMES: [&str; 2] = ["shell_command", "shell_session"];
pub const FULL_ACCESS_TOOL_NAMES: [&str; 10] = [
    "read_file",
    "list_directory",
    "apply_patch",
    "write_file",
    "run_diagnostic",
    "git_status",
    "git_diff",
    "npm_diagnostic",
    "shell_command",
    "shell_session",
];
```

```rust
impl ToolRegistry {
    pub fn all_specs() -> Result<Vec<ToolSpec>, ToolValidationError>;
    pub fn specs_for(mode: PermissionMode) -> Result<Vec<ToolSpec>, ToolValidationError>;
    pub fn find(name: &str) -> Result<Option<ToolSpec>, ToolValidationError>;
}
```

- [ ] **Step 1: Add failing schema fixture tests**

Keep the current `fixtures/compat/tools/v1-schemas.json` and its eight-tool assertion byte-identical. Add a second test that `ToolRegistry::specs_for(FullAccess)` equals `full-access-schemas.v1.json`, contains the exact ten-name order, and has strict `additionalProperties=false` for both new tools.

The `shell_command` schema is:

```json
{
  "additionalProperties": false,
  "properties": {
    "command": {"minLength": 1, "maxLength": 32768, "type": "string"},
    "cwd": {"maxLength": 4096, "minLength": 1, "type": "string"},
    "yield_time_ms": {"minimum": 250, "maximum": 60000, "type": "integer"},
    "max_output_bytes": {"minimum": 1024, "maximum": 49152, "type": "integer"}
  },
  "required": ["command"],
  "type": "object"
}
```

The `shell_session` schema is:

```json
{
  "additionalProperties": false,
  "properties": {
    "session_id": {"minLength": 1, "maxLength": 128, "type": "string"},
    "action": {"enum": ["poll", "write", "stop"], "type": "string"},
    "input": {"maxLength": 16384, "type": "string"},
    "submit": {"type": "boolean"},
    "yield_time_ms": {"minimum": 0, "maximum": 60000, "type": "integer"},
    "max_output_bytes": {"minimum": 1024, "maximum": 49152, "type": "integer"}
  },
  "required": ["session_id", "action"],
  "type": "object"
}
```

- [ ] **Step 2: Add failing permission and argument-combination tests**

Add table rows for:

- confirm `shell_command` → `shell_requires_full_access`, zero approval/execution;
- full-access command containing `.env`, `password=`, absolute cwd, pipe, redirect, and network command → accepted by common preflight;
- wrong effect → `effect_mismatch`;
- blank/oversized command → `invalid_arguments`/`input_limit`;
- poll with input/submit → `invalid_arguments`;
- write with neither input nor submit → `invalid_arguments`;
- stop with input, submit, or nonzero yield → `invalid_arguments`;
- unknown session → `shell_session_not_found`;
- ninth session → `shell_session_limit`.

- [ ] **Step 3: Run tool tests and verify red**

```powershell
cargo test -p minimax-tools --test tool_schemas --locked
cargo test -p minimax-tools --test shell_tools --locked
```

Expected: compilation or assertion failure because the new constants, schemas, denial codes, and adapters do not exist.

- [ ] **Step 4: Implement denial codes and mode-aware common preflight**

Add exact denial/failure codes to `ToolDenialCode`:

```rust
ShellRequiresFullAccess => "shell_requires_full_access",
ShellSessionNotFound => "shell_session_not_found",
ShellSessionLimit => "shell_session_limit",
ShellLaunchFailed => "shell_launch_failed",
ShellStopIndeterminate => "shell_stop_indeterminate",
```

Define `shell_running`, `shell_exited`, `shell_nonzero_exit`, and `shell_stopped` as Shell adapter result-code constants; they are successful/terminal outcomes, not policy denials.

`Preflight::check` receives `ToolExecutionContext`. It validates name/schema/effect for all tools. For the two Shell names it first requires full-access/disabled, then returns without calling `validate_relative_path`, `ensure_public_path`, `scan_argument_content`, or `ensure_safe_output`. Existing tools continue through the old common path unchanged.

- [ ] **Step 5: Implement argument parsers and ToolResult mapping**

`ShellCommandTool` resolves relative cwd against `WorkspaceRoot`, canonicalizes only to confirm the directory exists, and does not enforce workspace containment. `ShellSessionTool` parses the action-specific combination before manager invocation.

Serialize every successful or failed manager receipt with `serde_json::to_string`. Map state/exit exactly:

```rust
match (receipt.state, receipt.exit_code) {
    (ShellSessionState::Running, _) => (Succeeded, "shell_running"),
    (ShellSessionState::Exited, Some(0)) => (Succeeded, "shell_exited"),
    (ShellSessionState::Exited, _) => (Failed, "shell_nonzero_exit"),
    (ShellSessionState::Stopped, _) => (Succeeded, "shell_stopped"),
    (ShellSessionState::Failed, _) => (Failed, "shell_launch_failed"),
}
```

The JSON result must validate under `MAX_TOOL_RESULT_BYTES` before return.

- [ ] **Step 6: Wire BuiltinToolPort production and test constructors**

Add one shared `ShellSessionManager` field. `production` uses `NativePtyBackend`, system clock, and process ID source. Tests can construct with an injected manager. Dispatch the two exact names only after common preflight. `definitions(mode)` delegates to `ToolRegistry::specs_for(mode)`.

- [ ] **Step 7: Run focused and existing eight-tool contracts**

```powershell
cargo test -p minimax-tools --test tool_schemas --locked
cargo test -p minimax-tools --test shell_tools --locked
cargo test -p minimax-tools --test workspace_tools --locked
cargo test -p minimax-tools --test process_tools --locked
```

Expected: both schema fixtures pass; old eight-tool denial matrix remains identical; all Shell policy cases pass.

- [ ] **Step 8: Commit schemas and adapters**

```powershell
git add crates/protocol/src/tool.rs crates/protocol/src/lib.rs crates/tools/src/error.rs crates/tools/src/policy.rs crates/tools/src/adapter.rs crates/tools/src/lib.rs crates/tools/src/shell/command.rs crates/tools/src/shell/session.rs crates/tools/tests/shell_tools.rs crates/tools/tests/tool_schemas.rs fixtures/compat/tools/full-access-schemas.v1.json
git commit -m "feat(shell): expose full-access command tools"
```

---

### Task 6: Connect Dynamic Provider Definitions, Permission Downgrade, and Shutdown

**Files:**

- Modify: `crates/core/src/ports.rs`
- Modify: `crates/core/src/lib.rs`
- Modify: `crates/core/src/trace.rs`
- Modify: `crates/core/tests/compaction_trace.rs`
- Modify: `crates/protocol/src/session.rs`
- Modify: `crates/tools/src/adapter.rs`
- Modify: `crates/cli/src/driver.rs`
- Modify: `crates/cli/src/main.rs`
- Modify: `crates/cli/src/doctor.rs`
- Modify: `crates/cli/tests/tool_loop.rs`
- Modify: `crates/cli/tests/headless.rs`
- Modify: `crates/cli/tests/restart.rs`

**Interfaces:**

```rust
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolLifecycleError {
    pub code: &'static str,
    pub session_ids: Vec<String>,
}

pub type ToolLifecycleFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), ToolLifecycleError>> + Send + 'a>>;

pub trait ToolPort: Send + Sync {
    // preflight and execute from Task 1
    fn transition_permission<'a>(&'a self, mode: PermissionMode) -> ToolLifecycleFuture<'a> {
        Box::pin(async { Ok(()) })
    }
    fn shutdown<'a>(&'a self) -> ToolLifecycleFuture<'a> {
        Box::pin(async { Ok(()) })
    }
}
```

Add `DriverError::ToolLifecycle(ToolLifecycleError)` with a Display implementation that prints the stable code and comma-separated session IDs without command text or output.

Add one safe trace code:

```rust
TraceCode::ToolCompleted
```

Its only allowed facts are `tool`, `session_id`, `state`, `exit_code`, `output_bytes`, `truncated`, and `elapsed_ms`. The command string, cwd, interactive input, and output body are never allowed facts.

```rust
impl<P: ProviderPort> RuntimeDriver<P> {
    pub async fn set_permission_mode(
        &mut self,
        mode: PermissionMode,
    ) -> Result<(), DriverError>;

    pub async fn shutdown_tools(&self) -> Result<(), DriverError>;
}
```

- [ ] **Step 1: Add failing Agent-definition tests**

In `tool_loop.rs`, capture Provider requests and assert:

```rust
assert_eq!(tool_names(&confirm_request), V1_TOOL_NAMES);
assert_eq!(tool_names(&full_access_request), FULL_ACCESS_TOOL_NAMES);
```

Add a scripted Provider response that returns a forged `shell_command` in confirm mode even though the schema was absent. Assert approval call count is zero, fake backend spawn count is zero, and the durable ToolResult code is `shell_requires_full_access`.

- [ ] **Step 2: Add failing permission-lifecycle tests**

Cover:

- full-access skips approval and starts one persistent fake session;
- switching to confirm marks driver mode confirm before cleanup, rejects concurrent new Shell work, and stops all sessions;
- cleanup failure keeps mode confirm and returns a `DriverError` containing the exact failed IDs;
- repeated confirm/full-access transitions are idempotent;
- startup `--permission full-access` enables the manager;
- restart begins confirm and old ID returns not found;
- normal completed, interrupted, and failed CLI exits each invoke shutdown exactly once.
- Shell tool completion records only the allowlisted metadata and serialized safe trace contains none of the command, cwd, input, or output body.

- [ ] **Step 3: Run CLI tests and verify red**

```powershell
cargo test -p minimax-cli --test tool_loop --locked
cargo test -p minimax-cli --test headless --locked
cargo test -p minimax-cli --test restart --locked
```

Expected: tests fail because definitions are static, permission setter is synchronous, and ToolPort has no lifecycle hooks.

- [ ] **Step 4: Add lifecycle defaults and BuiltinToolPort overrides**

Use the interface above. `BuiltinToolPort::transition_permission(FullAccess)` calls `manager.enable()`. Transition to confirm calls `manager.disable_and_stop_all()`. `shutdown` calls `manager.shutdown()`. Map failed IDs into `ToolLifecycleError { code: "shell_stop_indeterminate", session_ids }`.

- [ ] **Step 5: Make provider tool definitions permission-aware per Agent turn**

Keep all validated definitions in the driver, but filter only exact Shell names:

```rust
fn tool_definitions_for(&self, mode: PermissionMode) -> Vec<ToolDefinition> {
    self.tool_definitions
        .iter()
        .filter(|definition| {
            mode == PermissionMode::FullAccess
                || !SHELL_TOOL_NAMES.contains(&definition.name.as_str())
        })
        .cloned()
        .collect()
}
```

At `run_agent_with` start, snapshot `let mode = self.permission_mode;` and attach `tool_definitions_for(mode)` to every Provider continuation request for that turn.

- [ ] **Step 6: Make permission transition async and fail closed**

Implement ordering exactly:

```rust
pub async fn set_permission_mode(&mut self, mode: PermissionMode) -> Result<(), DriverError> {
    if self.permission_mode == mode {
        return Ok(());
    }
    match mode {
        PermissionMode::FullAccess => {
            self.tools.transition_permission(mode).await.map_err(DriverError::ToolLifecycle)?;
            self.permission_mode = mode;
            Ok(())
        }
        PermissionMode::Confirm => {
            self.permission_mode = PermissionMode::Confirm;
            self.tools
                .transition_permission(PermissionMode::Confirm)
                .await
                .map_err(DriverError::ToolLifecycle)
        }
    }
}
```

Update startup and `/permissions` to await it. On cleanup error, print the confirm status plus a high-priority warning listing failed session IDs. Do not silently restore full-access.

- [ ] **Step 7: Route every normal CLI exit through tool shutdown**

Change `finish_chat_session` to receive `&mut RuntimeDriver<P>`, call `shutdown_tools().await` before Wiki finalization, and preserve the original exit class while printing cleanup failure. Make all return paths use this helper. The driver/port Drop path remains best effort and must not claim confirmed cleanup.

- [ ] **Step 8: Update permission status text**

Full-access text must say:

```text
permission mode: full-access | approval: skipped | subprocess sandbox: disabled-by-full-access | arbitrary Shell: enabled for this process | commands can access host files, network, and environment credentials; tool output is persisted locally and sent to the configured Provider
```

Confirm text must explicitly say `arbitrary Shell: disabled`.

- [ ] **Step 9: Record bounded Shell metadata in safe trace**

Add `ToolCompleted` to `TraceCode` and the exact allowlist above to `SafeTraceRecorder`. Around `tools.execute`, measure elapsed milliseconds with the existing monotonic budget clock. After the terminal ToolResult is durable, parse a valid Shell receipt and record:

```rust
BTreeMap::from([
    ("tool".to_owned(), SafeTraceFact::String(result.tool_name.clone())),
    ("session_id".to_owned(), SafeTraceFact::String(receipt.session_id.as_str().to_owned())),
    ("state".to_owned(), SafeTraceFact::String(shell_state_name(receipt.state).to_owned())),
    ("exit_code".to_owned(), receipt.exit_code.map_or(SafeTraceFact::Null, |code| SafeTraceFact::I64(i64::from(code)))),
    ("output_bytes".to_owned(), SafeTraceFact::U64(u64::try_from(receipt.output.len()).unwrap_or(u64::MAX))),
    ("truncated".to_owned(), SafeTraceFact::Bool(receipt.output_truncated)),
    ("elapsed_ms".to_owned(), SafeTraceFact::U64(elapsed_ms)),
])
```

Add a trace regression containing a secret-looking command, cwd, input, and output; only the metadata above may survive.

- [ ] **Step 10: Run CLI and Agent regression tests**

```powershell
cargo test -p minimax-cli --test tool_loop --locked
cargo test -p minimax-cli --test headless --locked
cargo test -p minimax-cli --test restart --locked
cargo test -p minimax-core --test tool_machine --locked
cargo test -p minimax-core --test compaction_trace --locked
```

Expected: all dynamic-schema, no-approval, forged-call, transition, shutdown, and restart cases pass.

- [ ] **Step 11: Commit driver lifecycle integration**

```powershell
git add crates/protocol/src/session.rs crates/core/src/ports.rs crates/core/src/lib.rs crates/core/src/trace.rs crates/core/tests/compaction_trace.rs crates/tools/src/adapter.rs crates/cli/src/driver.rs crates/cli/src/main.rs crates/cli/src/doctor.rs crates/cli/tests/tool_loop.rs crates/cli/tests/headless.rs crates/cli/tests/restart.rs
git commit -m "feat(shell): bind sessions to full-access lifecycle"
```

---

### Task 7: Prove Real PTY Behavior, Render Results, and Update Release Truth

**Files:**

- Create: `crates/tools/tests/shell_pty.rs`
- Modify: `crates/tui/src/render.rs`
- Modify: `crates/tui/tests/command_render.rs`
- Modify: `README.md`
- Modify: `docs/release/subprocess-sandbox.md`
- Modify: `docs/verification/coding-agent-execution-plane.md`
- Modify: `.planning/phases/MMX-08-codex-style-subprocess-sandbox-hardening/08-SPEC.md`
- Modify: `.github/workflows/ci.yml`
- Modify: `crates/compat-harness/src/source_authority.rs`
- Modify: `crates/compat-harness/tests/source_authority.rs`
- Modify: `crates/compat-harness/src/manifest.rs`
- Modify: `crates/compat-harness/src/baseline.rs`
- Modify: `crates/compat-harness/tests/compat_report.rs`
- Modify: `fixtures/compat/public-contract.v1.json`
- Modify: `fixtures/compat/report.expected.json`

**Interfaces:**

Specialize rendering only for valid Shell receipts:

```rust
pub fn tool_result(result: &ToolResult) -> String {
    if SHELL_TOOL_NAMES.contains(&result.tool_name.as_str())
        && let Some(output) = result.output.as_deref()
        && let Ok(receipt) = serde_json::from_str::<ShellReceipt>(output)
    {
        return render_shell_receipt(result, &receipt);
    }
    render_generic_tool_result(result)
}
```

Rendered form:

```text
shell | session=shell-abcd-0001 | state=running | exit=none | truncated=false
server listening on 3000
```

- [ ] **Step 1: Add real PTY tests with platform-specific command fixtures**

Use one helper returning exact commands:

```rust
#[cfg(windows)]
fn command_fixture(kind: FixtureKind) -> &'static str {
    match kind {
        FixtureKind::Fast => "Write-Output 'fast-ready'",
        FixtureKind::Nonzero => "Write-Output 'failed'; exit 7",
        FixtureKind::Long => "Write-Output 'first'; Start-Sleep -Seconds 2; Write-Output 'second'",
        FixtureKind::Prompt => "$v = Read-Host 'value'; Write-Output \"got:$v\"",
        FixtureKind::Tty => "Write-Output \"in=$(-not [Console]::IsInputRedirected);out=$(-not [Console]::IsOutputRedirected)\"",
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
```

Tests must cover fast exit, exit 7, long incremental output without duplication, prompt write+submit, TTY detection, default/relative/outside cwd, Unicode/emoji, native pipes/redirects, 1 MiB truncation, explicit stop, parent+child tree stop, downgrade cleanup, and normal shutdown cleanup. Use bounded waits and print process IDs so survivor checks are deterministic.

- [ ] **Step 2: Run the real PTY suite locally and verify failures are behavioral**

```powershell
cargo test -p minimax-tools --test shell_pty --locked -- --nocapture
```

Expected before final fixes: at least one platform integration assertion exposes any quoting, EOF, interrupt, child-tree, or handle-close defect. Fix production code; do not weaken timeouts or skip the supported host.

- [ ] **Step 3: Add Shell-specific TUI rendering tests**

Assert a running receipt, terminal exit 7, truncated output, malformed JSON fallback, and the 16000-character renderer bound. Implement the specialization above without changing generic tool rendering.

- [ ] **Step 4: Update documentation and historical supersession**

Document in non-programmer-readable language:

- exactly two permission modes and process-scoped reset;
- confirm keeps arbitrary Shell disabled;
- full-access exposes arbitrary host Shell with no per-command confirmation;
- commands can access host files/network/environment credentials;
- ToolResult output persists locally and is sent to the configured Provider;
- two tools and their start/poll/write/stop flow;
- Windows/Linux support and macOS deferral;
- no Pi/Node/tmux runtime;
- normal cleanup guarantee and forced-process-exit limitation.

At the top of Phase 8 spec add a dated note saying its fixed-tool-only full-access statement remains historical for Phase 8 and is superseded for `shell_command`/`shell_session` by the 2026-07-21 design. Do not rewrite completed Phase 8 evidence.

- [ ] **Step 5: Bind the seven Shell requirements into compatibility authority**

Add `contract.requirement.SHELL-01` through `contract.requirement.SHELL-07` to `expected_contract_ids` and to the public-contract fixture in exact lexical order. Evidence mapping is:

```text
SHELL-01 protocol and schemas -> crates/protocol/tests/shell_roundtrip.rs, crates/tools/tests/tool_schemas.rs, fixtures/compat/tools/full-access-schemas.v1.json
SHELL-02 full-access policy   -> crates/cli/tests/tool_loop.rs, crates/tools/tests/shell_tools.rs
SHELL-03 session lifecycle   -> crates/tools/tests/shell_manager.rs
SHELL-04 native PTY          -> crates/tools/tests/shell_pty.rs, .github/workflows/ci.yml
SHELL-05 resource bounds     -> crates/tools/tests/shell_buffer.rs, crates/tools/tests/shell_manager.rs
SHELL-06 cleanup             -> crates/tools/tests/shell_pty.rs, crates/cli/tests/tool_loop.rs
SHELL-07 truthful docs       -> README.md, docs/release/subprocess-sandbox.md, docs/verification/coding-agent-execution-plane.md
```

Add `BaselineError::ShellEvidence(String)` and `validate_rust_shell_evidence`, requiring every item to be matched and every evidence path to exist. Call it from the same verification/report paths as `validate_rust_tool_evidence`. Add mutation tests that reject a missing ID, pending status, and missing evidence file.

After applying the fixture edit, compute the exact fingerprint without writing files from the command:

```powershell
node -e "const fs=require('fs'),c=JSON.parse(fs.readFileSync('fixtures/compat/public-contract.v1.json','utf8')),h=require('crypto').createHash('sha256'),i={schemaVersion:c.schemaVersion,contractVersion:c.contractVersion,provenanceCommit:c.provenanceCommit,productEntry:c.productEntry,requiredItemIds:c.requiredItemIds,items:c.items}; console.log('sha256:'+h.update(JSON.stringify(i)).digest('hex'))"
```

Apply the printed value to `contentFingerprint`, run `cargo run -p minimax-compat-harness --locked -- report --format json`, and update `fixtures/compat/report.expected.json` through `apply_patch` with the seven new ordered rows and fingerprint. Do not change unrelated historical rows.

- [ ] **Step 6: Add an explicit hosted PTY step on both matrix platforms**

Insert after `npm run check:rust`:

```yaml
      - name: Run native PTY Shell integration
        run: cargo test -p minimax-tools --test shell_pty --locked -- --nocapture
```

Do not make it Linux-only or allow failure.

- [ ] **Step 7: Make CI source authority require the PTY step**

Extend `validate_ci_workflow_text` to require exactly one `Run native PTY Shell integration` step with the exact cargo command and no `if:` or `continue-on-error`. Add source-authority mutations that remove the step, add a Linux-only condition, and add `continue-on-error: true`; each mutation must fail with `CI must run native PTY Shell integration on every hosted platform`.

- [ ] **Step 8: Run focused UI/docs/compatibility checks**

```powershell
cargo test -p minimax-tui --test command_render --locked
cargo test -p minimax-tools --test shell_pty --locked -- --nocapture
cargo test -p minimax-compat-harness --test source_authority --locked
cargo test -p minimax-compat-harness --test compat_report --locked
npm run verify:rust-contracts:candidate
npm run test:package
```

Expected: all pass; candidate verification may report only the explicitly expected hosted-evidence freshness status, not source/schema/architecture failures.

- [ ] **Step 9: Run the complete local quality gate**

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --all-features --locked
npm run eval:provider
npm run eval:retrieval
npm run verify:rust-contracts:candidate
npm run test:package
git diff --check
```

Expected: every local gate passes. Hosted freshness remains an external release step and is reported, not bypassed.

- [ ] **Step 10: Inspect product/runtime boundaries before commit**

```powershell
rg -n "@earendil-works|pi-coding-agent|node_modules|tmux|cmd\.exe" Cargo.toml Cargo.lock crates package.json package-lock.json
git diff --name-status 5351d32...HEAD
git status --short
```

Expected: no Pi/Node/tmux runtime dependency or cmd fallback; only approved product, test, fixture, CI, documentation, and GSD files changed.

- [ ] **Step 11: Commit verification and documentation**

```powershell
git add crates/tools/tests/shell_pty.rs crates/tui/src/render.rs crates/tui/tests/command_render.rs README.md docs/release/subprocess-sandbox.md docs/verification/coding-agent-execution-plane.md .planning/phases/MMX-08-codex-style-subprocess-sandbox-hardening/08-SPEC.md .github/workflows/ci.yml crates/compat-harness/src/source_authority.rs crates/compat-harness/tests/source_authority.rs crates/compat-harness/src/manifest.rs crates/compat-harness/src/baseline.rs crates/compat-harness/tests/compat_report.rs fixtures/compat/public-contract.v1.json fixtures/compat/report.expected.json
git commit -m "docs(shell): verify full-access terminal contract"
```

- [ ] **Step 12: Request whole-branch review**

Review the entire branch against `docs/superpowers/specs/2026-07-21-full-access-shell-design.md`, with special attention to forged confirm-mode calls, output memory, write cancellation, process-tree survivors, permission downgrade, normal shutdown, Provider output disclosure, and old eight-tool regressions. Resolve every Critical or Important issue, rerun the complete gate, and report hosted release evidence as pending external work.

---

## Final Acceptance Checklist

- [ ] `confirm` Provider requests expose exactly the original eight tools.
- [ ] `full-access` Provider requests expose exactly ten tools.
- [ ] A forged Shell call in confirm mode performs zero approval and zero process work.
- [ ] Full-access Shell commands run without per-command confirmation.
- [ ] Fast, nonzero, long-running, interactive, and stop flows pass on real Windows and Linux PTYs.
- [ ] Poll returns only new output and never exceeds 49152 Shell-output bytes per ToolResult.
- [ ] Per-session unread output never exceeds 1 MiB and eight sessions never exceed 8 MiB.
- [ ] Stop, downgrade, and normal shutdown terminate the tested parent/child process tree.
- [ ] Restart returns to confirm and old session IDs are invalid.
- [ ] Existing eight-tool schemas, denials, sandbox behavior, and tests remain unchanged.
- [ ] TUI output is readable and bounded without introducing a terminal emulator.
- [ ] README, permission status, sandbox docs, verification docs, Phase 8 note, and CI describe one consistent contract.
- [ ] Cargo/npm dependencies contain no Pi, TypeScript Agent runtime, tmux, or external terminal requirement.
- [ ] Local Rust, evaluation, compatibility-candidate, and package gates pass.
- [ ] Hosted Windows/Linux evidence refresh, release tag, push, and publication remain unperformed until separately authorized.
