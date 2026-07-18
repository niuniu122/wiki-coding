---
phase: MMX-10-rust-authority-and-source-boundaries
plan: "02"
subsystem: executable-and-state-authority
tags: [rust, npm-launcher, state-authority, migration, compatibility]
requires:
  - phase: MMX-10-rust-authority-and-source-boundaries
    plan: "01"
    provides: Hash-pinned source, executable, package-entry, and state authority contract
provides:
  - exactly one supported npm command mapped to the fixed Rust launcher
  - fail-closed launcher diagnostics with no legacy interpreter fallback
  - normalized `.minimax` runtime state paths and source-preserving `.mini-codex` migration
  - deterministic Rust-owned executable and filesystem authority tests
affects: [phase-11-typescript-retirement, phase-13-thin-npm, phase-14-hosted-closure]
tech-stack:
  added: []
  patterns: [single executable authority, canonical state roots, full-tree mutation snapshots, receipt-scoped rollback]
key-files:
  created:
    - crates/cli/tests/state_authority.rs
  modified:
    - package.json
    - bin/minimax-codex.cjs
    - crates/compat-harness/src/baseline.rs
    - crates/compat-harness/tests/compat_report.rs
    - crates/cli/src/main.rs
    - crates/cli/tests/headless.rs
    - crates/vault/src/runtime/mod.rs
    - fixtures/compat/source-authority.v1.json
key-decisions:
  - "The supported npm surface exposes only minimax-codex; TypeScript remains inert source evidence and is not deleted in Phase 10."
  - "All project runtime writes are normalized descendants of .minimax, while .mini-codex is accepted only as a read-only migration input."
  - "Migration rollback removes only unchanged receipt-owned targets and never modifies the legacy source tree."
patterns-established:
  - "Rust integration tests snapshot directories and file hashes before and after representative supported paths to prove write authority."
  - "Launcher failure tests use local fixture executables and require fixed sibling selection, shell-free argv forwarding, child outcome preservation, and zero fallback guidance."
requirements-completed: [RUST-01, RUST-03]
coverage:
  - id: D1
    description: "package.json exposes exactly one supported bin and no supported script reaches dist/cli.js."
    requirement: RUST-01
    verification:
      - kind: integration
        ref: "crates/cli/tests/headless.rs#npm_product_entry_uses_only_rust_launcher"
        status: pass
      - kind: integration
        ref: "crates/compat-harness/tests/compat_report.rs#rust_command_permission_provider_and_product_baselines_are_executable"
        status: pass
    human_judgment: false
  - id: D2
    description: "The fixed launcher fails closed for missing, unsafe, non-executable, unsupported, start-failure, and signal outcomes without another interpreter or PATH search."
    requirement: RUST-01
    verification:
      - kind: integration
        ref: "cargo test -p minimax-compat-harness --test compat_report --locked -- --skip hosted_cutover_evidence_matches_current_product"
        status: pass
      - kind: other
        ref: "local direct-versus-launcher --version identity probe"
        status: pass
    human_judgment: false
  - id: D3
    description: "Doctor, index, session, Vault binding, and migration paths never create legacy state and all project mutations remain normalized below .minimax."
    requirement: RUST-03
    verification:
      - kind: integration
        ref: "crates/cli/tests/state_authority.rs#supported_state_paths_write_only_normalized_minimax_descendants"
        status: pass
      - kind: integration
        ref: "crates/cli/tests/state_authority.rs#runtime_store_normalizes_an_aliased_project_root_before_writing"
        status: pass
    human_judgment: false
  - id: D4
    description: "Migration inventory and dry-run are deterministic and read-only; apply and rollback preserve the source byte-for-byte and are receipt-scoped."
    requirement: RUST-03
    verification:
      - kind: integration
        ref: "crates/cli/tests/state_authority.rs#legacy_migration_is_read_only_at_source_and_receipt_scoped_at_target"
        status: pass
      - kind: integration
        ref: "cargo test -p minimax-cli --test migration --locked"
        status: pass
    human_judgment: false
duration: 31min
completed: 2026-07-17
status: complete
---

# Phase 10 Plan 02: Sole Executable and Writable Authority Summary

**The supported npm command now has one fixed Rust execution path, and Rust-owned filesystem tests prove `.minimax` is the only project write authority while legacy `.mini-codex` data remains source-preserving migration input.**

## Performance

- **Duration:** 31 min
- **Started:** 2026-07-17T12:10:46Z
- **Completed:** 2026-07-17T12:41:41Z
- **Tasks:** 2
- **Files created/modified:** 9

## Accomplishments

- Removed the supported legacy npm bin and start route while retaining the normal `minimax-codex` launcher and the inert TypeScript tree for later retirement.
- Replaced legacy fallback guidance with stable Windows/Linux reinstall guidance and covered every fixed launcher outcome, exact argv forwarding, child exit propagation, and signal handling through Rust-owned fixtures.
- Added deterministic tree snapshots across doctor, indexes, runtime sessions, Vault binding, migration inventory/dry-run/apply/rollback, proving `.mini-codex` is never written and all project mutations remain below `.minimax`.
- Canonicalized the runtime project root before constructing its state directory so even aliased input paths expose normalized `.minimax/runtime/v1` descendants.
- Corrected Clap display-version classification so both direct Rust and npm launcher `--version` return status 0 with the identical `minimax-codex-rust 0.1.0` identity.

## Task Commits

Each task was committed atomically with its TDD evidence:

1. **Task 1 RED: Rust-only launcher authority contract** - `9d73082` (test)
2. **Task 1 GREEN: sole supported package command and fail-closed launcher** - `9c27ed8` (feat)
3. **Task 2 RED: deterministic state authority contract** - `7265567` (test)
4. **Task 2 GREEN: normalized Rust runtime state paths** - `183f225` (fix)

The summary and planning trackers are committed together in the final plan metadata commit.

## Files Created/Modified

- `package.json` - One supported `minimax-codex` bin; no supported legacy start route.
- `bin/minimax-codex.cjs` - Stable supported-platform and reinstall guidance only.
- `crates/compat-harness/src/baseline.rs` - Exact sole-bin product-entry validation and fallback rejection.
- `crates/compat-harness/tests/compat_report.rs` - Fixture-driven launcher branch, argv, exit, and signal evidence.
- `crates/cli/src/main.rs` - Successful Clap display help/version maps to completed exit status.
- `crates/cli/tests/headless.rs` - Sole npm authority, Rust version identity, and migration-default assertions.
- `crates/cli/tests/state_authority.rs` - Full-tree writable-root, source-preservation, rollback-scope, and normalized-path tests.
- `crates/vault/src/runtime/mod.rs` - Canonical project root before `.minimax/runtime/v1` construction.
- `fixtures/compat/source-authority.v1.json` - Refreshed hash for the reviewed launcher edit.

## Decisions Made

- Kept TypeScript build/test scripts and the full TypeScript source tree in place as inert transitional evidence; Phase 13 owns package pruning and Phase 14 owns deletion.
- Treated `.mini-codex` only as explicit migration input. Supported runtime and binding code has no legacy writable path or legacy process invocation.
- Kept Vault user data in its explicitly bound external root while storing only the canonical binding record under project `.minimax` state.
- Used complete directory/file snapshots rather than checking a few expected filenames, so new or changed paths outside authority fail deterministically.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Refreshed the source-authority launcher hash**

- **Found during:** Task 1 GREEN validation
- **Issue:** Plan 10-01 deliberately hash-pins allowed JavaScript, so the required launcher diagnostic edit made the authority manifest stale.
- **Fix:** Updated only the reviewed launcher SHA-256 in `source-authority.v1.json`.
- **Files modified:** `fixtures/compat/source-authority.v1.json`
- **Verification:** All six source-authority tests pass.
- **Committed in:** `9c27ed8`

**2. [Rule 1 - Bug] Returned success for Clap display-version output**

- **Found during:** Direct-versus-launcher version verification
- **Issue:** The Rust binary printed the correct version but mapped Clap's successful display result to usage exit 2.
- **Fix:** Map Clap errors whose native `exit_code()` is zero to `ExitClass::Completed`.
- **Files modified:** `crates/cli/src/main.rs`, `crates/cli/tests/headless.rs`
- **Verification:** Direct and launcher `--version` both return 0 and identical output.
- **Committed in:** `9c27ed8`

**3. [Rule 1 - Bug] Normalized aliased runtime project roots**

- **Found during:** Task 2 GREEN state-authority regression
- **Issue:** An input such as `nested/..` wrote to the correct directory but left `..` in the path exposed by `RuntimeStore`.
- **Fix:** Canonicalize the existing project root before joining `.minimax/runtime/v1`.
- **Files modified:** `crates/vault/src/runtime/mod.rs`, `crates/cli/tests/state_authority.rs`
- **Verification:** The focused regression failed before the fix, then all three state-authority and seven migration tests passed.
- **Committed in:** `183f225`

**4. [Rule 3 - Blocking Environment] Used the installed GNU-LLVM toolchain for local evidence**

- **Found during:** Task verification
- **Issue:** The local MSVC linker is unavailable.
- **Fix:** Ran local tests and builds with installed `1.97.0-x86_64-pc-windows-gnullvm` plus `rust-lld`; copied its local `libunwind.dll` only into the temporary launcher probe directory.
- **Files modified:** None
- **Verification:** Formatting, all authorized focused suites, build, and direct/launcher identity checks pass.
- **Committed in:** Not applicable; this is local development evidence only.

---

**Total deviations:** 4 auto-fixed (3 required correctness/contract fixes, 1 blocking environment workaround)
**Impact on plan:** Executable and writable authority are stricter; no TypeScript source was removed and no release/hosted configuration changed.

## Issues Encountered

- The complete compatibility target reports 19 passes and one failure at `hosted_cutover_evidence_matches_current_product` because tracked product inputs intentionally invalidate the previous hosted product fingerprint. This task did not edit `hosted-gates.v1.json` or claim new hosted verification; Phase 14 owns final hosted closure. The authorized local target passed all 19 other tests with only that case filtered.
- The first temporary launcher probe used the wrong directory shape and omitted GNU-LLVM's local runtime DLL. The corrected package-shaped probe (`bin/minimax-codex.cjs`, root `minimax-codex.exe`, temporary `libunwind.dll`) passed and was removed safely afterward.

## Verification

- `cargo fmt --all -- --check` - passed.
- `cargo test -p minimax-cli --test headless --test state_authority --test migration --locked` - 17 passed.
- `cargo test -p minimax-compat-harness --test compat_report --locked -- --skip hosted_cutover_evidence_matches_current_product` - 19 passed, 1 filtered.
- `cargo test -p minimax-compat-harness --test source_authority --locked` - 6 passed.
- Local direct-versus-launcher `--version` probe - both status 0, identical `minimax-codex-rust 0.1.0`.

## User Setup Required

None - all verification is offline and uses no credentials, Provider calls, downloads, or hosted actions.

## Next Phase Readiness

- Plan 10-03 can move CI/verification authority without ambiguity about the supported executable or project writable root.
- Phase 11 can retire TypeScript responsibilities against the unchanged hash-pinned tree.
- Phase 13 can prune package-only TypeScript dependencies/scripts, and Phase 14 can delete inert source and refresh hosted product fingerprint evidence.

## Self-Check: PASSED

- All planned artifacts exist, including `crates/cli/tests/state_authority.rs` and this summary.
- All four task commits exist in git history.
- Authorized local formatting, CLI, compatibility, source-authority, build, and launcher identity checks pass.
- `git diff --name-status aebf732..183f225` contains no deletions.
- No tracked TypeScript/TSX file was edited, renamed, or deleted.

---
*Phase: MMX-10-rust-authority-and-source-boundaries*
*Completed: 2026-07-17*
