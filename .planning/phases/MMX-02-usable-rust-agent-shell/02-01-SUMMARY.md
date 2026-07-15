---
phase: MMX-02-usable-rust-agent-shell
plan: "01"
subsystem: runtime
tags: [rust, reqwest, rustls, sse, tokio, state-machine, provider]
requires:
  - phase: MMX-01-contract-foundation
    provides: strict stream protocol, terminal reducer, fixtures, and architecture gates
provides:
  - strict provider-neutral conversation runtime records
  - bounded direct Responses and Chat Completions HTTP/SSE adapters
  - synchronous RunMachine with persist-before-publish effects
  - recursive core architecture enforcement
affects: [sessions, vault-runtime, cli-driver, tui, diagnostics]
tech-stack:
  added: [reqwest-0.13.4, tokio-1.52.3, tokio-util-0.7.18, futures-util-0.3.32, secrecy-0.10.3]
  patterns: [pure effect reducer, bounded SSE decoding, redacted typed failure, loopback-only network tests]
key-files:
  created:
    - crates/protocol/src/runtime.rs
    - crates/provider/src/client.rs
    - crates/provider/src/sse.rs
    - crates/core/src/runtime.rs
  modified:
    - Cargo.toml
    - Cargo.lock
    - crates/compat-harness/src/architecture.rs
key-decisions:
  - "Core remains a synchronous reducer; Tokio and Reqwest stay in Provider/CLI adapters."
  - "Supported Windows/MSVC and Linux builds enable rustls; the unsupported local windows-gnullvm fallback compiles loopback HTTP tests without TLS because it lacks Windows SDK import libraries."
  - "Tool calls are normalized and observed but terminate with tool_unavailable; no Phase 2 tool side effect exists."
patterns-established:
  - "Every observable event is emitted as Persist then Publish."
  - "Network, protocol, cancellation, timeout, and status failures expose fixed codes only."
requirements-completed: [RUN-01]
coverage:
  - id: D1
    description: Both Provider wire protocols stream through one strict redacted runtime contract.
    requirement: RUN-01
    verification:
      - kind: integration
        ref: "crates/provider/tests/http_stream.rs#responses_and_chat_completions_converge_on_safe_stream_events"
        status: pass
      - kind: unit
        ref: "crates/protocol/tests/runtime_roundtrip.rs"
        status: pass
    human_judgment: false
  - id: D2
    description: Cancellation, deadline, malformed stream, premature EOF, duplicate terminal, and data-after-terminal remain distinct truthful outcomes.
    requirement: RUN-01
    verification:
      - kind: integration
        ref: "crates/provider/tests/http_stream.rs"
        status: pass
      - kind: unit
        ref: "crates/core/tests/runtime_machine.rs"
        status: pass
    human_judgment: false
  - id: D3
    description: Core remains free of async runtime, HTTP, filesystem, terminal, keyring, and database adapters, including nested modules.
    requirement: RUN-01
    verification:
      - kind: integration
        ref: "cargo test -p minimax-compat-harness --locked architecture"
        status: pass
      - kind: other
        ref: "cargo clippy --workspace --all-targets --locked -- -D warnings"
        status: pass
    human_judgment: false
duration: 26min
completed: 2026-07-15
status: complete
---

# Phase 2 Plan 1: Provider Streaming Runtime Summary

**Direct Responses/Chat Completions streaming now feeds a strict redacted Rust protocol and a pure persist-before-publish run machine.**

## Performance

- **Duration:** 26 min
- **Started:** 2026-07-15T10:19:48Z
- **Completed:** 2026-07-15T10:45:31Z
- **Tasks:** 3
- **Files modified:** 18

## Accomplishments

- Added schema-v1 turn requests, runtime events, safe failure codes, terminal outcomes, and receipts with strict Serde validation.
- Added bounded chunk-safe SSE parsing and real Reqwest adapters for both Provider protocols with separate cancellation/deadline/status/protocol outcomes.
- Added a synchronous core `RunMachine` that persists before publishing, finalizes once, and never executes observed tool calls.
- Strengthened the architecture gate to scan nested core modules and reject async/network/storage/UI/keyring/database imports.

## Task Commits

1. **Task 1: Define the strict conversation runtime protocol** - `4c84c90`
2. **Task 2: Implement bounded direct Provider HTTP and SSE adapters** - `7a280ee`
3. **Task 3: Build the pure run machine and strengthen architecture gates** - `e445959`

## Files Created/Modified

- `crates/protocol/src/runtime.rs` - provider-neutral requests, events, failures, terminal receipts, and parsing.
- `crates/provider/src/client.rs` - cancellation/deadline-aware direct streaming client.
- `crates/provider/src/sse.rs` - bounded LF/CRLF decoder safe across chunk splits.
- `crates/provider/src/responses.rs` - Responses request construction and frame normalization.
- `crates/provider/src/chat_completions.rs` - Chat Completions request construction and frame normalization.
- `crates/core/src/runtime.rs` - synchronous run reducer and ordered effects.
- `crates/compat-harness/src/architecture.rs` - recursive core and database boundary enforcement.

## Decisions Made

- Kept the policy engine synchronous and adapter-free; async orchestration remains outside core.
- Used direct Reqwest/rustls instead of a Provider SDK or a second agent framework.
- Stored no raw response body, raw frame, credential, or reasoning content in errors or runtime events.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Preserved local windows-gnullvm verification without weakening supported release TLS**

- **Found during:** Task 2
- **Issue:** Rustls' Windows build dependencies require SDK import libraries absent from this unsupported Windows 10 gnullvm fallback.
- **Fix:** Enable Reqwest rustls on supported Windows/MSVC and Linux targets; use the gnullvm target only for loopback HTTP verification.
- **Files modified:** `Cargo.toml`, `crates/provider/Cargo.toml`
- **Verification:** Full local Clippy/tests pass; supported rustls targets remain represented in Cargo resolution and require hosted CI confirmation.
- **Committed in:** `7a280ee`

**2. [Rule 1 - Bug] Removed a false database-denylist match**

- **Found during:** Task 3
- **Issue:** The old substring `orm` rule incorrectly classified Reqwest's `form_urlencoded` dependency as a database ORM.
- **Fix:** Match concrete SQLite, SQLx, Diesel, and SeaORM package families.
- **Files modified:** `crates/compat-harness/src/architecture.rs`
- **Verification:** Real metadata and synthetic database rejection tests both pass.
- **Committed in:** `e445959`

**Total deviations:** 2 auto-fixed (1 blocker, 1 bug). **Impact:** No product scope change; the supported release path still requires rustls and default tests remain offline.

## Issues Encountered

- The local Rust toolchain has no global Cargo command on PATH; verification uses the installed pinned 1.97.0 gnullvm executables directly.

## User Setup Required

None - no real credential or external Provider request is required for this plan.

## Next Phase Readiness

- Runtime events and effects are ready for the durable session journal in Plan 02-02.
- Supported Windows/MSVC and Linux rustls compilation will be reconfirmed in hosted CI before Phase 2 is closed.

## Self-Check: PASSED

- Rust workspace: 40/40 tests passed; formatting and Clippy with `-D warnings` passed.
- TypeScript baseline: 432/432 tests passed; type checking and Rust compatibility verification passed.
- No real Provider, credential, embedding model, SQLite, migration, or npm-entry change was used.

---
*Phase: MMX-02-usable-rust-agent-shell*
*Completed: 2026-07-15*
