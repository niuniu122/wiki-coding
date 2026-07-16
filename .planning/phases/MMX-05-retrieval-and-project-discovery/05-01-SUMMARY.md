---
phase: MMX-05-retrieval-and-project-discovery
plan: "01"
subsystem: typed-retrieval-kernel
tags: [rust, retrieval, bm25, tokenizer, snapshots, compatibility]
requires:
  - phase: MMX-04-vault-and-main-model-wiki
    provides: strict current/superseded Wiki page records
provides:
  - schema-isolated capability, project, and Wiki lexical indexes
  - deterministic mixed Chinese/English exact and BM25 retrieval
  - candidate-bounded cosine and reciprocal-rank fusion primitives
  - expected-hash immutable typed index snapshots
affects: [project-discovery, cli, tui, migration, packaging]
key-files:
  created:
    - crates/protocol/src/retrieval.rs
    - crates/retrieval/src/domain.rs
    - crates/retrieval/src/bm25.rs
    - crates/retrieval/src/snapshot.rs
    - crates/retrieval/tests/lexical.rs
  modified:
    - crates/retrieval/src/lib.rs
key-decisions:
  - "Domain marker types prevent capability, project, and Wiki documents or snapshots from crossing public APIs."
  - "Exact identity wins; otherwise stable BM25 1.2/0.75 requires a meaningful term and never emits low-confidence single-character noise."
  - "Ordinary Wiki indexes accept current pages only; typed snapshots validate domain, tokenizer, and document hash before use."
requirements-completed: [RETR-01, RETR-02]
completed: 2026-07-16
status: complete
---

# Phase 5 Plan 1: Typed Retrieval Kernel Summary

**Rust now owns a deterministic, dependency-light retrieval kernel whose three data domains share ranking code without sharing types or snapshots.**

## Accomplishments

- Added strict retrieval protocol records, stable runtime modes, semantic degradation codes, and bounded explanation fields.
- Added a versioned mixed Chinese/English tokenizer, exact alias/command precedence, BM25 term contributions, finite cosine similarity, and stable reciprocal-rank fusion.
- Added separate capability, project, and current-Wiki document types with generic indexes that cannot be mixed at compile time.
- Added immutable expected-hash snapshot publication; loading validates schema, domain, tokenizer version, and document content hash before deserializing into the selected typed domain.
- Ported the existing TypeScript 175-case capability fixture into the Rust test gate and passed every case, including all no-match cases.

## Task Commit

- **Typed retrieval contracts, kernel, snapshots, and parity evidence** - `81db379`

## Verification

- `cargo test -p minimax-protocol --locked` passed.
- `cargo test -p minimax-retrieval --locked` passed, including all 175 inherited retrieval cases.
- `cargo-clippy.exe clippy -p minimax-retrieval --all-targets --locked -- -D warnings` passed.
- No database, network path, model resource, credential, or product-entry change was added.

## Self-Check: PASSED

---
*Phase: MMX-05-retrieval-and-project-discovery*
*Plan: 05-01 completed 2026-07-16*
