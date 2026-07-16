# Phase 6: Migration, Release, and Cutover - Specification

**Created:** 2026-07-16
**Ambiguity score:** 0.04 (gate: <= 0.20)
**Requirements:** MIGR-01 through MIGR-03, REL-01 through REL-04

## Goal

Finish the Rust rewrite with a source-preserving migration workflow and release evidence strong enough to make Rust the default command while retaining an explicit TypeScript fallback.

## Requirements

1. **MIGR-01 - pre-write truth:** Inventory and dry-run report supported inputs, exclusions, collisions, normalized target schema/paths, source hashes, target hashes, and a deterministic plan hash without writing.
2. **MIGR-02 - safe import:** Apply imports only safe config, sessions, turns, visible messages, bounded tool events, and capability metadata; secrets, private reasoning, traces, and derived data never enter the target.
3. **MIGR-03 - recovery and rollback:** Apply is drift-checked, idempotent, staged, receipt-backed, target-hash verified, source-preserving, and rollback removes only unchanged files created by that receipt.
4. **REL-01 - installable artifacts:** Windows/Linux base archives are versioned, checksum-covered, embedding-free, and accompanied by install/upgrade/rollback instructions.
5. **REL-02 - enforced footprint:** Recorded gates prove cold start <= 500 ms, idle RSS <= 150 MB, compressed base <= 50 MB, and 10k Wiki BM25 p95 <= 100 ms.
6. **REL-03 - offline release confidence:** Unit, contract, parity, recovery, security, migration, packaging, license, performance, and cross-platform CI require no real credential, Provider request, model download, or spend.
7. **REL-04 - evidence-gated cutover:** The Rust launcher becomes `minimax-codex` only after mandatory gates are green; the TypeScript entry remains `minimax-codex-legacy`, source data remains untouched, and rollback is documented.

## Acceptance criteria

- [x] Inventory/dry-run produce byte-stable JSON for unchanged fixture input and perform zero writes.
- [x] Fixture migration imports safe config, sessions/messages/tool events/capability metadata while an adversarial corpus proves secret/private/derived exclusions.
- [x] Source tree hash is unchanged after apply, verify, repeated apply, failed apply, and rollback.
- [x] Collision, source drift, crash residue, changed target, forged plan/receipt, symlink, oversized input, malformed record, and path escape fail closed.
- [x] The generated Rust journal replays successfully and receipts verify every target hash.
- [x] Windows/Linux packaging scripts produce versioned embedding-free archives, manifests, and SHA-256 files from release binaries.
- [x] Security/architecture/license gates and the four performance budgets are executable and recorded.
- [x] Hosted Windows MSVC and Linux CI run the complete offline release gate.
- [x] Compatibility evidence has no pending mandatory product item before cutover.
- [x] `package.json` points `minimax-codex` to the fixed Rust launcher and retains `minimax-codex-legacy` -> `dist/cli.js`.

## Must not

- MUST NOT mutate, rename, truncate, or delete the TypeScript source tree.
- MUST NOT import secrets, raw/private reasoning, traces, summaries, caches, indexes, databases, or arbitrary unknown files.
- MUST NOT overwrite a differing target or remove a reused/changed target during rollback.
- MUST NOT bundle/download embedding resources or call a network/Provider path in tests or packaging.
- MUST NOT claim Windows support only from the local GNU-LLVM fallback.
- MUST NOT publish, tag, merge, delete TypeScript source, or migrate real user data as part of verification.

## Verification strategy

The authoritative evidence is deterministic and executable: migration fixture hashes and replay, adversarial exclusion/collision/recovery matrices, package inspection and checksums, dependency/license/security checks, release-binary timing and RSS sampling, the existing 10k retrieval benchmark, complete offline test suites, compatibility reports, and hosted Windows/Linux CI.
