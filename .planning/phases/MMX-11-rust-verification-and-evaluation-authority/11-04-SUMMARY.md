---
phase: MMX-11-rust-verification-and-evaluation-authority
plan: "04"
subsystem: rust-evaluation-release-authority
tags: [rust, provider, retrieval, npm, ci, source-authority]
requires:
  - phase: MMX-11-rust-verification-and-evaluation-authority
    provides: Deterministic Rust Provider and retrieval evaluators from Plans 11-02 and 11-03
provides:
  - exact Rust-only npm aliases for Provider and retrieval evaluation
  - coverage-then-Provider-then-retrieval release ordering before build, package, or evidence
  - fail-closed package and CI structural enforcement against TypeScript evaluator execution
  - preserved inert hash-pinned TypeScript evaluator sources for Phase 14 removal
affects: [phase-12-compatibility-migration, phase-13-npm-native-release, phase-14-typescript-removal]
tech-stack:
  added: []
  patterns: [exact package-command authority, fail-closed CI gate ordering, inert transitional source evidence]
key-files:
  created:
    - .planning/phases/MMX-11-rust-verification-and-evaluation-authority/11-04-SUMMARY.md
  modified:
    - package.json
    - .github/workflows/ci.yml
    - crates/compat-harness/src/source_authority.rs
    - crates/compat-harness/tests/source_authority.rs
    - test/ci-contract.ts
    - test/ci-contract.test.ts
    - fixtures/compat/source-authority.v1.json
    - fixtures/compat/verification/typescript-responsibilities.v1.json
key-decisions:
  - "eval:provider and eval:retrieval are exact cargo-run aliases for the Rust compatibility harness; verify:agent composes coverage, Provider, then retrieval."
  - "CI must pass strict or candidate Rust coverage, Provider evaluation, and retrieval evaluation in that order before release build, package, or evidence steps."
  - "TypeScript tests, static checks, build, and smoke remain transitional, but TypeScript evaluator commands and src/eval paths are denied by package and CI authority checks."
patterns-established:
  - "Release aggregates are compared to exact commands so reordering, omission, or a TypeScript fallback fails closed."
  - "CI validation binds both named Rust evaluator steps and their predecessor relationship to every downstream build, package, and evidence step."
requirements-completed: [RVE-03]
coverage:
  - id: D1
    description: "Both npm evaluation aliases execute the deterministic Rust JSON reports and aggregate verification runs coverage, Provider, then retrieval."
    requirement: RVE-03
    verification:
      - kind: integration
        ref: "npm run eval:provider"
        status: pass
      - kind: integration
        ref: "npm run eval:retrieval"
        status: pass
      - kind: contract
        ref: "crates/compat-harness/tests/source_authority.rs#evaluator_package_scripts_are_rust_only_and_ordered_before_release_builds"
        status: pass
    human_judgment: false
  - id: D2
    description: "Coverage, Provider, and retrieval failures block CI before build, package, installed verification, or milestone evidence."
    requirement: RVE-03
    verification:
      - kind: contract
        ref: "crates/compat-harness/tests/source_authority.rs#ci_keeps_rust_authority_ahead_of_packaging_and_fails_closed"
        status: pass
      - kind: contract
        ref: "test/ci-contract.test.ts#Rust evaluations cannot move behind build, package, or hosted evidence"
        status: pass
    human_judgment: false
  - id: D3
    description: "Package and CI contain no TypeScript evaluator route while both transitional evaluator files remain unchanged and hash-pinned."
    requirement: RVE-03
    verification:
      - kind: contract
        ref: "crates/compat-harness/tests/source_authority.rs#rejects_transitional_typescript_evaluator_routes"
        status: pass
      - kind: structural
        ref: "rg -n src/eval/(provider-conformance|capability-retrieval-report)|ts-node package.json .github/workflows/ci.yml"
        status: pass
    human_judgment: false
duration: 26min
completed: 2026-07-18
status: complete
---

# Phase 11 Plan 04: Rust Evaluation Release Authority Summary

**Rust Provider and retrieval reports are now the only package and CI evaluation authority, with coverage and both reports blocking every build, package, and evidence step while the inert TypeScript evaluator sources remain untouched.**

## Performance

- **Duration:** 26 min
- **Started:** 2026-07-17T16:30:00Z
- **Completed:** 2026-07-17T16:56:00Z
- **Tasks:** 1
- **Files created/modified:** 9
- **Execution mode:** Generic-agent workaround with the complete `gsd-executor` contract loaded.

## Accomplishments

- Replaced the retrieval TypeScript package route with the exact Rust `retrieval-eval --format json` command and retained the already-Rust Provider alias.
- Made `verify:agent` run Rust coverage, Provider evaluation, then retrieval evaluation, and made `verify:release` depend on that aggregate before build and release work.
- Reordered CI to run coverage, Rust Provider evaluation, and Rust retrieval evaluation before release build, package assembly, installed verification, and milestone evidence without changing its matrix or read-only permissions.
- Added Rust and TypeScript structural contracts that reject TypeScript evaluator routes, exact-command drift, and moving either Rust evaluator behind downstream release work.
- Kept `src/eval/provider-conformance.ts` and `src/eval/capability-retrieval-report.ts` byte-unchanged and refreshed only the hashes for the deliberately updated CI contract tests.

## Task Commit

1. **Task 1: Make Rust evaluation reports the blocking package and CI authority** - `c1852be` (chore)

The task added failing structural expectations first, completed the implementation, and was committed atomically as the plan's single behavior task. The summary and planning trackers are committed separately in the final metadata commit.

## Files Created/Modified

- `package.json` - Exact Rust Provider/retrieval aliases plus coverage-first aggregate and release order.
- `.github/workflows/ci.yml` - Blocking coverage, Provider, and retrieval steps before all downstream release work.
- `crates/compat-harness/src/source_authority.rs` - Exact package command contracts, TypeScript evaluator denial, and CI ordering enforcement.
- `crates/compat-harness/tests/source_authority.rs` - Positive and negative package/CI authority tests.
- `test/ci-contract.ts` - Workflow step names/order and active TypeScript evaluator rejection.
- `test/ci-contract.test.ts` - Exact package aggregate assertions and downstream-order mutation coverage.
- `fixtures/compat/source-authority.v1.json` - Refreshed hashes for the two changed CI contract test sources.
- `fixtures/compat/verification/typescript-responsibilities.v1.json` - Matching responsibility-matrix hashes for those test sources.
- `.planning/phases/MMX-11-rust-verification-and-evaluation-authority/11-04-SUMMARY.md` - Plan result, evidence, and handoff.

## Decisions Made

- `verify:agent` is the shared package aggregate for Rust contract coverage and both evaluator reports, so `verify:release` cannot accidentally maintain a second order.
- Provider precedes retrieval in both package and CI execution, matching D-11-05 and the structural validators.
- Transitional TypeScript test/static/build/smoke commands remain allowed through Phase 13; evaluator execution is separately and explicitly denied.
- No hosted fingerprint, release fixture, artifact, permission, target matrix, or transitional evaluator source was refreshed from local GNU-LLVM evidence.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing Critical] Extended the existing structural authority gates with the package/CI cutover contract**
- **Found during:** Task 1 RED
- **Issue:** Editing only package and workflow text would make the cutover regressible and would leave the existing source-authority validator permitting TypeScript evaluator routes.
- **Fix:** Updated the Rust and TypeScript structural validators/tests and refreshed the exact hashes of those changed TypeScript test sources in both authority manifests.
- **Files modified:** `crates/compat-harness/src/source_authority.rs`, `crates/compat-harness/tests/source_authority.rs`, `test/ci-contract.ts`, `test/ci-contract.test.ts`, `fixtures/compat/source-authority.v1.json`, `fixtures/compat/verification/typescript-responsibilities.v1.json`
- **Verification:** Focused CI contract tests passed 23/23; Rust source-authority tests passed 12/12; the full TypeScript suite passed 441/441.
- **Committed in:** `c1852be`

**2. [Rule 1 - Tracker correction] Aligned generated planning state with the GSD command results**
- **Found during:** Final planning-state verification
- **Issue:** `state update-progress` reported 36/44 plans and 82% but persisted 79%; the roadmap handler completed the phase and plan-file checklist while leaving the duplicate descriptive 11-04 entry unchecked.
- **Fix:** Corrected STATE to 82%, aligned its phase/activity labels with the generated `verifying` status, and checked the matching ROADMAP entry.
- **Files modified:** `.planning/STATE.md`, `.planning/ROADMAP.md`
- **Verification:** STATE records 36 completed plans and 82%; ROADMAP records Phase 11 at 4/4 complete with both 11-04 entries checked.
- **Committed in:** Final metadata commit

---

**Total deviations:** 2 auto-fixed (1 missing-critical contract gap, 1 tracker correction).
**Impact on plan:** The added files only make the requested package/CI authority durable and hash-consistent, and the tracker correction records the command's actual result; product behavior, platform coverage, and authority scope are unchanged.

## Issues Encountered

- The first local Rust verification attempt inherited Cargo's missing GNU-LLVM clang driver. Re-running with the already-installed `1.97.0-x86_64-pc-windows-gnullvm` toolchain and bundled `rust-lld` passed; no repository configuration, dependency, or hosted evidence changed.

## Verification

- `npx tsx --test test/ci-contract.test.ts` - 23 passed.
- `npm run check` - passed.
- `npm test` - 441 passed.
- `cargo fmt --all -- --check` - passed.
- `cargo clippy -p minimax-compat-harness --all-targets --locked -- -D warnings` - passed.
- `cargo test -p minimax-compat-harness --test source_authority --test coverage --locked` - 18 passed.
- `cargo test -p minimax-compat-harness --test provider_eval --test retrieval_eval --test coverage --locked` - 19 passed.
- `npm run eval:provider` - emitted the deterministic passing 2-protocol, 20-check Rust JSON report.
- `npm run eval:retrieval` - emitted the deterministic passing 175-case Rust JSON report.
- `cargo run -p minimax-compat-harness --locked -- verify-candidate` - passed.
- `rg -n "src/eval/(provider-conformance|capability-retrieval-report)|ts-node" package.json .github/workflows/ci.yml` - no matches.
- `git diff --check` - passed before the task commit.

All Rust development evidence used the already-installed `1.97.0-x86_64-pc-windows-gnullvm` toolchain and bundled `rust-lld`. It is local development evidence only and is not represented as Windows MSVC, Linux hosted, or release evidence.

## Known Stubs

None. The cutover uses the completed Rust Provider and retrieval evaluators and introduces no fallback, placeholder, network, Provider credential, model, or download path.

## Threat Surface

No new runtime threat surface was introduced. The change narrows executable evaluator authority, adds no permission, secret, endpoint, subprocess capability, or writable root, and preserves the existing offline/read-only CI boundary.

## User Setup Required

None - both evaluators are local, offline, deterministic, fixture-only, credential-independent, and model-free.

## Next Phase Readiness

- Phase 12 can make the Rust compatibility harness and migration gates the final immutable public-contract authority without a TypeScript evaluator dependency.
- Phase 14 can delete the now-unreachable TypeScript evaluator sources against explicit source-authority and CI regression tests.
- Hosted release evidence remains intentionally stale until the planned final Windows/Linux fingerprint refresh.

## Self-Check: PASSED

- The implementation commit `c1852be` exists and contains exactly the eight task files.
- Both transitional TypeScript evaluator sources still exist at their unchanged hashes and have no package or CI execution route.
- Focused structural tests, full TypeScript tests, Rust evaluator/coverage/source-authority tests, format, Clippy, both npm aliases, and candidate verification pass.
- No hosted fingerprint, release evidence, package artifact, external provider, credential, embedding model, download, or network resource was changed or used.

---
*Phase: MMX-11-rust-verification-and-evaluation-authority*
*Completed: 2026-07-18*
