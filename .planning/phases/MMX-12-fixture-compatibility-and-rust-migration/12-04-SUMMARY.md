---
phase: 12-fixture-compatibility-and-rust-migration
plan: "04"
subsystem: migration
tags: [rust, migration, rollback, symlink-containment, ownership-provenance]

requires:
  - phase: 12-02-migration-release-evidence
    provides: Source-preserving Rust migration, durable operation records, and narrow rollback lifecycle
provides:
  - Durable pre-write created/reused dispositions bound across plan, operation, and receipt records
  - Exact rollback authorization that rejects recomputed forged ownership claims
  - Component-wise no-symlink target resolution with canonical-root containment before writes and removals
affects: [13-thin-npm-and-native-release, 14-typescript-removal-and-hosted-closure, migration, compatibility]

tech-stack:
  added: []
  patterns:
    - Pre-write ownership provenance is authoritative; unkeyed record hashes are corruption checks only
    - Every target-side path component is inspected without following symlinks and rechecked below the canonical root

key-files:
  created: []
  modified:
    - crates/cli/src/migration.rs
    - crates/cli/tests/migration.rs

key-decisions:
  - "Authorize rollback only when the fixed durable plan, operation, and receipt agree on migration identity, target content, and pre-write created/reused disposition."
  - "Reject symlinked existing ancestors and canonical escapes before lock, staging, durable evidence, artifact, report, or removal operations."

patterns-established:
  - "Deletion authority: a target is removable only when the durable plan observed it absent, the operation recorded it created, and its current exact bytes still match."
  - "Contained mutation: parent directories are created one component at a time and every existing or newly created parent is revalidated under the canonical target root."

requirements-completed: [RCMP-02]

coverage:
  - id: D1
    description: Recomputed operation and receipt hashes cannot reclassify a pre-existing byte-identical allowlisted migration target as created or authorize its deletion.
    requirement: RCMP-02
    verification:
      - kind: integration
        ref: crates/cli/tests/migration.rs#gap_closure_forged_created_claims_cannot_delete_preexisting_allowlisted_targets
        status: pass
      - kind: integration
        ref: cargo test -p minimax-cli --test migration --locked
        status: pass
    human_judgment: false
  - id: D2
    description: A symlinked .minimax or nested target ancestor fails before any lock, staging, receipt, artifact, report, or external-directory write.
    requirement: RCMP-02
    verification:
      - kind: integration
        ref: crates/cli/tests/migration.rs#gap_closure_target_ancestor_symlinks_fail_before_any_external_write
        status: pass
      - kind: integration
        ref: cargo test -p minimax-cli --test migration --locked
        status: pass
    human_judgment: false
  - id: D3
    description: Source immutability, idempotent replay, interruption recovery, exact-byte narrow rollback, fixture provenance, and the support-window gate remain intact.
    requirement: RCMP-02
    verification:
      - kind: integration
        ref: cargo test -p minimax-cli --test migration --locked
        status: pass
      - kind: integration
        ref: cargo test -p minimax-compat-harness --test migration_support --locked
        status: pass
      - kind: integration
        ref: cargo run -p minimax-compat-harness --locked -- verify-candidate
        status: pass
    human_judgment: false

duration: 2h 46m
completed: 2026-07-18
status: complete
---

# Phase 12 Plan 04: Bind Migration Ownership and Contain Target Writes Summary

**Rust migration now binds deletion authority to durable pre-write ownership and rejects symlinked target ancestors before any write can escape the canonical project root.**

## Performance

- **Duration:** 2h 46m
- **Started:** 2026-07-18T06:24:29+08:00
- **Completed:** 2026-07-18T09:10:26+08:00
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments

- Persisted each target's absent or byte-identical-existing disposition before publication and cross-bound that fact through the durable plan, operation, and receipt records.
- Made rollback and interrupted recovery delete only exact durable planned-created targets, so forged created claims, reused targets, changed targets, missing targets, and unowned paths survive.
- Replaced lexical-only target joins with component-wise no-symlink resolution and canonical containment checks before lock, directory creation, evidence publication, artifact writes, verification, recovery, and removal.

## Task Commits

Each TDD gate was committed atomically:

1. **Task 1 RED: Reproduce forged created claims and target-ancestor symlink writes** - `2e27e81` (test)
2. **Task 2 GREEN: Persist ownership provenance and gate every target path component** - `c5a99b8` (fix)

## Files Created/Modified

- `crates/cli/src/migration.rs` - Durable plan/operation/receipt ownership validation and component-safe canonical target resolver.
- `crates/cli/tests/migration.rs` - Forged allowlisted ownership matrix, target-ancestor symlink containment, and preserved recovery/idempotency/rollback regression tests.

## Decisions Made

- A receipt or operation self-hash proves only record integrity; it never supplies deletion authority. Authority comes from exact agreement with the fixed pre-write plan snapshot.
- Existing target ancestors are inspected without following symlinks, and containment is rechecked immediately before every target-side mutation.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking Environment] Used the installed offline GNU-LLVM development toolchain**
- **Found during:** Plan-level Clippy verification
- **Issue:** The repository-pinned MSVC toolchain could not link because this workstation has no `link.exe`.
- **Fix:** Re-ran Rust checks with the already-installed `1.97.0-x86_64-pc-windows-gnullvm` toolchain, bundled `rust-lld`, `CARGO_NET_OFFLINE=true`, and isolated `target/phase12-04-gnullvm`.
- **Files modified:** None
- **Verification:** Strict Clippy and every planned test/check passed.
- **Committed in:** Not applicable; process-local verification only.

---

**Total deviations:** 1 auto-fixed blocking environment issue.
**Impact on plan:** No product, repository configuration, release evidence, hosted evidence, or dependency changed. GNU-LLVM results are development-only and do not claim Windows MSVC or Linux GNU release authority.

## Issues Encountered

- The first Clippy attempt selected the unavailable MSVC linker and exited before checking project code. The controlled offline GNU-LLVM/rust-lld rerun passed.

## User Setup Required

None - no external service configuration required.

## Verification

- `cargo fmt --all -- --check` - passed.
- Offline GNU-LLVM `cargo clippy -p minimax-cli --all-targets --locked -- -D warnings` - passed.
- Offline GNU-LLVM `cargo test -p minimax-cli --test migration --locked` - 17 passed, 0 failed.
- Offline GNU-LLVM `cargo test -p minimax-compat-harness --test migration_support --locked` - 4 passed, 0 failed.
- Offline GNU-LLVM `cargo run -p minimax-compat-harness --locked -- verify-candidate` - passed.
- `git diff --check 2e27e81^..c5a99b8` - passed.
- No Provider request, credential read, network access, dependency download, package publication, push, PR, tag, merge, hosted-evidence refresh, or real user-data migration occurred.

## Next Phase Readiness

- RCMP-02's negative ownership and target-containment gaps are closed and ready for Phase 12 re-verification.
- Phase 13 can consume the Rust migration boundary after Phase 12 verification is marked passed.
- Final hosted Windows MSVC/Linux GNU fingerprint evidence remains explicitly owned by Phase 14.

## Self-Check: PASSED

- Modified source and test files exist.
- RED/GREEN commits `2e27e81` and `c5a99b8` exist on `codex/rust-convergence-v3`.
- All plan-level formatting, Clippy, migration, compatibility-support, and candidate checks pass offline.
- Every coverage deliverable has passing automated evidence and requires no subjective UAT.

---
*Phase: 12-fixture-compatibility-and-rust-migration*
*Completed: 2026-07-18*
