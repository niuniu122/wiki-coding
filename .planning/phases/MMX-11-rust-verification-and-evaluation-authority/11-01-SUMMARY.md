---
phase: MMX-11-rust-verification-and-evaluation-authority
plan: "01"
subsystem: verification-authority
tags: [rust, serde, coverage-matrix, cli, lifecycle, tools, rendering]
requires:
  - phase: MMX-10-rust-authority-and-source-boundaries
    provides: Hashed transitional source inventory and Rust-owned verification preflights
provides:
  - exact historical TypeScript verification responsibility matrix with reviewed dispositions
  - fail-closed Rust validation of source hashes, evidence, contracts, and retirement rationale
  - deterministic Rust evidence for distinct CLI, lifecycle, permission, and command outcomes
affects: [phase-11-evaluators, phase-11-deletion, phase-14-hosted-closure]
tech-stack:
  added: []
  patterns: [strict serde authority manifests, exact Rust test evidence, explicit non-public retirement]
key-files:
  created:
    - fixtures/compat/verification/typescript-responsibilities.v1.json
    - crates/compat-harness/src/coverage.rs
    - crates/compat-harness/tests/coverage.rs
  modified:
    - crates/compat-harness/src/lib.rs
    - crates/compat-harness/src/main.rs
    - crates/cli/tests/headless.rs
    - crates/cli/tests/lifecycle_wiki.rs
    - crates/cli/tests/tool_loop.rs
    - crates/tui/tests/command_render.rs
key-decisions:
  - "The matrix exactly freezes all 97 Phase 10 test, evaluation, smoke, and legacy diagnostic paths by path plus SHA-256 while allowing multiple distinct responsibilities per source."
  - "Only documented public or safety behavior is Rust-covered; dormant, internal, or unshipped TypeScript behavior is explicitly retired rather than ported for filename parity."
  - "The matrix records evidence but is not runtime authority: every Rust-covered row names an existing Rust file and exact test function that the validator checks."
patterns-established:
  - "Both compatibility verification modes validate historical responsibility coverage before loading compatibility manifests or making a candidate decision."
  - "Public behavior suites assert production outcomes first and then bind the exact passing test to a distinct historical responsibility ID."
requirements-completed: [RVE-01]
coverage:
  - id: D1
    description: "Every transitional TypeScript test, evaluator, smoke, and diagnostic responsibility has an exact source hash and reviewable Rust, package-smoke, or retirement disposition."
    requirement: RVE-01
    verification:
      - kind: integration
        ref: "crates/compat-harness/tests/coverage.rs#coverage_matrix_exists_and_matches_the_phase_ten_verification_inventory"
        status: pass
      - kind: integration
        ref: "crates/compat-harness/tests/coverage.rs#repository_matrix_validates_with_no_unresolved_responsibility"
        status: pass
    human_judgment: false
  - id: D2
    description: "Distinct public terminal output, lifecycle finalization, permission reset, and retry/continue outcomes are proved by deterministic Rust tests and block candidate verification through the matrix gate."
    requirement: RVE-01
    verification:
      - kind: integration
        ref: "cargo test -p minimax-cli --test headless --test lifecycle_wiki --test tool_loop --locked"
        status: pass
      - kind: integration
        ref: "cargo test -p minimax-tui --test command_render --locked"
        status: pass
      - kind: integration
        ref: "cargo run -p minimax-compat-harness --locked -- verify-candidate"
        status: pass
    human_judgment: false
duration: 30min
completed: 2026-07-17
status: complete
---

# Phase 11 Plan 01: Coverage Disposition and Behavioral Gaps Summary

**All 97 transitional verification sources now have exact historical dispositions, strict Rust validation, and deterministic Rust evidence for the previously collapsed public CLI, lifecycle, permission, and command outcomes.**

## Performance

- **Duration:** 30 min
- **Started:** 2026-07-17T14:54:00Z
- **Completed:** 2026-07-17T15:24:00Z
- **Tasks:** 2
- **Files created/modified:** 9

## Accomplishments

- Froze the Phase 10 verification inventory as 97 exact source path/hash entries and 101 responsibility rows: 71 Rust-covered, 4 package-smoke, and 26 explicitly retired.
- Added a strict serde validator that rejects schema drift, missing or changed sources, duplicate responsibility IDs, unresolved dispositions, TypeScript evidence, invalid retirement of public contracts, absent evidence, and mismatched Rust test names.
- Wired both `verify` and `verify-candidate` through coverage validation before compatibility manifests and candidate decisions.
- Added exact, offline Rust assertions for terminal text/JSONL and exit parity, finalize-before-restart semantics, process-scoped permission reset, and distinct retry/continue command outcomes.

## Task Commits

Each task was committed atomically in TDD order:

1. **Task 1 RED: add failing coverage authority tests** - `917133c` (test)
2. **Task 1 GREEN: freeze and validate every TypeScript responsibility** - `c05815b` (feat)
3. **Task 2 RED: expose distinct public behavior evidence gaps** - `c9ce246` (test)
4. **Task 2 GREEN: bind exact passing Rust behavior evidence** - `67ee848` (feat)

The summary and planning trackers are committed together in the final plan metadata commit.

## Files Created/Modified

- `fixtures/compat/verification/typescript-responsibilities.v1.json` - Exact historical source hashes, responsibility categories, dispositions, evidence, contract references, and rationales.
- `crates/compat-harness/src/coverage.rs` - Strict matrix loading and fail-closed validation.
- `crates/compat-harness/src/lib.rs` - Public coverage loader, validator, schema, and error exports.
- `crates/compat-harness/src/main.rs` - Coverage gate ordering for both repository verification modes.
- `crates/compat-harness/tests/coverage.rs` - Real-repository completeness plus mutation tests for every invalid authority shape.
- `crates/cli/tests/headless.rs` - Exact terminal text/JSONL, sanitization, and public exit-code parity.
- `crates/cli/tests/lifecycle_wiki.rs` - Single finalization and durable Wiki behavior before restart.
- `crates/cli/tests/tool_loop.rs` - Process-scoped full-access reset to confirmation after restart.
- `crates/tui/tests/command_render.rs` - Exact `/retry` and `/continue` public command outcomes.

## Decisions Made

- Preserved source-level traceability without requiring one Rust filename per TypeScript filename. Multiple public outcomes receive distinct global responsibility IDs even when one historical source or Rust suite covers them.
- Allowed only `rust_covered`, `package_smoke`, and `retired`. Retirement requires a concrete dormant, internal, or unshipped rationale and cannot cite a locked public contract.
- Required Rust evidence to name both an existing `.rs` file and an exact test function present in that file. Package-only evidence is limited to the reviewed Phase 10 JavaScript smoke allowlist.
- Kept every transitional source file in place. This plan changes decision authority and evidence only; deletion remains gated by later Phase 11 plans.

## Deviations from Plan

### Local toolchain substitution

- **Found during:** Task 1 RED
- **Issue:** The workstation's default MSVC linker was unavailable, and the installed GNU-LLVM toolchain's default external Clang linker was also absent.
- **Resolution:** Ran all Rust development evidence with the already-installed GNU-LLVM toolchain and its bundled `rust-lld` through process-local environment variables.
- **Scope:** Validation only. No repository, release, CI, or hosted configuration changed, and no toolchain or dependency was downloaded.

## Issues Encountered

- The first verification-order assertion searched the whole source file and matched an import before the call site. Scoping the assertion to `verify_repository` made it prove the intended preflight order.

## Verification

- `cargo fmt --all -- --check` - passed.
- `cargo test -p minimax-compat-harness --test coverage --locked` - 6 passed.
- `cargo test -p minimax-cli --test headless --test lifecycle_wiki --test tool_loop --locked` - 22 passed.
- `cargo test -p minimax-tui --test command_render --locked` - 7 passed.
- `cargo run -p minimax-compat-harness --locked -- verify-candidate` - passed.
- `cargo clippy -p minimax-compat-harness -p minimax-cli -p minimax-tui --all-targets --locked -- -D warnings` - passed.
- Matrix inspection - 97 sources, 101 responsibilities, no unresolved disposition, and no TypeScript authority evidence.

## Known Stubs

None. Strings such as `requires_port` and `placeholder` occur only in validator rejection rules and negative tests.

## User Setup Required

None - all work was local/offline and used no credentials, publication, downloads, Provider calls, hosted workflow runs, or external APIs.

## Next Phase Readiness

- Plan 11-02 can replace evaluator decision authority against a complete, strict historical responsibility baseline.
- Later TypeScript deletion can require the same matrix and exact Rust evidence rather than relying on file-count parity.
- Hosted evidence remains intentionally untouched until the planned final stable fingerprint refresh.

## Self-Check: PASSED

- The canonical summary exists and all four TDD commits resolve in git history.
- Formatting, focused Rust suites, strict Clippy, and candidate verification pass under the documented local development toolchain substitution.
- No tracked sources were deleted, no unresolved responsibility remains, and no TypeScript file is cited as behavioral authority.

---
*Phase: MMX-11-rust-verification-and-evaluation-authority*
*Completed: 2026-07-17*
