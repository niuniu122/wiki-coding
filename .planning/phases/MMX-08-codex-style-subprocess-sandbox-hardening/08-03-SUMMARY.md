---
phase: MMX-08-codex-style-subprocess-sandbox-hardening
plan: "03"
subsystem: sandbox-diagnostics-and-release-gates
tags: [rust, doctor, cli, ci, release, documentation]
requires:
  - phase: MMX-08-codex-style-subprocess-sandbox-hardening
    provides: Linux enforced sandbox and adversarial security evidence
provides:
  - truthful sandbox status and remediation in doctor and permission surfaces
  - strict hosted release evidence bound to the current product fingerprint
  - manual-only candidate evidence refresh with ordinary push/PR strictness
  - cross-platform release and milestone regression closure
affects: [cli, tui, docs, ci, compat, release]
key-decisions:
  - "Candidate mode may skip only stale hosted-attestation comparison and is unreachable from push or pull_request events."
  - "Windows and unsupported platforms report confirm-mode process execution as fail-closed rather than implying an advisory sandbox."
  - "Hosted evidence is accepted only after a second ordinary strict push validates the refreshed fingerprint."
requirements-completed: [SBOX-01, SBOX-05, SBOX-06, SBOX-07, REL-01, REL-03, REL-04]
completed: 2026-07-17
status: complete
---

# Phase 8 Plan 3: Diagnostics, Documentation, and Release Gates Summary

Doctor, permission/help text, security documentation, release guidance, and CI now distinguish user approval from operating-system isolation and report enforced, disabled-by-full-access, unsupported, missing, and failed-backend states without overstating safety.

The hosted evidence workflow has two deliberately narrow stages. A manually dispatched candidate run executes every product, security, package, performance, and offline gate while skipping only comparison with stale evidence. Its machine-readable Windows/Linux artifacts are then committed, after which an ordinary push must pass the strict comparison and full milestone flow.

Candidate run `29553147648` produced fingerprint `12e41e7384a4474e8e1ed53ccb8942fd7992a6b7b0585a1ab537406b9c74cce4` for 406 product files. Strict run `29553650069` passed on Windows job `87801243529` and Ubuntu job `87801243532`, including release packaging, performance/security budgets, and milestone-flow verification.

## Verification

- Rust formatting and hosted Clippy/all-target checks passed.
- TypeScript checks, 440 tests, and build passed locally; hosted strict Rust tests/contracts passed on both native platforms.
- Retrieval and Provider evaluations, release packaging, installed-command smoke, and milestone-flow gates passed.
- No live Provider call, credential read, embedding download, SQLite use, publication, tag, or real migration occurred.
