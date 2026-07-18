# Phase 14: TypeScript Removal and Hosted Closure - Specification

**Created:** 2026-07-17
**Ambiguity score:** 0.08 (gate: <= 0.20)
**Requirements:** RCUT-01, RCUT-02, RCUT-03

## Goal

Delete the replaced TypeScript implementation and close v3 only after the Rust product, evaluations, compatibility, migration, npm/native packaging, and hosted Windows/Linux evidence all pass from the final tree.

## Requirements

1. Remove `.ts/.tsx` product/test source, `tsconfig.json`, legacy build/test/eval scripts, TypeScript/tsx/React/Ink dependencies, `dist/`, and stale references.
2. Retain static JSON/JSONL migration/compatibility fixtures and permitted thin CJS/MJS release files.
3. Make CI/release commands Rust-authoritative and add a zero-TS/no-fallback source gate.
4. Update user/maintainer documentation for Rust-only architecture, npm/native install, supported platforms, failure messages, migration/rollback, and support window.
5. Refresh hosted Windows x64 MSVC and Linux x64 GNU evidence against one final fingerprint; stale or development-only evidence is rejected.

## Boundaries

**In scope:** source/config/dependency deletion, reference cleanup, final CI/docs, product fingerprint, hosted candidate/strict evidence, milestone closeout.

**Out of scope:** deleting migration fixtures, ending the support window, publishing npm, adding platforms/features, real user-data migration.

## Acceptance Criteria

- [ ] Repository scan finds zero `.ts/.tsx` product/test files and zero TypeScript compiler/runtime/UI dependencies.
- [ ] All Rust, evaluation, compatibility, migration, release, package, security, license, and performance gates pass after deletion.
- [ ] npm/native installed smoke passes from the final package with no legacy path.
- [ ] Docs contain the Rust-only/npx/native/migration/rollback/support-window contract.
- [ ] Hosted Windows/Linux evidence matches the final fingerprint and exact artifact hashes.

## Edge Coverage

| Edge | Resolution | Coverage |
|------|------------|----------|
| deletion leaves stale source import/path | zero-TS/reference scan and full gates fail | covered |
| static migration fixture name contains “typescript” | allow data fixtures, not executable source | covered |
| local Windows lacks MSVC linker | do not fabricate; hosted MSVC job is mandatory | covered |
| hosted evidence predates final deletion | fingerprint mismatch blocks closeout | covered |

## Prohibitions

| Must-NOT statement | Status | Verification |
|--------------------|--------|--------------|
| MUST NOT delete migration fixtures or support metadata | resolved | retained-file assertions |
| MUST NOT close on stale/local development evidence | resolved | host/fingerprint/hash checks |
| MUST NOT publish, push, or open a PR without fresh approval | resolved | explicit manual checkpoint |
| MUST NOT combine unrelated feature expansion with cleanup | resolved | diff and roadmap scope audit |

## Verification Strategy

Run the complete local Rust/release matrix, repository scans, GSD consistency checks, and diff audits. Hosted candidate and strict runs are explicit manual checkpoints requiring fresh authorization and exact evidence verification.
