---
phase: MMX-04-vault-and-main-model-wiki
plan: "03"
subsystem: vault-maintenance
tags: [rust, vault, lint, rebuild, gc, trash, forget, cli, architecture]
requires:
  - phase: MMX-04-vault-and-main-model-wiki
    provides: immutable raw evidence, strict Wiki transactions, inbox import, and main-model workflow receipts
provides:
  - deterministic read-only Vault lint, narrow repair, and raw-preserving rebuild
  - reference-safe report-first GC with drift rejection, seven-day trash, undo, and reconfirmed purge
  - Wiki-first privacy forget with exact confirmation and non-secret tombstone
  - typed Rust CLI/JSONL maintenance commands and architecture gates
affects: [retrieval, migration, packaging, cutover]
key-files:
  created:
    - crates/vault/src/lint.rs
    - crates/vault/src/rebuild.rs
    - crates/vault/src/gc.rs
    - crates/vault/src/forget.rs
    - crates/cli/tests/vault_commands.rs
    - fixtures/compat/vault/maintenance.v1.json
  modified:
    - crates/cli/src/app.rs
    - crates/cli/src/main.rs
    - crates/compat-harness/src/architecture.rs
key-decisions:
  - "Lint never mutates; repair can only quarantine an incomplete final workflow fragment or roll forward an already validated transaction."
  - "Ordinary GC has no authority over raw evidence and binds apply/purge to exact visible plan hashes."
  - "Forget requires every affected claim to be re-crystallized before sensitive raw bytes are removed."
requirements-completed: [VAULT-02, VAULT-04, VAULT-06, WIKI-04]
duration: 96min
completed: 2026-07-16
status: complete
---

# Phase 4 Plan 3: Vault Maintenance and Retention Summary

**The project Vault now behaves like a mature local data system without becoming a database: damage is diagnosable, compiled knowledge is rebuildable, cleanup is reversible, and privacy deletion has separate authority.**

## Accomplishments

- Added stable issue codes and deterministic read-only lint across manifests, owned paths, raw hashes, strict Wiki pages, provenance, workflow journals, and transaction recovery state.
- Added allowlisted repair and an explicit Provider-free rebuild that replays accepted durable patches through expected-hash transactions and proves every raw hash remains unchanged.
- Added reference-first GC classification. Preview is mutation-free; apply recomputes the exact plan, protects raw/pinned/referenced data, and moves only rebuildable/eligible transient objects into a plan-scoped trash directory.
- Added exact seven-day undo semantics and a separate expired-plan purge confirmation. No force switch exists.
- Added claim inventory and Wiki-first `forget`: all affected pages must be safely re-crystallized before raw removal, and the permanent tombstone excludes the original ID, paths, and content.
- Added top-level Rust Vault commands, JSONL output, TUI routing guidance, compatibility fixtures, and gates preventing Vault-to-Provider/database edges or CLI/TUI Markdown parsing.

## Task Commits

1. **Lint, repair, and rebuild** - `e2b18b5`
2. **GC, trash, purge, and forget** - `09625f7`
3. **CLI/TUI and architecture evidence** - `66247e5`

## Verification

- Rust formatting, workspace Clippy with warnings denied, all 181 executed Rust tests/doc tests, 432 TypeScript tests, TypeScript check/build, retrieval/provider evals, Rust contract verification, and `git diff --check` passed.
- Retention fixtures prove report-only behavior, drift rejection, protected raw evidence, undo at the exact expiry instant, purge only after expiry and reconfirmation, and Wiki-first raw deletion.
- `package.json` still points to `dist/cli.js`; no live Provider, credential, model download, SQLite, migration, PR, merge, or product cutover occurred.

## Self-Check: PASSED

---
*Phase: MMX-04-vault-and-main-model-wiki*
*Plan: 04-03 completed 2026-07-16*
