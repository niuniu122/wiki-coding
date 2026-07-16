---
phase: MMX-06-migration-release-and-cutover
plan: "01"
subsystem: transactional-migration
tags: [rust, migration, receipts, rollback, source-preservation, security]
requires:
  - phase: MMX-05-retrieval-and-project-discovery
    provides: complete Rust runtime, Vault, and capability target schemas
provides:
  - deterministic read-only TypeScript inventory and dry-run plans
  - allowlisted secret-safe normalization into replay-valid Rust session journals
  - plan-bound staged apply with immutable receipts and idempotence
  - target-hash verification and created-only rollback
affects: [release-gates, cutover, support-docs]
key-files:
  created:
    - crates/cli/src/migration.rs
    - crates/cli/tests/migration.rs
    - fixtures/compat/migration/typescript-v1/config.json
  modified:
    - crates/cli/src/app.rs
    - crates/cli/src/main.rs
key-decisions:
  - "Inventory and dry-run are write-free; apply and rollback require exact evidence hashes."
  - "Only safe config, sessions, visible messages, tool history, and capability metadata migrate; secrets, private traces, summaries, caches, locks, and unknown paths remain excluded."
  - "Source bytes are never changed; collisions and drift fail closed; rollback removes only unchanged targets created by the receipt."
requirements-completed: [MIGR-01, MIGR-02, MIGR-03]
completed: 2026-07-16
status: complete
---

# Phase 6 Plan 1: Transactional Migration Summary

**Rust now has an explicit, auditable migration workflow that can import supported TypeScript state without changing or deleting the source.**

## Accomplishments

- Replaced the unavailable migrate route with `inventory`, `dry-run`, `apply`, `verify`, and `rollback` commands and stable JSON/text evidence.
- Added deterministic recursive source hashing, path/type/size bounds, strict known-schema parsing, symlink exclusion, target collision reporting, and target hashes before writes.
- Normalized safe TypeScript configuration, threads, turns, visible user/assistant messages, completed tool history, and capability snapshots into strict Rust files; generated journals replay through `SessionMachine` before publication.
- Excluded credential files/fields, secret-looking records, private traces, summaries, indexes/caches, locks, databases, and unsupported paths.
- Added exact plan confirmation, source drift revalidation, staged publication, interrupted-operation recovery, immutable created/reused receipts, repeat idempotence, target verification, and receipt-bound rollback that preserves reused or changed files.
- Added a complete adversarial fixture and seven transaction tests covering determinism, zero-write dry-run, secret exclusion, replay, idempotence, collision, drift, forgery, recovery, changed-target refusal, and source-preserving rollback.

## Task Commit

- **Safe transactional migration and fixtures** - `789a3ab`

## Verification

- `cargo-clippy.exe clippy --workspace --all-targets --locked -- -D warnings` passed.
- `cargo test --workspace --locked` passed, including 7 migration transaction tests.
- Manual JSON dry-run reported exact included/excluded files, three normalized targets, hashes, plan ID, and confirmation without writing.
- No real user data, Provider call, credential, model download, database, source deletion, npm cutover, PR, or merge was used.

## Self-Check: PASSED

---
*Phase: MMX-06-migration-release-and-cutover*
*Plan: 06-01 completed 2026-07-16*
