---
phase: 12-fixture-compatibility-and-rust-migration
plan: "03"
subsystem: compatibility
tags: [rust, module-closure, source-inventory, typescript-cutover, hermetic-verification]

requires:
  - phase: 12-01-fixture-owned-compatibility
    provides: Fixture-owned deterministic Rust compatibility reports and the original no-TypeScript boundary
provides:
  - Exact compatibility-harness module closure derived from lib.rs and main.rs
  - Recursive regular Rust source inventory whose set must equal the derived closure
  - Structural executable-edge scanner covering source reads, includes, direct processes, shell wrappers, and constant concatenation
affects: [12-04-migration-gap-closure, 13-thin-npm-and-native-release, 14-typescript-removal]

tech-stack:
  added: []
  patterns:
    - Root-derived Rust module resolution with exact recursive source-set equality
    - Token-aware executable-edge classification that leaves static authority literals inert

key-files:
  created: []
  modified:
    - crates/compat-harness/src/report.rs
    - crates/compat-harness/tests/compat_report.rs

key-decisions:
  - "Treat lib.rs and main.rs as the only fixed Rust roots, then derive every external local module instead of maintaining another source allowlist."
  - "Require the recursively inventoried regular src/**/*.rs set and the derived module set to be exactly equal in both directions."
  - "Reject legacy paths and runtimes only when they flow through executable read, include, process, or shell contexts; static fixture and authority text remains evidence."

patterns-established:
  - "Closure integrity: unresolved, ambiguous, duplicate, orphaned, symlinked, non-regular, unsafe, or unreadable module paths fail closed with deterministic repository-relative diagnostics."
  - "Executable legacy classification: direct and constant-built Node/npm/npx/tsc processes, shell-mediated builds, and TypeScript product reads/includes fail in every reached module."

requirements-completed: [RCMP-01]

coverage:
  - id: D1
    description: The compatibility gate derives the complete lib.rs/main.rs module closure and proves exact equality with every regular Rust source under compat-harness/src.
    requirement: RCMP-01
    verification:
      - kind: integration
        ref: crates/compat-harness/tests/compat_report.rs#compatibility_source_boundary_rejects_forbidden_references_in_derived_module_closure
        status: pass
      - kind: integration
        ref: cargo test -p minimax-compat-harness --test compat_report --locked -- --skip hosted_cutover_evidence_matches_current_product
        status: pass
    human_judgment: false
  - id: D2
    description: Every derived module rejects executable TypeScript source and Node/npm/npx/tsc process edges while static authority and fixture literals remain inert.
    requirement: RCMP-01
    verification:
      - kind: integration
        ref: crates/compat-harness/tests/compat_report.rs#compatibility_source_boundary_rejects_forbidden_references_in_derived_module_closure
        status: pass
      - kind: integration
        ref: cargo run -p minimax-compat-harness --locked -- verify-candidate
        status: pass
    human_judgment: false

duration: 29min
completed: 2026-07-17
status: complete
---

# Phase 12 Plan 03: Close the Compatibility Source Boundary Summary

**Compatibility now derives all eleven compiled Rust source modules from standard crate roots, requires exact recursive inventory equality, and rejects executable legacy edges anywhere in that closure.**

## Performance

- **Duration:** 29 min
- **Started:** 2026-07-17T21:41:00Z
- **Completed:** 2026-07-17T22:10:04Z
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments

- Replaced the four-file security allowlist with standard Rust module resolution rooted only at `lib.rs` and `main.rs`, covering all eleven current compatibility-harness sources.
- Added an independent recursive `src/**/*.rs` inventory and exact set equality so unreachable/orphaned additions and unresolved/ambiguous/duplicate/symlinked paths fail closed.
- Added executable-context classification for direct and constant-built legacy processes, shell-mediated commands, and TypeScript product reads/includes while keeping static authority literals legal.

## Task Commits

Each TDD gate was committed atomically:

1. **Task 1 RED: Expose omitted reachable modules and incomplete closure discovery** - `11cf82b` (test)
2. **Task 2 GREEN: Derive and inspect the exact compat-harness module closure** - `2ccb651` (feat)

## Files Created/Modified

- `crates/compat-harness/src/report.rs` - Root-derived module resolver, recursive regular-source inventory, exact set gate, and structural executable legacy-reference classifier.
- `crates/compat-harness/tests/compat_report.rs` - Formerly omitted module mutations, closure-integrity adversaries, recursive nested-module coverage, and static-literal control.

## Decisions Made

- The only named roots are Rust's standard `lib.rs` and `main.rs`; every other file is discovered through module declarations and checked against the filesystem inventory.
- Module discovery and source inventory are separate computations whose exact equality is the forcing function against both missing and extra files.
- Static historical names are not authority violations by themselves. A violation requires an executable include/read/process/build edge.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Distinguished Rust lifetimes from character literals in the source tokenizer**
- **Found during:** Task 2 focused GREEN control
- **Issue:** The first implementation parsed `'static` in `architecture.rs` as a malformed character literal, causing the unchanged control repository to fail before boundary evaluation.
- **Fix:** Classified apostrophe-prefixed identifiers without an immediate closing quote as lifetimes while retaining ordinary character-literal validation.
- **Files modified:** `crates/compat-harness/src/report.rs`
- **Verification:** The unchanged control, focused closure test, and full candidate compatibility suite all passed.
- **Committed in:** `2ccb651`

---

**Total deviations:** 1 auto-fixed (1 correctness bug).
**Impact on plan:** The fix was required for the scanner to accept existing valid Rust source; it did not expand scope or weaken legacy-edge detection.

## Issues Encountered

- The unfiltered compatibility suite passed 23 tests and failed only `hosted_cutover_evidence_matches_current_product` with the known stale `CutoverEvidence` fingerprint. The candidate suite passed 23/23 with that Phase 14-only hosted assertion filtered. No hosted record was edited or represented as current.
- The installed MSVC linker is unavailable, so Rust commands used the existing `1.97.0-x86_64-pc-windows-gnullvm` plus `rust-lld` fallback and an isolated target directory. This is local development evidence only.
- The injected Node/TypeScript-shaped mutations were read as inert temporary-copy text and were never compiled or executed. The existing thin npm launcher test in the full compatibility suite continued to exercise only its established JavaScript launcher probes, not the injected legacy payloads or a TypeScript product.

## User Setup Required

None - no external service configuration required.

## Verification

- RED focused test compiled, ran, and failed at `provider_eval.rs` returning `Ok(())`, proving the old four-file scan omitted the reachable module.
- Focused GREEN closure/adversarial test - 1 passed.
- Original report source/process negative test - 1 passed after expressing each case as an executable-shaped edge.
- Candidate compatibility suite - 23 passed, 0 failed, 1 hosted test filtered.
- Unfiltered compatibility suite - 23 passed, 1 known hosted-fingerprint failure deferred to Phase 14.
- `cargo run -p minimax-compat-harness --locked -- report --format json` - passed with 34 entries and contract version v1.
- `cargo run -p minimax-compat-harness --locked -- verify-candidate` - passed.
- `cargo fmt --all -- --check` and `git diff --check` - passed.
- `cargo clippy -p minimax-compat-harness --all-targets --locked -- -D warnings` - passed.
- No Provider request, credential read, network access, model download, package publication, push, PR, or hosted verification occurred.

## Next Phase Readiness

- RCMP-01's complete compatibility source boundary is ready for independent re-verification.
- Plan 12-04 remains responsible for migration receipt ownership and target-ancestor symlink gaps; this plan did not enter or modify that scope.

## Self-Check: PASSED

- Modified source and test files exist.
- RED/GREEN commits `11cf82b` and `2ccb651` exist on `codex/rust-convergence-v3`.
- Focused closure, candidate compatibility, deterministic report, candidate preflight, formatting, diff hygiene, and strict Clippy checks pass.
- Working tree was clean before plan metadata creation.

---
*Phase: 12-fixture-compatibility-and-rust-migration*
*Completed: 2026-07-17*
