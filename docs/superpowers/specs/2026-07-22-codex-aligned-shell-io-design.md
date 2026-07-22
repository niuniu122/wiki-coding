# Codex-Aligned Shell I/O Boundary Repair Design

**Date:** 2026-07-22

**Status:** Approved; Task 1 implementation under review-fix

## Problem

The full-access Shell implementation currently sends every command through a
120-column PTY. On Windows, ConPTY may insert a physical line break at column
120. That changes ordinary command output: a long working-directory path or a
single long machine-readable line can be split even though the command never
emitted a newline.

The public contract also accepts commands up to 32 KiB, but the authenticated
Windows host launches PowerShell with the complete command in a `-Command`
process argument. `CreateProcess` applies its command-line limit to the
executable, flags, quoting, and command together, so a contract-valid 32 KiB
ASCII command cannot launch.

These are boundary-design defects rather than output-formatting defects:

- ordinary captured execution and interactive terminal execution are different
  I/O modes;
- the trusted host receives the command out of band, but then unnecessarily
  puts it back into the Windows process command line.

## Reference Direction

The repair follows the local reference implementations without copying their
unrelated architecture:

- Claw Code runs ordinary one-shot Shell commands with piped standard I/O;
- OpenAI Codex defaults unified execution to `tty: false` and selects a pipe or
  PTY/ConPTY launch path from that value;
- Codex uses `tty: true` only when a real terminal is requested.

MiniMax Codex retains its existing two-tool session contract, authenticated
internal host, bounded output buffers, and process-tree containment. Only the
I/O-mode selection and Windows command payload transport change.

## Authorized Windows ConPTY Boundary

Task 1 review exposed an intermittent Windows teardown failure: concurrent
terminal sessions could terminate and reap their contained process trees while
the erased `portable-pty` reader remained blocked past the fixed cleanup
deadline. A test-wide semaphore was rejected because production supports eight
concurrent sessions and independent managers.

The user explicitly authorized one narrow exception to the workspace's
no-unsafe rule. A new Windows boundary crate, `crates/windows-conpty`, may use
the minimum Windows FFI needed to own ConPTY, anonymous-pipe, reader-thread,
process, and Job-assignment handles. Its public API is safe and exposes no raw
handle. `crates/tools` and every existing workspace crate continue to inherit
`unsafe_code = "forbid"`; no generic shell lifecycle code may contain unsafe.

The boundary follows the local Codex/WezTerm ConPTY ownership pattern under its
retained MIT attribution, but does not copy Codex's non-cancellable
`spawn_blocking` reader lifecycle. The locked safety invariants are:

- create the ConPTY and both communication pipes before spawning the trusted
  host, and retain the ConPTY-side pipe handles until `ClosePseudoConsole`;
- start one dedicated output-drain thread before child activation and keep it
  draining through `ClosePseudoConsole`, including any final frame, until the
  output pipe breaks;
- assign the trusted host process to the kill-on-close Job through the
  boundary's safe API before authenticated host activation or command delivery;
- close terminal input, close the ConPTY exactly once, drain and join the output
  thread, and only then release the remaining output/process handles;
- use `CancelSynchronousIo` only as a bounded fallback after ConPTY closure and
  the normal drain interval, targeted exclusively at the owned drain thread;
- share the manager's existing single two-second reader-cleanup deadline across
  ConPTY close, final-frame drain, fallback cancellation, and both reader joins;
- preserve fixed 120x30 terminal geometry, output budgets, authentication,
  permissions, containment confirmation, and all pipe-mode behavior;
- retain `portable-pty` unchanged for Linux and other non-Windows terminal
  compilation paths.

## User-Facing Contract

`shell_command` gains one optional field:

```json
{
  "command": "cargo test",
  "cwd": "optional/path",
  "tty": false,
  "yield_time_ms": 10000,
  "max_output_bytes": 16384
}
```

- `tty` is a boolean and defaults to `false` when omitted.
- `tty: false` means ordinary captured execution through OS pipes. It preserves
  logical long lines and does not advertise a terminal to the child process.
- `tty: true` means an interactive terminal through PTY/ConPTY. Programs may
  prompt, use terminal detection, and apply normal terminal-width wrapping.
- The terminal size remains 120 columns by 30 rows for this repair. Resizing is
  a separate feature and remains out of scope.
- The maximum command remains exactly 32 KiB of UTF-8, and commands larger than
  that continue to fail with `input_limit` before manager work.
- Both modes may return `shell_running` and the same `session_id` contract.
  `shell_session` poll, write, submit, and stop continue to work in both modes.
- A program that requires a terminal must opt in with `tty: true`; changing the
  default is intentional and matches Codex.

No new tool name, permission mode, approval path, receipt field, session action,
or terminal status code is introduced.

## Internal Model

The backend API will represent the mode explicitly instead of describing every
spawn as a PTY:

```rust
pub enum ShellIoMode {
    Pipe,
    Terminal { cols: u16, rows: u16 },
}

pub struct ShellSpawnRequest {
    pub command: String,
    pub cwd: PathBuf,
    pub io_mode: ShellIoMode,
}
```

The generic backend/resource names change from `PtyBackend`, `PtyChild`,
`PtyGuard`, and `SpawnedPty` to `ShellBackend`, `ShellChild`, `ShellGuard`, and
`SpawnedShell`. `NativePtyBackend` becomes `NativeShellBackend`. These are Rust
API names inside the workspace; the JSON tool names remain unchanged.

`ShellSessionManager` maps the request as follows:

```text
tty omitted/false -> ShellIoMode::Pipe
tty true          -> ShellIoMode::Terminal { cols: 120, rows: 30 }
```

Startup cursor-query handling runs only for terminal resources. Pipe resources
never receive terminal control sequences.

## Pipe Launch Path

The native backend continues to start the trusted internal host before the user
command. The host authentication and containment sequence stays unchanged:

```text
allocate I/O -> spawn trusted host -> assign the outer boundary when applicable
-> authenticate -> activate -> host preflight/contained -> send command
-> spawn PowerShell/POSIX shell -> publish session
```

For pipe mode, the backend creates:

- one input pipe whose read end becomes the host's standard input;
- one output pipe whose write end is duplicated for the host's standard output
  and standard error;
- one reader and one writer returned to the session manager.

The `filedescriptor` crate will be declared as a direct dependency because the
locked `portable-pty` dependency already uses version 0.8.3 and its cross-
platform pipe handles support safe `Stdio` duplication. This keeps stdout and
stderr in the existing merged Shell output contract without sequential-reader
deadlocks.

On Windows, the host process is assigned to the kill-on-close Job before it is
activated and before it can start PowerShell. On Linux, the existing host
preflight and process-group/subreaper containment remain authoritative. A pipe
session therefore has the same cleanup boundary as a terminal session.

For terminal mode, Linux retains the current `portable-pty` launch path.
Windows uses the isolated ConPTY boundary above so reader teardown remains
deterministic under the same supported concurrency as the session manager.

## Windows Command Payload

The native parent stages the command before launching the trusted host. It:

1. creates an exclusive, randomly named temporary payload with a `.ps1` suffix;
2. writes the command as UTF-8, flushes it, closes the file handle, and retains
   the `TempPath` cleanup owner outside the Job/process tree;
3. supplies only that random path to the trusted host through
   `MINIMAX_SHELL_COMMAND_PATH`, while the authenticated parent-to-host protocol
   independently delivers the authoritative command text;
4. transfers the `TempPath`, child, reader, writer, and Job/ConPTY cleanup
   controls together to the manager on every post-spawn startup failure.

After containment and authentication, the trusted Windows host receives the
command and reads the staged payload. It rejects startup unless the payload text
exactly equals the authenticated command. The host then binds a loopback
acknowledgement listener with a fresh 256-bit token and launches PowerShell with
`-NoLogo -NoProfile -Command` plus one short constant bootstrap. The bootstrap:

1. captures the payload path and acknowledgement values and removes all three
   variables from PowerShell's process environment;
2. strict-decodes the payload as UTF-8 and deletes it in a `finally` path;
3. rejects startup if the payload path still exists;
4. returns the 256-bit token to the loopback listener; and only then
5. evaluates the decoded command in the command scope.

The host waits for that authenticated acknowledgement before its spawn step
succeeds and before it sends `Ready` to the native parent. The native-side
`TempPath` guard remains authoritative even though successful bootstrap cleanup
has already removed the file; dropping the guard is therefore an idempotent
fallback, not host ownership through process exit.

The user command is therefore absent from the Windows process command line, and
the 32 KiB public boundary no longer competes with executable/flag/quoting
overhead. Standard input remains available for `shell_session` writes because
the bootstrap reads the command from the payload rather than stdin.

Payload creation/writing failure occurs before host launch. Payload mismatch,
strict decoding, deletion confirmation, acknowledgement, or PowerShell spawn
failure occurs before the host sends `Ready`. A post-spawn failure returns
complete cleanup ownership to the manager, which publishes no user-visible
session ID and registers an internal unpublished session. Its running slot,
payload guard, reader/writer, child, and Job/ConPTY tree remain owned until
unified cleanup succeeds; a failed cleanup attempt remains registered for the
next downgrade/shutdown retry. Only successful cleanup releases the slot and
removes the unpublished entry. The public result is `shell_launch_failed` when
that cleanup succeeds immediately, or the existing indeterminate cleanup error
when containment/payload deletion cannot yet be confirmed.

Linux keeps its existing `-lc <command>` transport because the 32 KiB contract
is below the supported argument boundary there. The I/O-mode split still
applies on Linux.

## Output and Session Semantics

The existing bounded output and receipt behavior is unchanged:

- one merged stdout/stderr stream;
- at most 1 MiB unread output per session and 8 MiB globally;
- at most 49,152 output bytes per tool result;
- incremental delivery with `output_truncated` when unread data is lost;
- the same terminal receipt retention and garbage collection;
- the same cancellation, stop, permission downgrade, and shutdown cleanup.

Pipe output is decoded and normalized by the existing `ShellOutputBuffer`. It
must not be passed through terminal cursor emulation or line de-wrapping.
Terminal output continues through the existing VT normalization. Soft wrapping
in explicit terminal mode is expected terminal behavior and is not repaired by
guessing which newlines a terminal inserted.

## Security and Failure Boundaries

- Shell tools remain visible and executable only in `full-access`.
- The permission snapshot and execution preflight remain unchanged.
- Host authentication still precedes command delivery.
- Windows Job assignment still precedes host activation.
- Raw Windows handles and unsafe operations remain confined to the isolated
  Windows ConPTY crate; safe callers cannot construct, duplicate, cancel, or
  close an unowned handle.
- Internal host address, token, protocol version, and timeout variables are not
  inherited by PowerShell.
- The temporary payload path is removed from PowerShell's environment before
  user code runs.
- The payload uses exclusive creation, is never reused, and its native-side
  `TempPath` guard remains outside the Job so failed-start cleanup can retry
  deletion after the contained process tree is terminated.
- Command contents remain excluded from safe trace and receipt metadata.
- No fallback silently changes a requested terminal session into pipe mode or a
  pipe session into terminal mode.

## Testing Strategy

Implementation follows test-driven development in two independently reviewable
cycles.

### Cycle 1: I/O mode split

Write failing tests that prove:

- omitted `tty` reaches the backend as `ShellIoMode::Pipe`;
- explicit `tty: true` reaches it as a 120x30 terminal;
- invalid `tty` types fail with `invalid_arguments` before manager work;
- a real default-mode command reports redirected/non-TTY input;
- a real `tty: true` command reports terminal input;
- the deliberately long working-directory fixture returns one intact logical
  path in default pipe mode;
- long-running pipe and terminal sessions both support poll, write, stop, and
  process-tree cleanup;
- cursor-query startup behavior is terminal-only.

Then implement the minimal mode-aware backend and run focused protocol, tool,
manager, native Shell, CLI, and policy-schema tests.

### Cycle 1 review fix: concurrent ConPTY teardown

Write boundary tests before implementation that prove a drain thread blocked on
an open anonymous pipe is cancelled and joined within the supplied deadline,
and that multi-chunk output plus an exact tail marker is preserved when normal
writer closure produces EOF. Then run real native tests proving one manager can
close eight simultaneous terminal sessions and at least eight independent
managers can do the same without serialization. The full native suite must run
with the test harness's ordinary parallelism.

The boundary tests must first fail because the crate/API does not exist. The
GREEN path must retain the exact final marker, use one shared two-second cleanup
budget, and leave no detached reader thread or raw handle.

### Cycle 2: Windows 32 KiB payload

Write failing Windows tests that prove:

- an exact `MAX_SHELL_COMMAND_BYTES` command launches and emits a final marker;
- `MAX_SHELL_COMMAND_BYTES + 1` still fails with `input_limit`;
- Unicode command payloads decode exactly;
- the dedicated payload-path environment variable is unavailable to user code;
- normal exit, nonzero exit, cancellation, spawn failure, stop, permission
  downgrade, and application shutdown leave no payload file or process-tree
  survivor.

Then implement the guarded payload/bootstrap transport and repeat the focused
test suites.

### Final gates

Run fresh, unfiltered local verification appropriate to the existing project:

- Shell manager and native Windows PTY/pipe suites;
- CLI headless, internal host, restart, and tool-loop integrations;
- all `minimax-tools` targets and features;
- full Rust workspace tests excluding only the two already-authorized hosted-
  fingerprint freshness checks;
- Windows and Linux build/Clippy authority, `cargo fmt --check`, Clippy with
  warnings denied, candidate contracts, provider/retrieval evaluations, and npm
  package tests;
- schema/source-authority fixtures and contract fingerprint updates caused by
  the new `tty` field.

Every regression must first be observed failing for the expected reason. No
completion claim is made from stale results.

## Scope

In scope:

- the `tty` field and schema description;
- pipe/terminal backend separation;
- the isolated Windows ConPTY ownership and deterministic reader-teardown
  boundary authorized during Task 1 review;
- Windows command payload transport;
- exact regression and lifecycle tests;
- affected documentation, contract fixtures, and fingerprints;
- architecture/code review of the resulting diff.

Out of scope:

- browser control, OpenCLI, browser-harness, Pi, or Node runtime integration;
- terminal resizing or full-screen terminal rendering;
- a new shell tool or session action;
- macOS enablement;
- changing permission modes or approval behavior;
- updating hosted evidence, tagging, releasing, merging, or pushing to `main`.
