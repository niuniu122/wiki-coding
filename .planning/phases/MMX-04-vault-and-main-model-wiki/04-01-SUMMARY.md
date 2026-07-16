---
phase: MMX-04-vault-and-main-model-wiki
plan: "01"
subsystem: vault-foundation
tags: [rust, obsidian, vault, raw-evidence, sha256, frontmatter, transaction, recovery]
requires:
  - phase: MMX-03-safe-tool-completion
    provides: append-synced session journal, writer lease, typed recovery, and safe content boundaries
provides:
  - strict Vault, evidence, Wiki page, patch, transaction, workflow, and receipt contracts
  - idempotent project-bound fixed Obsidian-compatible Vault bootstrap
  - immutable SHA-256 terminal session evidence with final-fragment quarantine
  - deterministic strict Wiki frontmatter and expected-hash transaction roll-forward
affects: [inbox-import, wiki-workflow, maintenance, retrieval, migration]
tech-stack:
  added: [sha2, tempfile]
  patterns: [fixed ownership tree, content-addressed evidence, prepared manifest, expected-hash roll-forward]
key-files:
  created:
    - crates/protocol/src/knowledge.rs
    - crates/protocol/src/vault.rs
    - crates/vault/src/bootstrap.rs
    - crates/vault/src/raw.rs
    - crates/vault/src/page.rs
    - crates/vault/src/transaction.rs
  modified:
    - crates/vault/src/runtime/mod.rs
    - crates/vault/src/runtime/lease.rs
key-decisions:
  - "The manifest, not AGENTS.md, is the project binding contract; bootstrap never edits Git policy."
  - "Finalized session metadata contains hashes and counts while events.jsonl remains the sole raw event-body copy."
  - "All changed targets are prevalidated before page-index-log roll-forward, preserving external edits without blind rollback."
patterns-established:
  - "Vault mutations use one non-blocking writer lease, sibling temporary files, sync, and atomic publication."
  - "Recovery may continue only a strict prepared manifest whose staging bytes and target old/new hashes all validate."
requirements-completed: [VAULT-01, VAULT-02, VAULT-03, VAULT-04]
requirements-progressed: [WIKI-03, WIKI-04]
coverage:
  - id: D1
    description: Project binding, fixed layout, warnings, ownership, and competing-writer behavior are executable and idempotent.
    requirement: VAULT-01
    verification:
      - kind: integration
        ref: "crates/vault/tests/vault_bootstrap.rs"
        status: pass
    human_judgment: false
  - id: D2
    description: Terminal runtime events freeze into immutable evidence and only a final incomplete fragment is repairable.
    requirement: VAULT-03
    verification:
      - kind: integration
        ref: "crates/vault/tests/raw_evidence.rs"
        status: pass
    human_judgment: false
  - id: D3
    description: Strict pages and prepared transactions converge after interruption while stale edits and tampered staging replace nothing.
    requirement: VAULT-04
    verification:
      - kind: integration
        ref: "crates/vault/tests/wiki_transaction.rs"
        status: pass
    human_judgment: false
duration: 38min
completed: 2026-07-16
status: complete
---

# Phase 4 Plan 1: Vault Foundation Summary

**Rust now binds one project to one readable Obsidian-compatible Vault, freezes terminal session evidence, and commits deterministic Wiki files through crash-recoverable expected-hash transactions.**

## Accomplishments

- Added strict bounded schema-v1 contracts for project IDs, SHA-256 hashes, evidence, Wiki pages, source citations, knowledge operations, workflow events/usage, transaction manifests, and receipts.
- Created the fixed Vault tree with a machine manifest, human inbox/guidance, explicit ownership, sibling-path recommendation, plaintext/Git warnings, and the existing non-blocking writer-lease discipline.
- Promoted complete runtime sessions to immutable `session.json` metadata plus one `events.jsonl` body, rejected later appends and sensitive/private-reasoning markers, and quarantined only an incomplete final fragment.
- Added deterministic Obsidian frontmatter rendering/parsing and page-index-log transactions that prevalidate every target/staging hash and roll forward idempotently after injected crashes.

## Task Commits

1. **Strict Vault and knowledge contracts** - `b7faa1f`
2. **Vault bootstrap and raw finalization** - `d6fb2cc`
3. **Deterministic Wiki pages and transactions** - `791b896`

## Decisions and Deviations

- Used already-pinned `sha2` and `tempfile`; no database, ORM, RAG framework, Provider SDK, or Markdown database was added.
- Stored runtime finalization markers beside the Phase 2 journal so every later append fails even if the Vault is a sibling directory.
- Hashed session and transaction IDs for internal directory names, avoiding unsafe path use while retaining the stable original IDs inside strict metadata.
- No scope deviation, live Provider request, model download, migration, deletion, product-entry change, PR, or merge occurred.

## Verification

- Focused protocol/Vault bootstrap, raw evidence, page, and transaction tests passed.
- Rust workspace formatting, Clippy with `-D warnings`, all workspace tests, doc tests, and `git diff --check` passed.
- Crash, retry, duplicate, Unicode, secret, future-schema, tail-corruption, middle-corruption, competing-writer, in-Git, stale-edit, and tampered-staging cases are covered.

## Next Plan Readiness

- Plan 04-02 can now import exact inbox bytes, evaluate durable value locally, ask the source session's pinned main model for a structured patch, and commit it through the new transaction boundary.
- The TypeScript npm entry remains `dist/cli.js`; cutover is still reserved for Phase 6 gates.

## Self-Check: PASSED

---
*Phase: MMX-04-vault-and-main-model-wiki*
*Plan: 04-01 completed 2026-07-16*
