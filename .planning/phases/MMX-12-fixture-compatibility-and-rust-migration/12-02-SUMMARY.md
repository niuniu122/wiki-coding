---
phase: 12-fixture-compatibility-and-rust-migration
plan: "02"
subsystem: migration
tags: [rust, migration, fixtures, sha256, retention, recovery, rollback]

requires:
  - phase: 12-fixture-compatibility-and-rust-migration
    provides: Immutable public-contract compatibility and Rust-only candidate verification from Plan 12-01
provides:
  - Exact fingerprinted inventory of all TypeScript-v1 migration evidence with reviewable include/exclude policy
  - Machine-enforced two-subsequent-public-release retention gate after the v3.0.0 cutover
  - Source-preserving Rust migration safety coverage for bounds, secrets, symlinks, drift, recovery, receipts, collisions, idempotency, and narrow rollback
affects: [13-thin-npm-and-native-release, 14-typescript-removal]

tech-stack:
  added: []
  patterns:
    - Self-excluding immutable fixture manifests with exact recursive path, length, SHA-256, role, and disposition validation
    - Release-count retention gates using explicit ordered public versions rather than time or cadence inference
    - Recovery and rollback ownership constrained to exact plan-bound or receipt-allowlisted Rust targets

key-files:
  created:
    - fixtures/compat/migration/typescript-v1/manifest.v1.json
    - fixtures/compat/migration/typescript-v1/support-window.v1.json
    - crates/compat-harness/src/migration_support.rs
    - crates/compat-harness/tests/migration_support.rs
  modified:
    - crates/compat-harness/src/lib.rs
    - crates/compat-harness/src/main.rs
    - crates/cli/src/migration.rs
    - crates/cli/tests/migration.rs

key-decisions:
  - "Exclude manifest.v1.json and support-window.v1.json from their own recursive fingerprint while requiring every source-evidence file exactly once."
  - "Count only distinct, strictly ordered major.minor.patch public releases after 3.0.0; removalEligible must equal the computed two-release result."
  - "Reject all symlinked migration inputs and bind interrupted recovery to the exact plan hash and target metadata."
  - "Limit receipt-created and receipt-reused ownership to unique known Rust migration targets so recomputed forged receipts cannot claim arbitrary project files."

patterns-established:
  - "Migration evidence policy: immutable bytes and include/exclude intent drift together and fail candidate verification together."
  - "Source preservation: every hostile and lifecycle case compares full source/project tree hashes before and after the operation."

requirements-completed: [RCMP-02]

coverage:
  - id: D1
    description: Exact TypeScript-v1 migration evidence inventory rejects missing, added, edited, duplicate, self-referential, or policy-drifted fixture entries.
    requirement: RCMP-02
    verification:
      - kind: integration
        ref: crates/compat-harness/tests/migration_support.rs#fixture_manifest_covers_every_source_evidence_file_exactly_once
        status: pass
      - kind: integration
        ref: crates/compat-harness/tests/migration_support.rs#fixture_manifest_rejects_tamper_missing_extra_duplicate_and_metadata_self_entry
        status: pass
    human_judgment: false
  - id: D2
    description: TypeScript-v1 fixture removal remains ineligible until two distinct ordered public releases after v3.0.0 are explicitly recorded.
    requirement: RCMP-02
    verification:
      - kind: integration
        ref: crates/compat-harness/tests/migration_support.rs#support_window_is_counted_from_distinct_ordered_public_releases_after_v3
        status: pass
      - kind: integration
        ref: crates/compat-harness/tests/migration_support.rs#support_window_rejects_premature_duplicate_pre_v3_unordered_and_non_public_evidence
        status: pass
    human_judgment: false
  - id: D3
    description: Rust migration is bounded, source-preserving, secret-free, idempotent, collision-safe, recoverable, verifiable, and narrowly reversible without legacy execution.
    requirement: RCMP-02
    verification:
      - kind: integration
        ref: cargo test -p minimax-cli --test migration --locked
        status: pass
      - kind: integration
        ref: cargo run -p minimax-compat-harness --locked -- verify-candidate
        status: pass
    human_judgment: false

duration: 23min
completed: 2026-07-17
status: complete
---

# Phase 12 Plan 02: Migration Evidence and Support Window Summary

**Fingerprint-gated TypeScript-v1 evidence and a strict two-release retention record now back a source-preserving Rust migration with bounded recovery and rollback ownership.**

## Performance

- **Duration:** 23 min
- **Started:** 2026-07-17T20:46:21Z
- **Completed:** 2026-07-17T21:09:33Z
- **Tasks:** 2
- **Files modified:** 8

## Accomplishments

- Added a strict manifest for all eight immutable source-evidence files, including exact byte length, SHA-256, role, and expected migration disposition.
- Added a machine-checkable support record that is false at v3.0.0 and accepts eligibility only after two explicit, distinct, ordered public releases.
- Expanded the Rust migration suite to 15 lifecycle and hostile cases, closing symlink, private-reasoning, forged recovery, recomputed receipt, bounds, drift, reused-target, and narrow rollback gaps.

## Task Commits

Each TDD task preserved explicit RED and GREEN evidence:

1. **Task 1 RED: migration fixture and retention gates** - `b7b76ae`
2. **Task 1 GREEN: fingerprinted evidence and release window** - `156a49e`
3. **Task 2 RED: migration bounds and safety cases** - `1ff0a53`
4. **Task 2 RED: recomputed forged receipt case** - `589609f`
5. **Task 2 GREEN: recovery and rollback ownership hardening** - `becf136`
6. **Task 2 regression: source/target drift reporting** - `9478374` (necessary because the prior suite covered apply/rollback drift but did not directly prove that `verify_migration` reports source drift, rejects target drift, and writes nothing)

## Files Created/Modified

- `fixtures/compat/migration/typescript-v1/manifest.v1.json` - Exact self-excluding evidence inventory and provenance fingerprint.
- `fixtures/compat/migration/typescript-v1/support-window.v1.json` - v3.0.0 cutover and two-release eligibility record, currently false.
- `crates/compat-harness/src/migration_support.rs` - Strict recursive fixture, policy, fingerprint, public-release, and computed-eligibility validators.
- `crates/compat-harness/src/lib.rs` - Exports migration support validation APIs and status.
- `crates/compat-harness/src/main.rs` - Runs migration support gates before compatibility release evidence.
- `crates/compat-harness/tests/migration_support.rs` - Positive and table-driven negative fixture/retention cases.
- `crates/cli/src/migration.rs` - Fails symlinks/private reasoning closed and constrains recovery/receipt ownership.
- `crates/cli/tests/migration.rs` - Fifteen deterministic temp-copy migration safety and lifecycle cases.

## Decisions Made

- Fixture metadata is deliberately outside its own recursive content set; its declared fingerprint still binds provenance, metadata exclusions, every evidence row, and every inclusion/exclusion policy.
- Public-release evidence uses strict `major.minor.patch` values only. No elapsed-time guess, prerelease, duplicate, cutover, older, or unordered version can advance eligibility.
- Interrupted operation manifests must match the active plan hash and exact target hashes/lengths before cleanup; receipt targets must be unique known Rust migration paths.

## Deviations from Plan

None - plan executed exactly as written. Production migration code changed only where focused RED tests exposed the safety gaps the plan required it to close.

## Issues Encountered

- The host does not provide the Windows MSVC `link.exe`; Rust builds/tests used the existing isolated `1.97.0-x86_64-pc-windows-gnullvm` plus `rust-lld` development target. This is not Windows MSVC release evidence.
- The unfiltered compatibility report suite passed 22 tests and failed only `hosted_cutover_evidence_matches_current_product`, because the checked-in hosted fingerprint is intentionally stale after product changes. Candidate compatibility passed 22/22, and hosted evidence was not edited or fabricated.

## User Setup Required

None - no external service, credential, network, Provider, or model setup was used.

## Verification

- `cargo fmt --all -- --check` - passed.
- `cargo test -p minimax-cli --test migration --locked` - 15 passed, 0 failed.
- `cargo test -p minimax-compat-harness --test migration_support --locked` - 4 passed, 0 failed.
- Candidate `compat_report` suite - 22 passed, 0 failed, 1 hosted test filtered.
- `cargo run -p minimax-compat-harness --locked -- verify-candidate` - passed.
- `cargo clippy -p minimax-cli --tests --locked -- -D warnings` - passed.
- `cargo build -p minimax-cli --locked` - passed with the documented local development fallback.
- `npm run test:launcher` - 3 passed, 0 failed.

## Next Phase Readiness

- RCMP-02 is locally complete and Phase 13 may consume the source-preserving Rust migration contract.
- TypeScript-v1 fixtures remain mandatory and removal-ineligible. Phase 14 may delete legacy source/tests, but not these migration fixtures before a later milestone records two subsequent public releases.
- Hosted release evidence remains intentionally pending final fingerprint stabilization and fresh authorization.

## Self-Check: PASSED

- All four created artifacts exist and all six Task 12-02 commits are present.
- The checked-in manifest covers all eight source-evidence files exactly once and the support window remains false.
- Final migration, fixture, candidate, formatting, Clippy, CLI build, and launcher checks pass within the documented local evidence boundary.

---
*Phase: 12-fixture-compatibility-and-rust-migration*
*Completed: 2026-07-17*
