---
phase: MMX-10-rust-authority-and-source-boundaries
plan: "01"
subsystem: architecture-testing
tags: [rust, serde, sha256, source-authority, compatibility]
requires:
  - phase: MMX-09-capability-workspace-and-non-programmer-harness
    provides: Rust-owned product surface and candidate compatibility verification
provides:
  - schema-versioned source and state authority manifest
  - strict offline Rust loader with hash and path validation
  - mandatory source-authority preflight shared by verify and verify-candidate
affects: [phase-11-typescript-retirement, phase-12, phase-13, phase-14-legacy-zero]
tech-stack:
  added: []
  patterns: [deny-by-default source inventory, hash-pinned transitional evidence, shared compatibility preflight]
key-files:
  created:
    - fixtures/compat/source-authority.v1.json
    - crates/compat-harness/src/source_authority.rs
    - crates/compat-harness/tests/source_authority.rs
  modified:
    - crates/compat-harness/src/lib.rs
    - crates/compat-harness/src/main.rs
key-decisions:
  - "Every tracked TS/TSX source is hash-pinned as inert transitional evidence so additions and edits require review before Phase 11 retirement."
  - "The three diagnostic JavaScript fixtures have a separate lifecycle class with Phase 11 disposition and Phase 14 zeroing metadata, never executable authority."
  - "Generated dist contents are excluded from the offline source inventory; committed source and package entry links remain authoritative."
patterns-established:
  - "Authority manifests use strict serde schemas, safe repository-relative paths, duplicate checks, regular-file checks, and SHA-256 drift detection."
  - "Both compatibility modes enter through one verifier and run source authority before loading compatibility evidence."
requirements-completed: [RUST-02]
coverage:
  - id: D1
    description: "A complete, reviewable manifest classifies Rust roots, executable and JavaScript entries, transitional TS and legacy fixtures, immutable fixtures, targets, and state roots."
    requirement: RUST-02
    verification:
      - kind: unit
        ref: "crates/compat-harness/tests/source_authority.rs#manifest_schema"
        status: pass
    human_judgment: false
  - id: D2
    description: "A deterministic Rust validator rejects unclassified sources, unsafe paths, hash drift, JavaScript product behavior, fixture smuggling, and extra writable roots."
    requirement: RUST-02
    verification:
      - kind: integration
        ref: "cargo test -p minimax-compat-harness --test source_authority --locked"
        status: pass
    human_judgment: false
  - id: D3
    description: "verify and verify-candidate share the mandatory source-authority preflight while the Rust CLI and npm launcher remain present."
    requirement: RUST-02
    verification:
      - kind: integration
        ref: "cargo run -p minimax-compat-harness --locked -- verify-candidate"
        status: pass
      - kind: other
        ref: "cargo build -p minimax-cli --locked"
        status: pass
    human_judgment: false
duration: 29min
completed: 2026-07-17
status: complete
---

# Phase 10 Plan 01: Source Authority Contract Summary

**A hash-pinned source ownership manifest and mandatory Rust preflight now make JavaScript, transitional TypeScript, fixture, executable, and state authority fail closed.**

## Performance

- **Duration:** 29 min
- **Started:** 2026-07-17T11:32:50Z
- **Completed:** 2026-07-17T12:01:02Z
- **Tasks:** 2
- **Files modified:** 5

## Accomplishments

- Classified all 191 tracked TS/TSX files, the five permitted JavaScript orchestration files, the three separately governed diagnostic fixtures, Rust product roots, immutable fixture roots, supported targets, and state roots.
- Added strict parsing and deterministic repository validation for unsafe paths, duplicates, symlinks, hash drift, unclassified sources, package-entry mismatches, JavaScript product imports, interpreter fallback, runtime download, and product-domain implementation.
- Wired the gate ahead of compatibility manifest loading in the verifier shared by `verify` and `verify-candidate`; all six focused tests, strict Clippy, candidate verification, formatting, and the Rust CLI build pass.

## Task Commits

Each task was committed atomically with its TDD evidence:

1. **Task 1 RED: failing source authority contract** - `6209285` (test)
2. **Task 1 GREEN: complete authority manifest and strict loader** - `8e7771b` (feat)
3. **Task 2 RED: failing source authority gate cases** - `18036e5` (test)
4. **Task 2 GREEN: mandatory validator and shared preflight** - `59d4b24` (feat)

The summary and planning trackers are committed together in the final plan metadata commit.

## Files Created/Modified

- `fixtures/compat/source-authority.v1.json` - Complete source, executable, fixture, target, and state authority inventory.
- `crates/compat-harness/src/source_authority.rs` - Strict manifest loader and deterministic offline validator.
- `crates/compat-harness/tests/source_authority.rs` - Positive schema/inventory and table-driven negative boundary tests.
- `crates/compat-harness/src/lib.rs` - Public source-authority exports.
- `crates/compat-harness/src/main.rs` - Shared compatibility preflight wiring.

## Decisions Made

- Kept the full TypeScript tree as hash-pinned evidence; no transitional source was deleted or renamed in Phase 10.
- Kept diagnostic JavaScript fixtures outside executable authority with explicit Phase 11 disposition and Phase 14 deletion/zero contracts.
- Limited JavaScript authority to the fixed npm launcher and four release/package orchestration scripts; product-domain implementation remains Rust-owned.
- Validated the supported `minimax-codex` package executable link now; the legacy package-command cutover remains assigned to Plan 10-02.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Narrowed fallback detection to actual process invocation arguments**

- **Found during:** Task 2 positive repository validation
- **Issue:** The launcher contains an actionable error message mentioning `minimax-codex-legacy`; scanning the whole file around a process call falsely classified that text as an executed fallback.
- **Fix:** Restricted fallback matching to each process-invocation statement while retaining rejection of real legacy interpreter or script execution.
- **Files modified:** `crates/compat-harness/src/source_authority.rs`
- **Verification:** The real repository passes while the synthetic `spawnSync("node", ["dist/cli.js"])` case remains rejected.
- **Committed in:** `59d4b24`

**2. [Rule 3 - Blocking Environment] Used the installed GNU-LLVM Rust toolchain for local evidence**

- **Found during:** Task verification
- **Issue:** The local MSVC linker is unavailable.
- **Fix:** Ran local tests, Clippy, candidate verification, and builds with the installed `1.97.0-x86_64-pc-windows-gnullvm` toolchain and bundled `rust-lld`.
- **Files modified:** None
- **Verification:** Every plan command and strict Clippy completed successfully.
- **Committed in:** Not applicable; this is local development evidence only.

---

**Total deviations:** 2 auto-fixed (1 correctness bug, 1 blocking environment workaround)
**Impact on plan:** The source-boundary contract and scope are unchanged; the workaround is not hosted Windows MSVC evidence.

## Issues Encountered

- The workstation lacks the MSVC linker. GNU-LLVM plus bundled `rust-lld` provided complete local development verification without modifying product or release configuration.

## User Setup Required

None - the validator is offline and requires no external services or credentials.

## Next Phase Readiness

- Plan 10-02 can consume the exact executable/package and JavaScript authority contract for the legacy command cutover.
- Phase 11 can disposition each hash-pinned transitional responsibility and diagnostic fixture without rediscovering repository ownership.
- No implementation blocker remains; hosted MSVC evidence stays governed by the existing release workflow.

## Self-Check: PASSED

- All five planned artifact paths exist.
- All four TDD task commits exist in git history.
- The final focused suite reports all 6 tests passed; candidate verification, strict Clippy, formatting, and CLI build also pass.
- `git diff --name-status c40d56f..HEAD` contains no deletions.

---
*Phase: MMX-10-rust-authority-and-source-boundaries*
*Completed: 2026-07-17*
