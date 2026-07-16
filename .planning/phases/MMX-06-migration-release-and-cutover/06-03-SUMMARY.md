---
phase: MMX-06-migration-release-and-cutover
plan: "03"
subsystem: cutover
tags: [rust, launcher, parity, legacy, rollback, hosted-ci]
requires:
  - phase: MMX-06-migration-release-and-cutover
    plan: "02"
    provides: green native release prerequisites and recorded budgets
provides:
  - fixed shell-free Rust default launcher for Windows x64 and Linux x64
  - explicit minimax-codex-legacy TypeScript support entry
  - zero-pending executable Rust compatibility evidence
  - final green hosted cutover matrix and rollback/support documentation
affects: [default-entry, distribution, support, milestone-audit]
key-files:
  created:
    - bin/minimax-codex.cjs
    - test/launcher.test.ts
    - docs/release/cutover.md
  modified:
    - package.json
    - crates/compat-harness/src/baseline.rs
    - fixtures/compat/baseline-status.v1.json
    - scripts/release/package-rust.mjs
key-decisions:
  - "The launcher accepts only fixed Windows x64 and Linux x64 sibling binary names and has no shell, download, environment override, or silent fallback."
  - "TypeScript remains explicit as minimax-codex-legacy throughout v0.1 and for at least 90 days after the first published Rust-default build."
  - "Cutover requires an executable green release record and rejects every pending Rust compatibility item."
requirements-completed: [REL-04]
completed: 2026-07-16
status: complete
---

# Phase 6 Plan 3: Evidence-Gated Cutover Summary

**Rust is now the default product entry, with a fixed fail-closed launcher, an explicit TypeScript legacy command, complete parity evidence, and green final Windows/Linux release gates.**

## Accomplishments

- Added a minimal CommonJS launcher that forwards exact argv through `spawnSync` with `shell: false`, rejects unsupported platforms and unsafe/missing/non-executable artifacts, and never searches, downloads, reads credentials, or silently falls back.
- Changed `minimax-codex` to the Rust launcher and retained `minimax-codex-legacy` -> `dist/cli.js`; npm package contents are allowlisted to release/runtime files instead of source and planning material.
- Added the launcher and its SHA-256 to every base archive and manifest while retaining one native binary and no embedding resource.
- Closed the three Rust Provider-profile entries with executable offline config mapping, added migration/release/retrieval/Vault/product evidence, and made cutover fail if any Rust item remains pending.
- Documented fresh install, migration, verify, upgrade, binary/data rollback, source retention, distribution boundaries, and the legacy support/removal rule.

## Task Commits

- **Fixed Rust launcher and compatibility cutover** - `7d62a53`
- **Install, cutover, and rollback documentation** - `e19cee0`

## Verification

- Local gates passed: 437 TypeScript tests, TypeScript check/build, launcher tests, direct official GNU-LLVM Clippy, all Rust workspace tests/doc tests, compatibility verification, 175-case retrieval eval, both Provider evals, deterministic package inspection, licenses, security, and release budgets.
- Final hosted run `29476499926` passed Windows x64 MSVC and Linux x64 GNU from exact tree `1f8d46812465755a59b45a426b4e93596d21adc5`.
- Windows final artifact: 4,000,935 bytes, 21.659 ms cold-start p95, 7,217,152-byte maximum idle RSS, and 0.655 ms Wiki p95.
- Linux final artifact: 4,746,736 bytes, 3.099 ms cold-start p95, 5,861,376-byte maximum idle RSS, and 1.152 ms Wiki p95.
- Both final jobs checked 234 packages and recorded zero invalid licenses, unsafe/database/migration-network paths, credentials, Provider calls, or model downloads.
- No publication, tag, PR, merge, TypeScript deletion, real-data migration, embedding download, Provider spend, or SQLite use occurred.

## Self-Check: PASSED

---
*Phase: MMX-06-migration-release-and-cutover*
*Plan: 06-03 completed 2026-07-16*
