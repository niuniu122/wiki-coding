---
phase: MMX-06-migration-release-and-cutover
plan: "02"
subsystem: release-gates
tags: [rust, release, packaging, ci, security, licenses, performance]
requires:
  - phase: MMX-06-migration-release-and-cutover
    plan: "01"
    provides: source-preserving migration and recovery evidence
provides:
  - deterministic embedding-free Windows and Linux base archives
  - strict checksum, manifest, license, security, and package inspection
  - enforced cold-start, idle-RSS, compressed-size, and 10k-Wiki budgets
  - green hosted Windows MSVC and Linux GNU offline release gates
affects: [parity, launcher, cutover, support-docs]
key-files:
  created:
    - scripts/release/package-rust.mjs
    - scripts/release/verify-rust-release.mjs
    - fixtures/compat/release/hosted-gates.v1.json
    - docs/release/install-upgrade-rollback.md
  modified:
    - .github/workflows/ci.yml
    - package.json
    - test/ci-contract.ts
key-decisions:
  - "Only hosted Windows MSVC and Linux GNU evidence can authorize cutover; local GNU-LLVM evidence remains development-only."
  - "The base archive contains no embedding model and every release gate is offline, credential-free, and Provider-free."
  - "Packaging writes only under target and the verifier parses the archive itself rather than trusting external tar output."
requirements-completed: [REL-01, REL-02, REL-03]
completed: 2026-07-16
status: complete
---

# Phase 6 Plan 2: Release Gates Summary

**Supported Windows and Linux release artifacts now pass the same strict offline package, license, security, and performance gate locally and in hosted native CI.**

## Accomplishments

- Added deterministic versioned tar/gzip archives, SHA-256 sidecars, strict release manifests, both project licenses, and explicit supported/development-only platform tiers.
- Added archive/path/hash inspection, Cargo dependency-license policy, workspace-wide unsafe-code and database rejection, and migration network/credential/download isolation.
- Enforced 500 ms cold-start, 150 MiB idle RSS, 50 MiB compressed-base, and 100 ms 10k-Wiki BM25 p95 limits with recorded environments and raw samples.
- Extended the exact read-only CI matrix so Windows and Linux must build, package, and verify native release binaries after all TypeScript, Rust, compatibility, and offline evaluation gates.
- Fixed Windows CI sampling by collecting five readings in one PowerShell process while retaining the original memory limit and sample count.

## Task Commits

- **Offline native release gates** - `c976d02`
- **Portable Clippy invocation** - `15d885a`
- **Stable Windows RSS sampling** - `66c8567`

## Verification

- Hosted run `29474558013` passed on Windows x64 MSVC and Linux x64 GNU from tree `b4c19d5f776850808d138cf51a694789eb67be38`.
- Windows: 4,000,125-byte archive, 27.905 ms cold-start p95, 7,213,056-byte maximum idle RSS, and 1.366 ms Wiki p95.
- Linux: 4,745,674-byte archive, 4.113 ms cold-start p95, 5,664,768-byte maximum idle RSS, and 2.099 ms Wiki p95.
- Both jobs checked 234 dependency packages and reported zero invalid licenses, unsafe files, database packages, migration network/credential paths, credentials, Provider calls, or model downloads.
- No package publication, tag, PR, merge, TypeScript deletion, real-data migration, or embedding download occurred.

## Self-Check: PASSED

---
*Phase: MMX-06-migration-release-and-cutover*
*Plan: 06-02 completed 2026-07-16*
