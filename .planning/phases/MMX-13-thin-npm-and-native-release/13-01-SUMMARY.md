---
phase: 13-thin-npm-and-native-release
plan: "01"
subsystem: distribution
tags: [npm, rust, launcher, source-authority, offline-verification]

requires:
  - phase: 12-fixture-compatibility-and-rust-migration
    provides: Rust-only product, compatibility, evaluation, and migration authority
provides:
  - Dependency-free npm distribution metadata with one minimax-codex command
  - Stable fail-closed launcher taxonomy for supported native siblings
  - Rust-owned package and source-authority tests that reject legacy runtime reintroduction
affects: [13-02-deterministic-candidates, 13-03-offline-corruption-guards, 14-typescript-removal-and-hosted-closure]

tech-stack:
  added: []
  patterns:
    - npm metadata describes distribution only and contains no application runtime dependencies
    - The CJS launcher selects one fixed sibling binary and emits categorized no-fallback errors

key-files:
  created: []
  modified:
    - package.json
    - package-lock.json
    - bin/minimax-codex.cjs
    - crates/compat-harness/src/baseline.rs
    - crates/compat-harness/src/source_authority.rs
    - crates/compat-harness/tests/compat_report.rs
    - crates/compat-harness/tests/source_authority.rs
    - crates/cli/tests/product_identity.rs
    - fixtures/compat/source-authority.v1.json

key-decisions:
  - "Keep npm as a dependency-free distribution shell: one bin, seven allowed packaged paths, and Rust/release-only scripts."
  - "Use stable E_* launcher categories with exact expected-path or supported-target guidance while preserving child argv and exit codes."
  - "Treat local GNU-LLVM/rust-lld validation as development-only; the intentionally stale hosted fingerprint remains Phase 14 work."

patterns-established:
  - "Thin package contract: package.json and package-lock.json are exact allowlists, so dependencies, lifecycle hooks, legacy bins, and extra scripts fail closed."
  - "Launcher contract: unsupported, missing, unsafe, non-executable, start-failed, and signal outcomes have stable categories and never search, download, or fall back."

requirements-completed: [RNPM-01, RNPM-02]

coverage:
  - id: D1
    description: The npm package exposes one minimax-codex command and contains no TypeScript, React, Ink, keyring, legacy bin, install hook, or legacy application graph.
    requirement: RNPM-02
    verification:
      - kind: integration
        ref: crates/compat-harness/tests/compat_report.rs#thin_npm_manifest_and_lock_are_distribution_only
        status: pass
      - kind: other
        ref: npm pack --ignore-scripts --dry-run --json with staged minimax-codex.exe
        status: pass
    human_judgment: false
  - id: D2
    description: Supported npm and npx paths start only the fixed packaged Rust sibling, preserve arguments and exit codes, and fail with stable actionable no-fallback errors.
    requirement: RNPM-01
    verification:
      - kind: integration
        ref: crates/compat-harness/tests/compat_report.rs#rust_command_permission_provider_and_product_baselines_are_executable
        status: pass
      - kind: integration
        ref: crates/cli/tests/product_identity.rs#npm_launcher_defines_the_stable_fail_closed_error_taxonomy
        status: pass
      - kind: other
        ref: node bin/minimax-codex.cjs --version and index capabilities status with staged Rust binary
        status: pass
    human_judgment: false

duration: 12min
completed: 2026-07-18
status: complete
---

# Phase 13 Plan 01: Thin npm Metadata and Launcher Summary

**npm is now a dependency-free Rust distribution shell with one fixed native command and stable, actionable no-fallback launcher failures.**

## Performance

- **Duration:** 12 min
- **Started:** 2026-07-18T09:22:13+08:00
- **Completed:** 2026-07-18T09:34:01+08:00
- **Tasks:** 2
- **Files modified:** 9

## Accomplishments

- Removed the legacy TypeScript/React/Ink/keyring dependency graph, generated `dist` path, old application scripts, and legacy lockfile entries from npm metadata.
- Locked package metadata, files, scripts, dependencies, lifecycle hooks, bin mapping, and lockfile shape behind exact Rust-owned positive and negative tests.
- Added stable launcher error categories with concrete path/target guidance while preserving Unicode/metacharacter argv, shell-free spawn, exact sibling selection, and child exit codes.

## Task Commits

Each TDD task was committed atomically:

1. **Task 1 RED: Expose legacy npm application surface** - `c8b1f3b` (test)
2. **Task 1 GREEN: Prune npm to Rust distribution metadata** - `30a2355` (feat)
3. **Task 2 RED: Lock launcher failure taxonomy** - `2be9759` (test)
4. **Task 2 GREEN: Stabilize fail-closed npm launcher** - `ad72aab` (feat)

## Files Created/Modified

- `package.json` - Exact dependency-free distribution files and Rust/release-only scripts.
- `package-lock.json` - Mechanically regenerated single-root, zero-dependency lockfile.
- `bin/minimax-codex.cjs` - Fixed native sibling launcher and stable E_* error taxonomy.
- `crates/compat-harness/src/baseline.rs` - Package-lock and product-entry validation.
- `crates/compat-harness/src/source_authority.rs` - Exact thin-package metadata/script allowlist.
- `crates/compat-harness/tests/compat_report.rs` - Package negatives and end-to-end launcher behavior.
- `crates/compat-harness/tests/source_authority.rs` - Updated thin-package synthetic fixtures and source gates.
- `crates/cli/tests/product_identity.rs` - Stable launcher error and no-download contract.
- `fixtures/compat/source-authority.v1.json` - Updated launcher source hash.

## Decisions Made

- Keep npm metadata intentionally exact instead of accepting arbitrary future fields or scripts; distribution changes must update the Rust-owned contract deliberately.
- Report launcher failures as stable `E_*` categories and always include either the concrete expected path or the two supported packaged target identities.
- Do not refresh or forge hosted evidence locally after product fingerprint drift; Phase 13 uses the existing candidate verification branch until Phase 14 runs the hosted matrix.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing Critical] Enforced the thin package in the source-authority validator**
- **Found during:** Task 1 (package metadata pruning)
- **Issue:** The declared task files omitted `crates/compat-harness/src/source_authority.rs`, but its legacy `dev`/`start` requirements would reject the planned package and leave extra scripts/hooks insufficiently constrained.
- **Fix:** Replaced the legacy route checks with exact top-level, file, identity, engine, and Rust/release script allowlists.
- **Files modified:** `crates/compat-harness/src/source_authority.rs`
- **Verification:** Package mutation tests, source-authority suite, strict Clippy, and `verify-candidate` pass.
- **Committed in:** `30a2355`

**2. [Rule 1 - Bug] Updated stale source-authority tests to the post-prune contract**
- **Found during:** Task 2 plan-level source-authority verification
- **Issue:** Synthetic fixtures still required removed TypeScript build/test/smoke scripts and one test searched for an obsolete verifier function name.
- **Fix:** Based synthetic metadata on the committed thin manifest, asserted legacy scripts are absent, and checked the current compatibility verification call ordering.
- **Files modified:** `crates/compat-harness/tests/source_authority.rs`
- **Verification:** All 13 source-authority tests pass.
- **Committed in:** `ad72aab`

---

**Total deviations:** 2 auto-fixed (1 missing critical contract, 1 stale-test bug).
**Impact on plan:** Both changes were required to enforce and verify the planned thin distribution boundary; no product scope, dependency, publication, or external action was added.

## Issues Encountered

- Strict compatibility evidence is intentionally stale after the launcher/package fingerprint changed. The candidate suite passed with only `hosted_cutover_evidence_matches_current_product` excluded, matching the Phase 13/14 contract.
- The local GNU-LLVM binary initially could not start without its toolchain runtime on `PATH`. Re-running the staged smoke with the installed toolchain bin directory succeeded; this remains development-only evidence.

## User Setup Required

None - no external service configuration required.

## Verification

- `npm install --package-lock-only --ignore-scripts --offline` - passed; one package audited, zero dependencies.
- Offline GNU-LLVM `cargo fmt --all -- --check` - passed.
- Offline GNU-LLVM `cargo clippy --workspace --all-targets --locked -- -D warnings` - passed.
- Offline GNU-LLVM `cargo test -p minimax-cli --test product_identity --locked` - 4 passed.
- Offline GNU-LLVM `cargo test -p minimax-compat-harness --test compat_report --locked -- --skip hosted_cutover_evidence_matches_current_product` - 24 passed, 1 intentionally filtered.
- Offline GNU-LLVM `cargo test -p minimax-compat-harness --test source_authority --locked` - 13 passed.
- Offline GNU-LLVM `cargo run -p minimax-compat-harness --locked -- verify-candidate` - passed.
- Staged launcher `--version` and `index capabilities status` - passed against the exact sibling Rust binary.
- `npm pack --ignore-scripts --dry-run --json` with staged Windows binary - 10 thin entries; native binary present; no `dist/cli.js` or lockfile leak.
- No network request, credential read, dependency download, runtime download, publish, push, PR, tag, merge, or hosted trigger occurred.

## Next Phase Readiness

- Plan 13-02 can consume the exact package/launcher contract to build deterministic native and npm candidates.
- Hosted Windows x64 MSVC/Linux x64 GNU evidence remains explicitly deferred to the authorized Phase 14 workflow.

## Self-Check: PASSED

- All nine modified contract, launcher, fixture, and test files exist and are committed.
- RED/GREEN commits `c8b1f3b`, `30a2355`, `2be9759`, and `ad72aab` exist on `codex/rust-convergence-v3`.
- Package, launcher, source-authority, Clippy, candidate, dry-run pack, version, and read-only capability checks pass offline.
- Every shipped deliverable has passing automated evidence and requires no subjective UAT.

---
*Phase: 13-thin-npm-and-native-release*
*Completed: 2026-07-18*
