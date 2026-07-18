---
phase: MMX-10-rust-authority-and-source-boundaries
plan: "04"
subsystem: package-route-authority
tags: [rust, cargo, npm, source-authority, tdd]
requires:
  - phase: MMX-10-rust-authority-and-source-boundaries
    plan: "03"
    provides: Rust-only release artifacts, installed identity, and Rust-first CI authority
provides:
  - exact npm development and installed routes to the Rust CLI
  - shared fail-closed package product-script validation in both authority preflights
  - explicit preservation of transitional TypeScript build, test, evaluation, and smoke commands
affects: [phase-11-rust-verification, phase-13-thin-npm, phase-14-hosted-closure]
tech-stack:
  added: []
  patterns: [exact Rust package routes, allowlisted transitional TypeScript verification]
key-files:
  created: []
  modified:
    - package.json
    - crates/compat-harness/src/baseline.rs
    - crates/compat-harness/src/source_authority.rs
    - crates/compat-harness/tests/source_authority.rs
    - crates/compat-harness/tests/compat_report.rs
    - crates/cli/tests/headless.rs
    - test/ci-contract.test.ts
    - fixtures/compat/source-authority.v1.json
key-decisions:
  - "npm dev is exactly cargo run -p minimax-cli --locked --, preserving argv forwarding without a TypeScript product route."
  - "Both compatibility and source-authority preflights use one structural package policy that rejects TypeScript or legacy product aliases without executing package scripts."
  - "TypeScript remains permitted only in the existing build, check, test, evaluation, and smoke verification lanes pending Phase 11 retirement."
patterns-established:
  - "Product-entry checks compare the fixed dev, start, and bin routes exactly before inspecting every remaining script for denied aliases."
  - "Transitional TypeScript verification is classified by its test, src/eval, or src/smoke path rather than by a broad file-extension ban."
requirements-completed: [RUST-01, RUST-03]
coverage:
  - id: D1
    description: "npm development and installed entry routes resolve only to the Rust CLI and preserve forwarded arguments."
    requirement: RUST-01
    verification:
      - kind: integration
        ref: "crates/compat-harness/tests/source_authority.rs#repository_product_scripts_are_rust_owned"
        status: pass
      - kind: integration
        ref: "npm run dev -- --version"
        status: pass
    human_judgment: false
  - id: D2
    description: "Both source-authority and compatibility preflights reject direct TypeScript, generated JavaScript, legacy, and equivalent product aliases."
    requirement: RUST-03
    verification:
      - kind: integration
        ref: "crates/compat-harness/tests/source_authority.rs#package_product_script_mutations_fail_closed"
        status: pass
      - kind: integration
        ref: "cargo run -p minimax-compat-harness --locked -- verify-candidate"
        status: pass
    human_judgment: false
  - id: D3
    description: "Existing TypeScript build, check, test, evaluation, and smoke scripts remain available only as transitional verification surfaces."
    requirement: RUST-03
    verification:
      - kind: integration
        ref: "crates/compat-harness/tests/source_authority.rs#transitional_verification_scripts_remain_available"
        status: pass
      - kind: integration
        ref: "npm test"
        status: pass
    human_judgment: false
duration: 20min
completed: 2026-07-17
status: complete
---

# Phase 10 Plan 04: Rust Package Route Closure Summary

**npm development now enters Rust directly, installed startup remains bound to the fixed native launcher, and both offline authority preflights reject any TypeScript or legacy product alias while retaining transitional verification commands.**

## Performance

- **Duration:** 20 min
- **Started:** 2026-07-17T14:20:23Z
- **Completed:** 2026-07-17T14:40:23Z
- **Tasks:** 2
- **Files created/modified:** 8

## Accomplishments

- Replaced the last direct TypeScript development entry with the exact locked Cargo route and proved npm forwards `--version` to the Rust binary.
- Added one fail-closed structural package policy shared by source-authority and compatibility preflights, covering fixed bin/start/dev routes plus direct TS/TSX, generated JS, legacy, and equivalent product aliases.
- Preserved the current TypeScript build, typecheck, test, launcher-test, retrieval/provider evaluation, and smoke commands as explicit transitional verification surfaces.
- Added exact real-repository assertions and table-driven synthetic mutation coverage across Rust and Node contract suites.

## Task Commits

Each task was committed atomically in TDD order:

1. **Task 1 RED: lock Rust-only package routes and rejection matrix** - `896f1bf` (test)
2. **Task 2 GREEN: enforce shared Rust product-route authority** - `49712c7` (feat)

The summary and planning trackers are committed together in the final plan metadata commit.

## Files Created/Modified

- `package.json` - Exact locked Cargo development route with the fixed native launcher retained for installed startup.
- `crates/compat-harness/src/source_authority.rs` - Shared package product-script policy and source-authority integration.
- `crates/compat-harness/src/baseline.rs` - Compatibility preflight reuse of the same structural package policy.
- `crates/compat-harness/tests/source_authority.rs` - Real-repository assertions, transitional positives, and table-driven product-alias rejection cases.
- `crates/compat-harness/tests/compat_report.rs` - Exact compatibility assertions for Cargo development and native installed startup.
- `crates/cli/tests/headless.rs` - Exact npm entry-route contract with legacy and generated aliases excluded.
- `test/ci-contract.test.ts` - Node-side exact package route and transitional script contract.
- `fixtures/compat/source-authority.v1.json` - Reviewed hash update for the changed transitional CI contract test.

## Decisions Made

- Kept `start` on `node bin/minimax-codex.cjs` because that fixed launcher is the installed cross-platform npm shim to the packaged Rust executable; only the development source route changes to direct Cargo.
- Required exact commands for the two supported product routes and inspected package scripts as inert JSON. Authority checks do not execute npm scripts or infer intent from command success.
- Classified TypeScript under `test/`, `src/eval/`, and `src/smoke/` as transitional verification only. Any direct product TS/TSX path, `dist/cli.js`, or legacy CLI alias fails closed.

## Deviations from Plan

None - the plan was executed as written.

## Issues Encountered

- The first synthetic mutation RED reached the source-authority CI requirement before package validation because the synthetic repository lacked the committed workflow fixture. Copying the valid CI fixture isolated the planned failure, after which the mutation matrix failed for the intended package-policy reason.
- The workstation has no MSVC linker. Focused Rust tests, build, strict Clippy, candidate verification, and the development smoke used the installed GNU-LLVM toolchain with its bundled `rust-lld`; repository and release configuration were unchanged.

## Verification

- `cargo fmt --all -- --check` - passed.
- `cargo test -p minimax-compat-harness --test source_authority --locked` - 10 passed.
- Exact `compat_report` package-route test - passed.
- Exact `headless` npm package-route test - passed.
- `npx --no-install tsx --test test/ci-contract.test.ts` - 22 passed.
- `cargo run -p minimax-compat-harness --locked -- verify-candidate` - passed from the committed tree.
- `cargo build -p minimax-cli --locked` - passed.
- `cargo clippy -p minimax-cli -p minimax-compat-harness --all-targets --locked -- -D warnings` - passed.
- `npm test` - 440 passed.
- `npm run dev -- --version` - passed and reported `minimax-codex-rust 0.1.0` through the Cargo route.

## Known Stubs

None. No placeholder or unfinished product entry was introduced.

## User Setup Required

None - all work was local/offline and used no credentials, publication, downloads, Provider calls, hosted workflow runs, or external APIs.

## Next Phase Readiness

- Phase 10 is complete with source inventory, package routes, installed startup, writable state, release candidates, and CI ordering all converged on Rust authority.
- Phase 11 can retire the explicitly retained TypeScript verification and evaluation surfaces without any ambiguity about supported product execution.
- Hosted evidence remains intentionally untouched until its planned refresh after the final stable product fingerprint.

## Self-Check: PASSED

- The canonical summary exists and both TDD commits resolve in git history.
- All focused route, authority, compatibility, Node, formatting, build, strict Clippy, and full local TypeScript gates pass.
- No tracked files were deleted, no hosted evidence was refreshed, and no product script outside the intended `dev` route was removed.

---
*Phase: MMX-10-rust-authority-and-source-boundaries*
*Completed: 2026-07-17*
