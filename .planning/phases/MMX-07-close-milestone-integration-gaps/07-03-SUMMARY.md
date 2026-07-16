---
phase: MMX-07-close-milestone-integration-gaps
plan: "03"
subsystem: distribution-evidence
tags: [rust, npm, release, fingerprint, legacy]
requires:
  - phase: MMX-06-migration-release-and-cutover
    provides: fixed Rust launcher, legacy entry, and native release gates
provides:
  - deterministic platform npm package containing the Rust default and explicit TypeScript legacy entry
  - extracted-artifact smoke verification for the actual packaged Rust launcher
  - product fingerprint that invalidates stale hosted evidence after any tracked product change
  - Rust-default installation, rollback, architecture, Vault, and discovery documentation
affects: [release, compat-harness, cli, documentation, ci]
key-decisions:
  - "One platform npm artifact contains the exact verified native binary and launcher plus the explicit built TypeScript legacy entry."
  - "The product fingerprint covers tracked and untracked non-ignored product files, excluding only planning records and its circular hosted evidence record."
  - "Release verification starts the extracted packaged Rust default instead of relying on launcher unit substitution."
requirements-completed: [COMP-04, REL-01, REL-03, REL-04]
completed: 2026-07-16
status: complete
---

# Phase 7 Plan 3: Complete Distribution and Evidence Binding Summary

The release output now includes a deterministic platform npm package in which `minimax-codex` starts the exact sibling Rust binary and `minimax-codex-legacy` reaches the built TypeScript fallback. The verifier checks hashes, strict contents, both bin mappings, extracts the package, and starts the real packaged Rust command successfully.

## Verification

- Deterministic base and npm artifacts passed package tests, strict no-source/no-model-resource inspection, sidecar and manifest checks.
- The extracted Rust default returned the expected typed capability status; the legacy bin mapping resolved to `dist/cli.js`.
- Product-fingerprint tests prove product edits change the digest while planning and hosted-evidence-only edits do not.
- The Rust compatibility harness independently reproduced the 401-file fingerprint and accepted the machine-readable gate.
- TypeScript check/build, launcher tests, Rust release build, package verification, and targeted GNU-LLVM Clippy passed.

No package was published; no tag, PR, merge, Provider call, credential access, model download, SQLite use, source deletion, or user-data migration occurred.
