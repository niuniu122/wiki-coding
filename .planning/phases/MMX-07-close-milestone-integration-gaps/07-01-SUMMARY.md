---
phase: MMX-07-close-milestone-integration-gaps
plan: "01"
subsystem: runtime-vault-wiki-lifecycle
tags: [rust, vault, wiki, provider, lifecycle, recovery]
requires:
  - phase: MMX-06-migration-release-and-cutover
    provides: complete runtime and isolated Vault/Wiki components
provides:
  - stable project-to-sibling-Vault binding with explicit first-binding override
  - writer-lease-safe immutable runtime finalization
  - production pinned-main-model Wiki generation adapter and bounded input builder
  - end-to-end runtime-to-Wiki-to-current-retrieval tests
affects: [runtime, cli, vault, provider, wiki, retrieval]
key-decisions:
  - "A finalized active session is never reopened for mutation; the next process creates a new active session."
  - "The local durability gate can emit no-op without a Provider call; durable evidence uses the exact finalized session binding and reports separate usage."
  - "Only bounded visible messages and current Wiki excerpts enter the synthesis request; tool output and private reasoning do not."
requirements-completed: [VAULT-01, VAULT-03, WIKI-01, WIKI-02, WIKI-03, WIKI-04, RUN-02]
completed: 2026-07-16
status: complete
---

# Phase 7 Plan 1: Runtime/Vault/Wiki Lifecycle Summary

Runtime sessions now cross the previously missing product boundary: terminal evidence finalizes under the existing runtime writer lease, enters the deterministic durability gate, optionally calls the same pinned main-model Provider through a separate structured request, passes core validation, commits through the Vault writer, and becomes searchable current Wiki knowledge.

## Verification

- `cargo test -p minimax-cli --locked --test lifecycle_wiki` — 2 complete chain tests passed.
- Direct official GNU-LLVM Clippy for `minimax-cli` and `minimax-vault` with all targets and warnings denied passed.
- The synthesized test proves separate usage and exact binding; the lookup-only test proves a no-op receipt with zero Wiki model calls.
- Reopening after finalization creates a new session instead of mutating immutable evidence.

No live Provider call, credential access, model download, database, real-data migration, publication, or source deletion occurred.
