---
phase: MMX-01-contract-foundation
plan: "04"
subsystem: verification
tags: [compatibility-report, cargo-metadata, architecture-gate, ci, windows, linux]
requires:
  - phase: MMX-01-contract-foundation
    provides: typed protocol, deterministic reducer, and Provider fixture normalization
provides:
  - Deterministic evidence-backed matched, pending, and approved-difference reporting
  - Cargo metadata enforcement for dependency direction, cycles, harness isolation, and database exclusion
  - One credential-free Rust contract command in Windows and Linux CI
affects: [ci, architecture, compatibility, release-gates]
tech-stack:
  added: []
  patterns: [strict manifest loading, golden deterministic report, metadata graph policy, exact CI allowlist]
key-files:
  created:
    - crates/compat-harness/src/manifest.rs
    - crates/compat-harness/src/report.rs
    - crates/compat-harness/src/architecture.rs
    - crates/compat-harness/src/main.rs
    - fixtures/compat/report.expected.json
  modified:
    - fixtures/compat/baseline-status.v1.json
    - package.json
    - .github/workflows/ci.yml
key-decisions:
  - "Only rust.provider_protocols is promoted to matched; Rust commands, profiles, permissions, and product entry remain honestly pending."
  - "CI uses runner-provided rustup and the repository-pinned 1.97.0 toolchain without a third-party Rust setup action."
patterns-established:
  - "Matched compatibility claims require an existing evidence path; pending claims cannot carry success evidence."
  - "Production dependency direction and the no-database choice are executable Cargo metadata policy."
requirements-completed: [ARCH-02, ARCH-04, COMP-04]
coverage:
  - id: D1
    description: "Strict manifests produce a sorted golden report whose second run is byte-identical and whose matched entries require real evidence."
    requirement: COMP-04
    verification:
      - kind: integration
        ref: "cargo test -p minimax-compat-harness --locked compat_report (3/3)"
        status: pass
    human_judgment: false
  - id: D2
    description: "Real Cargo metadata passes while synthetic forbidden core, harness, cycle, and database cases fail with exact diagnostics."
    requirement: ARCH-02
    verification:
      - kind: integration
        ref: "cargo test -p minimax-compat-harness --locked architecture (5/5)"
        status: pass
    human_judgment: false
  - id: D3
    description: "One offline command verifies manifests, Provider fixtures, architecture, and deterministic output, and CI runs locked Rust gates on Windows and Linux."
    requirement: ARCH-04
    verification:
      - kind: e2e
        ref: "npm run verify:rust-contracts; CI contract tests (20/20); npm test (432/432)"
        status: pass
      - kind: integration
        ref: "cargo fmt; cargo clippy -D warnings; cargo test --workspace --locked"
        status: pass
    human_judgment: false
duration: 15min
completed: 2026-07-15
status: complete
---

# Phase 1 Plan 4: Compatibility and Architecture Gates Summary

**Phase 1 contracts are now executable gates: false parity claims, forbidden dependencies, database packages, nondeterministic reports, and unsupported CI changes fail automatically.**

## Performance

- **Duration:** 15 min
- **Started:** 2026-07-15T09:06:00Z
- **Completed:** 2026-07-15T09:20:42Z
- **Tasks:** 3
- **Files modified:** 13

## Accomplishments

- Built strict command, Provider, and baseline manifest loading plus a sorted golden compatibility report that remains byte-identical across runs.
- Turned crate boundaries into Cargo metadata policy with exact negative tests for core-to-adapter, production-to-harness, cycles, and database packages.
- Added a dependency-free `minimax-compat-harness verify` command and made both Windows and Linux CI run pinned fmt, Clippy, workspace tests, and contract verification without credentials.

## Task Commits

1. **Task 1: Generate deterministic compatibility reports** - `f1e8ef8`
2. **Task 2: Enforce Cargo architecture policy** - `d9830a7`
3. **Task 3: Gate Rust contracts on Windows and Linux** - `0a5bdec`

## Files Created/Modified

- `crates/compat-harness/src/manifest.rs` - Loads strict schema-versioned compatibility manifests from a repository-root path independent of the current directory.
- `crates/compat-harness/src/report.rs` - Sorts, serializes, and validates evidence-backed parity claims.
- `crates/compat-harness/src/architecture.rs` - Converts locked Cargo metadata into a validated dependency graph.
- `crates/compat-harness/src/main.rs` - Exposes `verify` and `report --format json` without a CLI framework.
- `crates/compat-harness/tests/compat_report.rs` - Covers golden reports and positive/negative architecture cases.
- `fixtures/compat/report.expected.json` - Locks the initial deterministic Rust parity report.
- `package.json` - Adds Rust check, test, and aggregate verification scripts without changing `bin`.
- `.github/workflows/ci.yml` - Runs the pinned Rust gates on `ubuntu-latest` and `windows-latest`.
- `test/ci-contract.ts` and `test/ci-contract.test.ts` - Keep the CI shape exact and credential-free after adding Rust gates.

## Decisions Made

- Promoted only Provider protocol behavior proven by executable Rust fixtures; all unimplemented product behavior remains `pending` rather than being inferred from scaffolding.
- Kept the compatibility harness development-only and rejected every production edge into it.
- Kept CI installation on runner-provided `rustup`; no third-party action, credential, cache secret, live Provider call, or embedding download was added.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Added Serde to the compatibility harness manifest**

- **Found during:** Task 1 strict manifest compilation
- **Issue:** Strict typed JSON manifests require Serde derives, but the task file list omitted the harness Cargo manifest and lockfile.
- **Fix:** Reused the exact workspace-owned Serde dependency and refreshed only the local dependency edge in `Cargo.lock`.
- **Files modified:** `crates/compat-harness/Cargo.toml`, `Cargo.lock`
- **Verification:** Locked harness and workspace tests pass with no new external package.
- **Committed in:** `f1e8ef8`

**2. [Rule 3 - Blocking] Extended the existing exact CI validator for the new Rust gates**

- **Found during:** Task 3 full TypeScript regression
- **Issue:** The repository intentionally rejects any CI step outside an exact eight-step allowlist, so the required Rust steps initially failed one of 432 tests.
- **Fix:** Extended the allowlist to exactly twelve ordered steps, added exact Rust script assertions, and retained all secret, smoke, permission, shell, and extra-step rejections.
- **Files modified:** `test/ci-contract.ts`, `test/ci-contract.test.ts`
- **Verification:** Focused CI contract tests pass 20/20 and the full TypeScript suite passes 432/432.
- **Committed in:** `0a5bdec`

**Total deviations:** 2 auto-fixed blocking prerequisites.
**Impact:** Both fixes strengthen the planned checks without changing product behavior, dependency policy, or authorization scope.

## Issues Encountered

- The first full TypeScript run correctly caught that its existing CI allowlist had not yet been updated; the repaired exact contract passed both focused and full reruns.

## User Setup Required

None - verification is fixture-only and CI installs the pinned base toolchain through runner-provided rustup.

## Next Phase Readiness

- Phase 1 has complete typed protocol, deterministic fixture, compatibility, architecture, and cross-platform CI evidence.
- The TypeScript CLI remains the product entry while Phase 2 can implement the first usable Rust runtime slice behind these gates.

## Self-Check: PASSED

- All three task commits and all claimed files exist; the worktree is clean before planning metadata updates.
- Eight harness tests, four core tests, five protocol tests, and two Provider tests pass under the official Rust 1.97.0 local gnullvm toolchain.
- Workspace fmt and Clippy with `-D warnings`, locked workspace tests, two aggregate verification runs, and byte-identical 1,398-byte reports pass.
- `npm run check`, `npm run build`, all 432 TypeScript tests, and `git diff --check` pass; `package.json` still launches `dist/cli.js`.

---
*Phase: MMX-01-contract-foundation*
*Completed: 2026-07-15*
