---
phase: MMX-02-usable-rust-agent-shell
plan: "02"
subsystem: sessions
tags: [rust, jsonl, fs4, recovery, compaction, trace, obsidian-vault]
requires:
  - phase: MMX-02-usable-rust-agent-shell
    provides: provider-neutral streaming runtime and strict terminal receipts
provides:
  - strict replayable session and turn journal records
  - single-writer project-local runtime store with final-fragment-only repair
  - deterministic local compaction with no Provider path
  - bounded allowlisted folded trace
affects: [cli-driver, headless, tui, doctor, vault-wiki]
tech-stack:
  added: [fs4-1.1.0, tempfile-3.27.0]
  patterns: [append-sync JSONL, content-addressed derived index, immutable retry, typed safe trace]
key-files:
  created:
    - crates/protocol/src/session.rs
    - crates/core/src/session.rs
    - crates/core/src/compaction.rs
    - crates/core/src/trace.rs
    - crates/vault/src/runtime/journal.rs
    - crates/vault/src/runtime/lease.rs
    - crates/vault/src/runtime/recovery.rs
  modified:
    - Cargo.toml
    - Cargo.lock
    - crates/compat-harness/src/architecture.rs
key-decisions:
  - "The Vault crate is the sole concrete project-local runtime writer; core remains filesystem- and lock-free."
  - "Derived indexes are content-addressed and atomically published under a new name, so Windows never needs to overwrite a live index file."
  - "Only a UTF-8 final fragment without a newline is quarantined and trimmed; complete-line or middle corruption never changes the journal."
patterns-established:
  - "Retry and recovery append new immutable facts; terminal history is never rewritten."
  - "Compaction retains whole completed exchanges and fails when the required whole entries cannot fit."
requirements-completed: [RUN-02, RUN-03, RUN-05]
requirements-progressed: [RUN-04]
coverage:
  - id: D1
    description: Session commands reconstruct deterministically and recovery appends one interruption without reusing partial assistant text as context.
    requirement: RUN-02
    verification:
      - kind: unit
        ref: "crates/core/tests/session_machine.rs"
        status: pass
      - kind: integration
        ref: "crates/vault/tests/runtime_store.rs#abandoned_turn_is_interrupted_exactly_once_across_restarts"
        status: pass
    human_judgment: false
  - id: D2
    description: One writer owns bounded append-synced JSONL; only a final fragment is repaired and index conflicts fail without journal mutation.
    requirement: RUN-04
    verification:
      - kind: integration
        ref: "crates/vault/tests/runtime_store.rs"
        status: pass
    human_judgment: false
  - id: D3
    description: Completed visible exchanges compact into byte-stable categories and retained recent turns with no Provider execution path.
    requirement: RUN-03
    verification:
      - kind: unit
        ref: "crates/core/tests/compaction_trace.rs"
        status: pass
    human_judgment: false
  - id: D4
    description: Trace keeps only bounded allowlisted facts and removes adversarial credential, reasoning, raw-frame, and tool-body markers before persistence.
    requirement: RUN-05
    verification:
      - kind: unit
        ref: "crates/core/tests/compaction_trace.rs#trace_keeps_only_bounded_allowlisted_safe_facts_and_folds_deterministically"
        status: pass
      - kind: integration
        ref: "crates/vault/tests/runtime_store.rs#safe_trace_protocol_record_never_persists_adversarial_input"
        status: pass
    human_judgment: false
duration: 27min
completed: 2026-07-15
status: complete
---

# Phase 2 Plan 2: Durable Sessions, Recovery, Compaction, and Trace Summary

**Rust conversations now survive restart through a leased append-only local journal, while compaction and trace remain deterministic, bounded, and secret-safe.**

## Performance

- **Duration:** 27 min
- **Started:** 2026-07-15T10:47:00Z
- **Completed:** 2026-07-15T11:14:00Z
- **Tasks:** 3
- **Files modified:** 22

## Accomplishments

- Added strict schema-v1 session, turn, recovery, compaction, and trace records plus a pure replayable session state machine.
- Added one project-local `RuntimeStore` with a non-blocking OS lease, one-MiB JSONL records, sync-before-acknowledgement, final-fragment evidence, and rebuildable content-addressed indexes.
- Recovered abandoned running turns as exactly one durable interruption and kept interrupted/failed partial assistant text out of later model context.
- Added a local structured compactor and folded trace recorder that exclude credentials, raw reasoning, raw Provider frames, tool bodies, and unknown facts.

## Task Commits

1. **Task 1: Define durable session records and the pure session machine** - `f930e13`
2. **Task 2: Implement the leased project-local runtime journal and recovery** - `1f43cfc`
3. **Task 3: Add deterministic compaction, safe trace, and failure-boundary gates** - `3db43bd`

## Files Created/Modified

- `crates/protocol/src/session.rs` - strict durable session, compaction, recovery, and trace contracts.
- `crates/core/src/session.rs` - immutable terminal history, linked retry, replay, and recovery policy.
- `crates/core/src/compaction.rs` - completed-visible-only local structured compactor.
- `crates/core/src/trace.rs` - bounded allowlisted safe trace and deterministic folding.
- `crates/vault/src/runtime/journal.rs` - append/sync JSONL plus fail-closed validation and final-fragment repair.
- `crates/vault/src/runtime/index.rs` - bounded content-addressed derived session index.
- `crates/vault/src/runtime/lease.rs` - non-blocking cross-platform single-writer OS lock.
- `crates/vault/src/runtime/recovery.rs` - one-time abandoned-turn reconciliation.

## Decisions Made

- Kept runtime facts inside the per-project `.minimax/runtime/v1` boundary; no SQLite or second storage authority was introduced.
- Published derived indexes under journal-fingerprint filenames, preserving atomic Windows/Linux publication without deleting old evidence.
- Left RUN-04 pending only for the Plan 02-03 composition driver's controlled-shutdown proof; its lease, startup recovery, and corruption boundaries are complete.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Opened the journal with explicit read/write access instead of append-only access**

- **Found during:** Task 2 final-fragment fault test
- **Issue:** The local Windows target could append through the handle but could not truncate a damaged tail through that append-only handle.
- **Fix:** Open without truncation using explicit read/write access and seek to end before each append.
- **Files modified:** `crates/vault/src/runtime/journal.rs`
- **Verification:** Final-fragment quarantine/trim and subsequent append/reopen tests pass on Windows.
- **Committed in:** `1f43cfc`

**2. [Rule 2 - Missing Critical] Added same-boundary index conflict detection**

- **Found during:** Task 2 acceptance review
- **Issue:** A stale index can be rebuilt, but an index claiming the current journal boundary with a different fingerprint must not be treated as merely stale.
- **Fix:** Compare schema, byte length, record count, fingerprint, and session projection; fail without touching the journal on conflict.
- **Files modified:** `crates/vault/src/runtime/index.rs`, `crates/vault/tests/runtime_store.rs`
- **Verification:** The injected conflict test returns `IndexConflict` and confirms byte-identical journal contents.
- **Committed in:** `1f43cfc`

**Total deviations:** 2 auto-fixed (1 bug, 1 missing critical guard). **Impact:** No scope expansion; both changes implement the planned failure boundaries.

## Issues Encountered

- Local Rust verification still uses the pinned 1.97.0 windows-gnullvm toolchain because this host cannot install the current MSVC Build Tools.

## User Setup Required

None - all tests are local and use no credential, Provider request, model download, database, or destructive migration.

## Next Phase Readiness

- Durable sessions, compaction, recovery, and trace are ready for the shared CLI composition driver.
- Plan 02-03 must prove controlled shutdown across active run cancellation, terminal persistence, and lease release before RUN-04 is marked complete.

## Self-Check: PASSED

- Rust workspace: 48/48 tests passed; formatting and workspace Clippy with `-D warnings` passed.
- TypeScript baseline: 432/432 tests passed; type checking and Rust compatibility verification passed.
- Final-fragment repair, middle corruption, invalid UTF-8, oversized records, stale/missing/conflicting indexes, repeated recovery, and two-writer exclusion all have offline tests.
- No real Provider, credential, embedding model, SQLite, deletion, migration, or npm-entry change was used.

---
*Phase: MMX-02-usable-rust-agent-shell*
*Completed: 2026-07-15*
