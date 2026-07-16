---
phase: MMX-07-close-milestone-integration-gaps
plan: "02"
subsystem: discovery-command-contracts
tags: [rust, bm25, project-discovery, cli, compatibility]
requires:
  - phase: MMX-07-close-milestone-integration-gaps
    plan: "01"
    provides: complete runtime lifecycle and Wiki product chain
provides:
  - embedded strict project catalog with optional path override
  - automatic read-only BM25-first discovery context for natural-language agent needs
  - product-reachable durable turn retry
  - machine-readable tested command differences
affects: [cli, tui, retrieval, compat-harness, release]
key-decisions:
  - "Automatic discovery augments only explicit open-source/project/tool intents and never installs or runs a result."
  - "The embedded catalog is the zero-configuration default; a strict user catalog remains an explicit override."
  - "Commands that intentionally differ for binding or safety reasons are recorded as explicit differences instead of unqualified behavioral parity."
requirements-completed: [COMP-01, COMP-04, RUN-02, CLI-01, RETR-03]
completed: 2026-07-16
status: complete
---

# Phase 7 Plan 2: Discovery and Command Contracts Summary

The non-programmer project finder is product-reachable again: ordinary agent requests for an open-source project or command-line tool receive bounded read-only evidence from the embedded catalog, with BM25 candidate recall before any optional verified embedding rerank.

## Verification

- Bundled discovery and prompt augmentation tests passed without a catalog path, embedding resource, network call, or installation side effect.
- Index command tests prove the default catalog and strict override both parse and return identical typed facts.
- Restart tests prove the latest terminal turn is retryable while unknown tool side effects remain protected by existing recovery rules.
- Compatibility tests validate the explicit command-difference fixture and all 19 harness tests pass.
- Direct GNU-LLVM Clippy with warnings denied passed for CLI and compatibility targets.

No project was installed or executed; no live Provider, remote lookup, credential, model download, database, publication, or destructive operation occurred.
