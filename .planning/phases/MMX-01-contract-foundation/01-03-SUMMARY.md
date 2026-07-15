---
phase: MMX-01-contract-foundation
plan: "03"
subsystem: protocol
tags: [rust, serde, state-machine, provider-normalization, fixtures]
requires:
  - phase: MMX-01-contract-foundation
    provides: pinned workspace and language-neutral compatibility fixtures
provides:
  - Strict schema-versioned provider-neutral stream events and stable typed IDs
  - Exactly-one-terminal sequence validation with deterministic replay ports
  - Responses and Chat Completions fixture normalization without private reasoning
affects: [core, provider, runtime, compat-harness]
tech-stack:
  added: []
  patterns: [strict event envelope, terminal-state reducer, deterministic clock-and-id ports, safe provider projection]
key-files:
  created:
    - crates/protocol/src/event.rs
    - crates/core/src/ports.rs
    - crates/core/src/sequence.rs
    - crates/provider/src/fixture_protocol.rs
  modified:
    - crates/protocol/src/lib.rs
    - crates/core/src/lib.rs
    - crates/provider/src/lib.rs
key-decisions:
  - "The typed Rust variant remains MissingToolCallId while its compatibility wire code stays missing_call_id."
  - "Reasoning input is represented only by the safe ReasoningFiltered category; raw private reasoning is never retained."
patterns-established:
  - "Provider-specific wire formats terminate at the provider boundary and emit only protocol events inward."
  - "A stream reducer accepts one terminal event and rejects duplicate, post-terminal, or premature EOF sequences."
requirements-completed: [ARCH-03, ARCH-04, COMP-02]
coverage:
  - id: D1
    description: "A strict schemaVersion-1 protocol round-trips typed IDs, usage, tool fragments, safe reasoning markers, and terminal outcomes."
    requirement: ARCH-03
    verification:
      - kind: unit
        ref: "cargo test -p minimax-protocol --locked (5/5)"
        status: pass
    human_judgment: false
  - id: D2
    description: "The core reducer enforces exactly one terminal outcome and fixed clock/ID replay produces byte-identical records."
    requirement: ARCH-04
    verification:
      - kind: unit
        ref: "cargo test -p minimax-core --locked (4/4)"
        status: pass
    human_judgment: false
  - id: D3
    description: "Responses and Chat Completions compatibility fixtures normalize to the same safe events and exact error codes."
    requirement: COMP-02
    verification:
      - kind: integration
        ref: "cargo test -p minimax-provider --locked (2/2)"
        status: pass
      - kind: integration
        ref: "npm test (432/432)"
        status: pass
    human_judgment: false
duration: 14min
completed: 2026-07-15
status: complete
---

# Phase 1 Plan 3: Typed Protocol and Provider Normalization Summary

**Both supported Provider stream shapes now cross one strict Rust protocol boundary, obey one terminal-state rule, and replay deterministically without exposing raw reasoning.**

## Performance

- **Duration:** 14 min
- **Started:** 2026-07-15T08:52:00Z
- **Completed:** 2026-07-15T09:05:51Z
- **Tasks:** 3
- **Files modified:** 14

## Accomplishments

- Defined a strict version-1 event envelope with validated session, turn, and tool-call IDs plus stable compatibility error codes.
- Added deterministic clock and ID ports and a stream reducer that rejects missing, duplicate, and post-terminal outcomes.
- Normalized Responses and Chat Completions fixtures into the same provider-neutral event sequence, including stable fragmented tool calls, usage, and safe reasoning filtering.

## Task Commits

1. **Task 1: Define the provider-neutral protocol** - `27ab49c`
2. **Task 2: Enforce deterministic stream terminal state** - `a4dcc80`
3. **Task 3: Normalize Provider compatibility fixtures** - `a3e03fd`

## Files Created/Modified

- `crates/protocol/src/event.rs` - Owns strict versioned events, IDs, usage, terminals, and safe parse errors.
- `crates/protocol/tests/roundtrip.rs` - Proves round trips, validation, safe reasoning, and unknown-event rejection.
- `crates/core/src/ports.rs` - Defines deterministic clock and ID boundaries with fixed test implementations.
- `crates/core/src/sequence.rs` - Enforces one terminal outcome and deterministic replay records.
- `crates/provider/src/fixture_protocol.rs` - Converts both Provider wire fixtures into protocol-neutral compatibility events.
- `crates/provider/tests/provider_fixtures.rs` - Proves valid parity, exact invalid errors, stable tool fragments, and secret-free output.

## Decisions Made

- Preserved the locked fixture spelling `missing_call_id` on the wire while keeping the clearer Rust enum name `MissingToolCallId` and accepting the descriptive alias on input.
- Added a category-only `ReasoningFiltered` event because the compatibility baseline requires an observable safe marker; it never contains Provider reasoning text.
- Kept sequence legality in core and Provider wire interpretation in the adapter so neither concern leaks into the other.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing Critical] Added the safe `ReasoningFiltered` protocol category required by the locked baseline**

- **Found during:** Task 1 protocol design
- **Issue:** The plan's event list omitted the safe reasoning marker already required by Phase 01-01 fixtures.
- **Fix:** Added a category-only event that carries no raw or private reasoning content.
- **Files modified:** `crates/protocol/src/event.rs`, `crates/protocol/tests/roundtrip.rs`
- **Verification:** Protocol and Provider tests assert safe output and reject leaked fixture secrets.
- **Committed in:** `27ab49c`

**2. [Rule 3 - Blocking] Added the locked Serde JSON dependencies needed by the strict protocol and fixture adapters**

- **Found during:** Tasks 1-3 compilation
- **Issue:** The action required JSON parsing and serialization, but the task file lists omitted the affected Cargo manifests and lockfile.
- **Fix:** Reused the exact workspace-owned audited Serde dependencies in protocol, core tests, and provider.
- **Files modified:** `crates/protocol/Cargo.toml`, `crates/core/Cargo.toml`, `crates/provider/Cargo.toml`, `Cargo.lock`
- **Verification:** Workspace Clippy and all locked tests pass with no new package family.
- **Committed in:** `27ab49c`, `a4dcc80`, `a3e03fd`

**Total deviations:** 2 auto-fixed (1 missing compatibility contract, 1 build prerequisite).
**Impact:** Both changes implement locked Phase 1 behavior without widening runtime scope or adding dependencies beyond the existing audited Serde policy.

## Issues Encountered

None beyond the plan/fixture consistency adjustments documented above.

## User Setup Required

None - all verification is credential-free and fixture-only.

## Next Phase Readiness

- Typed streams and exact compatibility evidence are ready for deterministic report generation.
- Core boundaries and Cargo metadata are ready for mechanical architecture enforcement in Plan 01-04.

## Self-Check: PASSED

- All three task commits exist and the claimed protocol/core/provider files are present.
- Protocol 5/5, core 4/4, and provider 2/2 tests pass under the official Rust 1.97.0 gnullvm local toolchain.
- Workspace fmt and Clippy with `-D warnings` pass; all 432 TypeScript tests and `npm run check` remain green.
- No fixture secret, raw reasoning text, database dependency, or TypeScript product-entry change was introduced.

---
*Phase: MMX-01-contract-foundation*
*Completed: 2026-07-15*
