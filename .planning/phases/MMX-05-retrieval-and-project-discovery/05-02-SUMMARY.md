---
phase: MMX-05-retrieval-and-project-discovery
plan: "02"
subsystem: project-discovery
tags: [rust, bm25, embedding, granite, catalog, offline, helper-process]
requires:
  - phase: MMX-05-retrieval-and-project-discovery
    plan: "01"
    provides: typed deterministic lexical indexes and candidate-bounded fusion
provides:
  - strict source-backed local project catalog with truthful unknown facts
  - separately installed Granite resource verification and fixed helper ABI
  - BM25-first candidate-only optional semantic reranking
  - stable lexical fallback for every resource and runtime failure
affects: [cli, tui, compatibility, packaging, release]
key-files:
  created:
    - crates/retrieval/src/catalog.rs
    - crates/retrieval/src/embedding.rs
    - crates/retrieval/src/discovery.rs
    - crates/retrieval/tests/project_discovery.rs
    - crates/retrieval/tests/embedding_resource.rs
    - fixtures/compat/retrieval/projects.v1.json
key-decisions:
  - "Project facts come only from a strict hash-bound local snapshot; absent license, activity, release, and maintenance data remains unknown."
  - "Embedding is a separately installed resource validated by model/package identity, ABI, CPU, hashes, dimensions, catalog fingerprint, vector fingerprint, and health."
  - "The helper receives only the query and BM25 candidates; malformed, failed, or timed-out semantics returns the unchanged lexical order with one stable reason."
requirements-completed: [RETR-03, RETR-04, RETR-05]
completed: 2026-07-16
status: complete
---

# Phase 5 Plan 2: Project Discovery Summary

**The non-programmer project finder is now an explicit Rust workflow: lexical evidence is always produced first, and a locally installed embedding helper may only reorder those candidates after complete verification.**

## Accomplishments

- Added a strict schema-v1 local catalog with HTTPS source/repository evidence, stable content fingerprint, duplicate and drift rejection, and no inferred facts.
- Added Granite multilingual qint8 x64-AVX2 resource validation without creating directories or downloading files. Every listed file hash, helper path, ABI, CPU capability, dimensions, catalog fingerprint, vector fingerprint, and platform-health field must pass.
- Added a fixed-executable, fixed-argument, clean-environment helper process with bounded JSON input/output, no shell, a 150 ms default deadline, and kill-on-timeout behavior.
- Added candidate-only semantic requests, finite/dimension/fingerprint output validation, cosine ranking, and RRF that cannot introduce any project outside the BM25 set.
- Added deterministic tests for strict catalog rejection, exact candidate ordering, outsider-vector rejection, resource failures, hash drift, missing resources, and unchanged lexical fallback.

## Task Commit

- **Strict catalog, embedding resource/helper boundary, and staged discovery** - `589785f`

## Verification

- `cargo test -p minimax-retrieval --locked` passed all lexical, catalog, resource, and discovery tests.
- `cargo-clippy.exe clippy -p minimax-retrieval --all-targets --locked -- -D warnings` passed.
- Tests use only tiny fixture bytes and scripted vectors; no live network, Provider, credential, model download, or API spend occurred.

## Self-Check: PASSED

---
*Phase: MMX-05-retrieval-and-project-discovery*
*Plan: 05-02 completed 2026-07-16*
