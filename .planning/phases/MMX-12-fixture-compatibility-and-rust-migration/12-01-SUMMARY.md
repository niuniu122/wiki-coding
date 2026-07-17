---
phase: 12-fixture-compatibility-and-rust-migration
plan: "01"
subsystem: compatibility
tags: [rust, fixtures, compatibility, golden, typescript-cutover]

requires:
  - phase: 11-rust-verification-and-evaluation-authority
    provides: Rust-owned Provider, retrieval, coverage, and package verification authority
provides:
  - Immutable fingerprinted public-contract fixture with exact command, Provider, permission, tool, migration, retrieval, Vault, and release identities
  - Deterministic contract-versus-Rust report with explicit approved-difference objects and no live TypeScript rows
  - Hermetic compatibility verification that runs Rust architecture and evaluation gates without a TypeScript product tree
affects: [12-02-rust-migration-support, 13-thin-npm-and-native-release, 14-typescript-removal]

tech-stack:
  added: []
  patterns:
    - Fingerprinted immutable contract manifests with exact required-ID sets
    - Stable approved-difference IDs joined fail-closed into deterministic Rust reports
    - Compatibility source boundary rejecting TypeScript imports, builds, and process execution

key-files:
  created:
    - fixtures/compat/public-contract.v1.json
  modified:
    - crates/compat-harness/src/manifest.rs
    - crates/compat-harness/src/report.rs
    - crates/compat-harness/src/baseline.rs
    - crates/compat-harness/tests/compat_report.rs
    - fixtures/compat/command-differences.v1.json
    - fixtures/compat/report.expected.json

key-decisions:
  - "Use contract.* IDs as the only compatibility identities; historical TypeScript appears only as immutable provenance and migration data."
  - "Join approved differences by stable difference.command.* IDs and reject any missing, extra, duplicate, or outcome-drifted entry."
  - "Keep the real verify-candidate preflight for transitional source authority while exposing a fixture-only Rust verification core that does not need src/, dist/, or test/."

patterns-established:
  - "Contract completeness: requiredItemIds, items, commands, profiles, and protocols form an exact fail-closed set."
  - "Report authority: contractVersion and contractFingerprint bind every byte-stable report to its immutable input."

requirements-completed: [RCMP-01]

coverage:
  - id: D1
    description: Immutable public-contract manifest replaces the executable-language baseline and rejects schema, identity, fingerprint, and evidence drift.
    requirement: RCMP-01
    verification:
      - kind: integration
        ref: crates/compat-harness/tests/compat_report.rs#compat_report_contains_every_contract_item_exactly_once
        status: pass
      - kind: integration
        ref: crates/compat-harness/tests/compat_report.rs#public_contract_manifest_fails_closed_on_schema_identity_and_evidence_drift
        status: pass
    human_judgment: false
  - id: D2
    description: Deterministic compatibility reports contain every contract and Rust evidence row exactly once with only explicitly approved differences.
    requirement: RCMP-01
    verification:
      - kind: integration
        ref: crates/compat-harness/tests/compat_report.rs#compat_report_matches_golden_and_is_byte_identical_on_second_run
        status: pass
      - kind: integration
        ref: cargo run -p minimax-compat-harness --locked -- report --format json
        status: pass
    human_judgment: false
  - id: D3
    description: Compatibility verification runs Rust architecture and evaluation gates without importing, building, or executing the transitional TypeScript runtime.
    requirement: RCMP-01
    verification:
      - kind: integration
        ref: crates/compat-harness/tests/compat_report.rs#compatibility_report_and_verify_are_hermetic_without_typescript_runtime
        status: pass
      - kind: integration
        ref: crates/compat-harness/tests/compat_report.rs#compatibility_rejects_unknown_differences_live_rows_and_typescript_execution_links
        status: pass
      - kind: integration
        ref: cargo run -p minimax-compat-harness --locked -- verify-candidate
        status: pass
    human_judgment: false

duration: 22min
completed: 2026-07-17
status: complete
---

# Phase 12 Plan 01: Fixture-Owned Compatibility Summary

**A fingerprinted `contract.*` manifest now drives byte-stable Rust compatibility reports and a hermetic no-TypeScript verification path.**

## Performance

- **Duration:** 22 min
- **Started:** 2026-07-17T20:10:09Z
- **Completed:** 2026-07-17T20:32:07Z
- **Tasks:** 2
- **Files modified:** 14

## Accomplishments

- Replaced `baseline-status.v1.json` and its live `typescript.*`/`rust.*` identities with one immutable, fingerprinted public-contract manifest containing 34 exact stable contracts.
- Rebuilt report composition around `contractVersion`, `contractFingerprint`, sorted Rust evidence, and five explicit approved-difference objects.
- Proved the complete Rust compatibility core works in a copied repository with no top-level `src/`, `dist/`, or `test/` tree and rejects TypeScript import, build, and process links.

## Task Commits

Each TDD task was committed with explicit RED and GREEN gates:

1. **Task 1 RED: public-contract coverage test** - `4b90ed4`
2. **Task 1 GREEN: immutable public-contract manifest** - `f1a1914`
3. **Task 2 RED: Rust-only report and hermetic verification tests** - `6f22d88`
4. **Task 2 GREEN: deterministic fixture-owned reports** - `3346425`

## Files Created/Modified

- `fixtures/compat/public-contract.v1.json` - Immutable v1 contract, provenance, fingerprint, required IDs, Rust evidence, and difference links.
- `fixtures/compat/baseline-status.v1.json` - Deleted after all runtime and test consumers moved.
- `crates/compat-harness/src/manifest.rs` - Strict manifest parsing, identity/evidence completeness, and fingerprint validation.
- `crates/compat-harness/src/report.rs` - Difference joining, report validation, no-TypeScript source boundary, and fixture-only Rust verification core.
- `crates/compat-harness/src/baseline.rs` - Contract-based tool/cutover validation and stable difference-link checks.
- `crates/compat-harness/tests/compat_report.rs` - Golden, negative, and hermetic no-TypeScript coverage.
- `fixtures/compat/report.expected.json` - New contract-versus-Rust golden.
- `test/rust-rewrite-compat-manifest.test.ts` - Retained static transitional evidence updated to validate the new immutable contract.

## Decisions Made

- Contract identities are language-neutral `contract.*` IDs; Rust ownership is expressed by validated evidence, not by a second product namespace.
- Difference rationale remains in one strict fixture and enters reports only through a stable ID link from an approved contract item.
- The hermetic verifier owns Rust compatibility, architecture, Provider, and retrieval gates; the real repository command additionally keeps Phase 11 transitional source-authority checks until Phase 14.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Migrated the retained transitional TypeScript fixture validator and its hash pins**
- **Found during:** Task 1
- **Issue:** `test/rust-rewrite-compat-manifest.test.ts` still directly consumed the deleted baseline fixture, while Phase 11 correctly hash-pinned that retained evidence.
- **Fix:** Pointed the static test at the public contract, strengthened its contract assertions, and updated the two reviewable SHA-256 pins. No TypeScript product behavior was added or executed by compatibility.
- **Files modified:** `test/rust-rewrite-compat-manifest.test.ts`, `fixtures/compat/source-authority.v1.json`, `fixtures/compat/verification/typescript-responsibilities.v1.json`
- **Verification:** Focused transitional test passed 4/4; `verify-candidate` passed.
- **Committed in:** `f1a1914`

---

**Total deviations:** 1 auto-fixed (1 blocking consumer migration).
**Impact on plan:** Required to delete the old fixture without breaking retained Phase 14 evidence; no scope or authority expansion.

## Issues Encountered

- The host lacks the MSVC `link.exe`. Rust compilation and tests used the existing isolated development-only `1.97.0-x86_64-pc-windows-gnullvm` plus `rust-lld` target; this is not represented as Windows MSVC release evidence.
- The unfiltered 23-test compatibility suite passed 22 tests and failed only `hosted_cutover_evidence_matches_current_product`, because this plan intentionally changes the product fingerprint. The candidate suite passed 22/22. Hosted evidence was not edited or fabricated and remains for the final authorized hosted closure.

## User Setup Required

None - no external service configuration required.

## Verification

- `cargo fmt --all -- --check` - passed.
- Focused public-contract and fail-closed tests - passed.
- Golden/determinism and negative TypeScript dependency tests - passed.
- Hermetic no-TypeScript Rust verification - passed.
- Candidate compatibility suite - 22 passed, 0 failed, 1 hosted test filtered.
- `cargo run -p minimax-compat-harness --locked -- report --format json` - passed with no live `typescript.*` rows.
- `cargo run -p minimax-compat-harness --locked -- verify-candidate` - passed.
- `cargo build -p minimax-cli --locked` - passed with the documented local development fallback.
- `npm run test:launcher` - 3 passed.

## Next Phase Readiness

- Fixture-owned compatibility is ready for Plan 12-02's source-preserving migration support-window work.
- Hosted release evidence remains intentionally stale until final fingerprint stabilization and fresh authorization.

## Self-Check: PASSED

- Created public-contract fixture exists and the retired baseline fixture is absent.
- RED/GREEN commits `4b90ed4`, `f1a1914`, `6f22d88`, and `3346425` exist.
- Plan acceptance tests, candidate verification, CLI build, and launcher smoke pass.

---
*Phase: 12-fixture-compatibility-and-rust-migration*
*Completed: 2026-07-17*
