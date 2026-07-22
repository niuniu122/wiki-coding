# Codex-Aligned Shell I/O Boundary Repair Design

**Date:** 2026-07-22

**Status:** Approved direction; written specification awaiting user review

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

For terminal mode, the current PTY/ConPTY launch path remains in use. The only
behavioral change there is the explicit mode selection.

## Windows Command Payload

The authenticated parent-to-host protocol remains the authority for accepting
the command. After receiving a validated command, the Windows host:

1. creates an exclusive, randomly named temporary payload with a `.ps1` suffix;
2. writes the command as UTF-8 and flushes it before child launch;
3. closes the file handle while retaining a `TempPath` cleanup owner;
4. launches PowerShell with `-NoLogo -NoProfile -Command` and a short constant
   bootstrap that reads the payload with explicit UTF-8 decoding and evaluates
   it in the command scope;
5. supplies only the random payload path through a dedicated child environment
   variable, removes that variable before evaluating user code, and removes all
   existing internal-host bootstrap variables as today;
6. retains the cleanup owner until PowerShell has exited or cleanup terminates
   the process tree, then deletes the payload.

The user command is therefore absent from the Windows process command line, and
the 32 KiB public boundary no longer competes with executable/flag/quoting
overhead. Standard input remains available for `shell_session` writes because
the bootstrap reads the command from the payload rather than stdin.

Payload creation, writing, decoding, or PowerShell spawn failure occurs before
the host sends `Ready`. The parent reports `shell_launch_failed`, runs unified
unpublished-session cleanup, publishes no session ID, and releases the running
slot. Cleanup also removes the payload on every error path.

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
- Internal host address, token, protocol version, and timeout variables are not
  inherited by PowerShell.
- The temporary payload path is removed from PowerShell's environment before
  user code runs.
- The payload uses exclusive creation, is never reused, and is owned by a guard
  whose drop path deletes it.
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
