---
phase: MMX-02-usable-rust-agent-shell
plan: "03"
subsystem: cli
tags: [rust, clap, crossterm, jsonl, keyring, doctor, controlled-shutdown]
requires:
  - phase: MMX-02-usable-rust-agent-shell
    provides: provider-neutral runtime, durable sessions, lease, recovery, compaction, and safe trace
provides:
  - shared interactive and headless Rust conversation driver
  - strict layered config and environment/keyring credential resolution
  - complete compatibility command parser and typed later-phase routing
  - stable schema-v1 JSONL and exit classes 0, 2, 3, 4, and 5
  - actionable redacted doctor report and restart/shutdown proof
affects: [safe-tools, vault-wiki, retrieval, migration, release]
tech-stack:
  added: [clap-4.6.1, crossterm-0.29.0, keyring-3.6.3, secrecy-0.10.3]
  patterns: [persist-before-publish, injectable-provider-port, streaming-shared-projection, typed-maintenance-route]
key-files:
  created:
    - crates/cli/src/app.rs
    - crates/cli/src/driver.rs
    - crates/cli/src/headless.rs
    - crates/cli/src/doctor.rs
    - crates/cli/tests/headless.rs
    - crates/cli/tests/restart.rs
    - crates/compat-harness/src/baseline.rs
  modified:
    - crates/cli/src/main.rs
    - crates/tui/src/shell.rs
    - crates/vault/src/runtime/mod.rs
    - fixtures/compat/baseline-status.v1.json
key-decisions:
  - "The driver persists every delta or terminal fact before publishing the same schema-v1 event to either JSONL or TUI."
  - "Headless credential resolution is environment-only; interactive mode may consult the OS keyring after environment lookup."
  - "Later-phase CLI routes parse now but return explicit owning-phase unavailability without side effects."
patterns-established:
  - "One injectable ProviderPort drives both real HTTP streaming and deterministic no-network tests."
  - "Doctor serializes only public config/credential sources and fixed recovery details, never values or raw errors."
requirements-completed: [RUN-04, CLI-01, CLI-02, CLI-03, CLI-04]
coverage:
  - id: D1
    description: Mock Provider deltas, usage, and terminal events are persisted before a byte-stable shared JSONL/TUI projection.
    requirement: CLI-02
    verification:
      - kind: integration
        ref: "crates/cli/tests/headless.rs#mock_run_projects_byte_stable_schema_v1_jsonl_without_terminal_hooks"
        status: pass
    human_judgment: false
  - id: D2
    description: Process reconstruction supports list, resume, continue, linked retry, local compaction, and one-writer exclusion.
    requirement: RUN-04
    verification:
      - kind: integration
        ref: "crates/cli/tests/restart.rs#conversation_reconstructs_then_lists_resumes_continues_retries_and_compacts"
        status: pass
    human_judgment: false
  - id: D3
    description: Controlled cancellation persists one interrupted receipt and releases the project lease for the next process.
    requirement: RUN-04
    verification:
      - kind: integration
        ref: "crates/cli/tests/restart.rs#controlled_cancellation_persists_once_and_releases_lease"
        status: pass
    human_judgment: false
  - id: D4
    description: All compatibility commands, aliases, two permission names, maintenance routes, and the unchanged npm entry have executable evidence.
    requirement: CLI-01
    verification:
      - kind: contract
        ref: "crates/compat-harness/src/baseline.rs"
        status: pass
      - kind: hosted-ci
        ref: "https://github.com/niuniu122/minimax-codex/actions/runs/29413637887"
        status: pass
    human_judgment: false
duration: 44min
completed: 2026-07-15
status: complete
---

# Phase 2 Plan 3: Compatible Rust CLI, Diagnostics, and Restart Proof Summary

**The Rust development binary is now a usable recoverable conversation shell with shared streaming TUI/JSONL events, safe configuration, controlled shutdown, and executable compatibility evidence.**

## Performance

- **Duration:** 44 min
- **Started:** 2026-07-15T11:16:00Z
- **Completed:** 2026-07-15T12:00:00Z
- **Tasks:** 3
- **Files modified:** 21

## Accomplishments

- Added strict configuration precedence from defaults through user, project, environment, and CLI, plus environment-first credentials with interactive-only OS keyring fallback.
- Added every compatibility slash command, terminal-safe rendering, raw-mode restoration, line fallback, and typed later-phase unavailability.
- Composed Provider, core state machines, leased local journal, TUI, headless JSONL, Clap routes, doctor, cancellation, and stable exit classes in one Rust development binary.
- Proved reconstruction, resume, continue, retry, compaction, two-writer exclusion, cancellation, exactly-one terminal persistence, and lease release with deterministic mock Providers.
- Promoted only command, permission-name, and product-entry compatibility rows backed by executable Rust evidence; provider-profile rows remain pending.

## Task Commits

1. **Task 1: Implement strict configuration and redacted credential resolution** - `c2baac4`
2. **Task 2: Implement the command inventory and terminal-only presentation** - `00003e8`
3. **Task 3: Compose the interactive/headless CLI, diagnostics, and restart proof** - `43900a3`

## Files Created/Modified

- `crates/cli/src/app.rs` - stable Clap routes for run, chat, doctor, migrate, vault, and index.
- `crates/cli/src/driver.rs` - shared async runtime composition with injected Provider stream and persist-before-publish ordering.
- `crates/cli/src/headless.rs` - schema-v1 JSONL writer and exact 0/2/3/4/5 exit mapping.
- `crates/cli/src/doctor.rs` - source-only, fixed-detail diagnostics for config, credentials, lease, journal, index, and terminal capability.
- `crates/tui/src/command.rs` - full compatibility parser with only confirm and full-access permission names.
- `crates/tui/src/shell.rs` - guarded raw terminal input plus safe line fallback.
- `crates/compat-harness/src/baseline.rs` - executable command, permission, and npm-entry baseline validation.

## Decisions Made

- Kept `package.json` on `dist/cli.js`; the Rust binary remains a development path until Phase 6 cutover gates pass.
- Kept TUI presentation replaceable: it depends on protocol events and typed intents, not Provider, Vault, or runtime policy.
- Kept credentials transient: no serializable secret type, plaintext fallback, diagnostic value, or default test credential exists.
- Kept Linux keyring support on the lighter native backend and avoided SQLite/database dependencies.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing Critical] Added a durable SessionCommand adapter at the Vault boundary**

- **Found during:** Task 3 driver composition
- **Issue:** The pure session state machine produced persistence effects, but the concrete runtime store lacked one operation that durably applied those effects before external publication.
- **Fix:** Added `RuntimeStore::apply_command`, which previews the pure transition and appends each persistence effect through the existing journal/index path before returning observable effects.
- **Files modified:** `crates/vault/src/runtime/mod.rs`
- **Verification:** Headless and restart integration tests prove persisted-before-published deltas and terminal reconstruction.
- **Committed in:** `43900a3`

**2. [Rule 1 - Bug] Added raw-mode key input and unsupported-target line fallback**

- **Found during:** Task 3 interactive composition review
- **Issue:** Standard buffered `read_line` is not reliable after enabling terminal raw mode, and the local gnullvm fallback cannot link Windows Crossterm SDK imports.
- **Fix:** Read key events through Crossterm on supported targets, restore raw mode with RAII, and fall back to line input when raw mode reports unsupported. Crossterm remains fully enabled on supported Windows/MSVC and Linux targets.
- **Files modified:** `crates/tui/src/shell.rs`, `crates/cli/src/main.rs`, `crates/tui/tests/command_render.rs`
- **Verification:** Local guard/fallback tests pass; hosted Windows/MSVC and Ubuntu CI both compile and pass the complete workflow.
- **Committed in:** `43900a3`

**Total deviations:** 2 auto-fixed (1 missing boundary, 1 interactive bug). **Impact:** Both fixes were required to satisfy the locked persistence and terminal behavior; no product cutover or later-phase implementation was pulled forward.

## Issues Encountered

- This host still lacks current MSVC Build Tools, so local verification used the pinned Rust 1.97.0 windows-gnullvm toolchain. Hosted CI run `29413637887` passed on both `windows-latest` MSVC and `ubuntu-latest`.

## User Setup Required

None. Default tests use deterministic mocks and do not open a remote socket, consume a credential, spend Provider quota, download an embedding model, migrate data, or alter the npm product entry.

## Next Phase Readiness

- Phase 2 is complete: the Rust shell can run, stream, recover, compact, diagnose, and shut down safely.
- Phase 3 can attach stable tool-call identity, approval policy, and bounded adapters to the existing `ProviderPort`/driver boundary without changing storage or TUI ownership.

## Self-Check: PASSED

- Rust workspace: 79/79 tests passed; formatting and workspace Clippy with `-D warnings` passed.
- TypeScript reference: 432/432 tests passed; type checking, build, retrieval/provider evaluations, and compatibility verification passed.
- Hosted CI: Windows/MSVC and Ubuntu jobs passed every offline gate in [run 29413637887](https://github.com/niuniu122/minimax-codex/actions/runs/29413637887).
- `package.json` still maps `minimax-codex` to `dist/cli.js`; provider-profile compatibility remains truthfully pending.

---
*Phase: MMX-02-usable-rust-agent-shell*
*Completed: 2026-07-15*
