---
phase: MMX-05-retrieval-and-project-discovery
verified: 2026-07-16T16:00:00Z
status: passed
score: 8/8 must-haves verified
behavior_unverified: 0
overrides_applied: 0
---

# Phase 5: Retrieval and Project Discovery Verification Report

**Phase Goal:** Non-programmers can describe a need and receive explainable open-source project matches from BM25 candidates, with optional locally verified semantic reranking that cannot remove lexical fallback or introduce un-recalled projects.
**Status:** passed

## Goal Achievement

| # | Observable truth | Status | Evidence |
|---|------------------|--------|----------|
| 1 | Capability, project, and Wiki documents share algorithms but cannot cross typed indexes or snapshots | VERIFIED | Generic marker types plus cross-domain snapshot rejection tests pass. |
| 2 | Exact/BM25 is deterministic, no-match is truthful, and mixed Chinese/English behavior is versioned | VERIFIED | Kernel tests and all 175 inherited TypeScript cases pass. |
| 3 | Ordinary Wiki retrieval indexes current pages only while Vault retains superseded provenance | VERIFIED | Retrieval and CLI current/superseded page tests pass through Vault-owned parsing. |
| 4 | Project facts are strict local catalog evidence and absent license/maintenance remains unknown | VERIFIED | Catalog duplicate/field/URL/fingerprint matrix and renderer tests pass. |
| 5 | BM25 candidates and contribution keywords exist before any embedding call | VERIFIED | Scripted runner records the exact prior lexical candidate order. |
| 6 | Semantic output is candidate-only and hybrid requires resource, helper, vector, dimension, and fingerprint proof | VERIFIED | Outsider, malformed, NaN/dimension, helper, hash, ABI, CPU, and fingerprint tests pass. |
| 7 | Every optional semantic failure preserves the unchanged BM25 order with one stable reason | VERIFIED | Full degradation matrix plus CLI no-resource output pass. |
| 8 | CLI/TUI text and JSONL expose the same actual facts and 10k Wiki p95 remains <=100 ms | VERIFIED | Strict protocol roundtrip, renderer evidence, and recorded 12.391 ms local p95 pass. |

## Requirements Coverage

| Requirement | Status |
|-------------|--------|
| RETR-01 through RETR-06 | VERIFIED |

## Verification Commands

| Gate | Result |
|------|--------|
| `cargo fmt --all -- --check` | PASS |
| `cargo-clippy.exe clippy --workspace --all-targets --locked -- -D warnings` | PASS |
| `cargo test --workspace --locked` | PASS - all workspace tests and doc tests |
| `cargo test -p minimax-retrieval --locked --test benchmark -- --nocapture` | PASS - 10k Wiki p95 12.391 ms |
| `npm run check && npm test && npm run build` | PASS - 432 TypeScript tests |
| `npm run eval:retrieval` | PASS - 175 cases |
| `npm run eval:provider` | PASS - both protocols |
| `npm run verify:rust-contracts` | PASS |
| `check.decision-coverage-verify` | PASS - 22/22 decisions honored |
| `git diff --check` | PASS |

No live Provider request, remote project lookup, credential access, embedding model download, SQLite database, migration, source deletion, PR, merge, or npm entry cutover was used.

## Gaps Summary

No Phase 5 implementation or verification gap remains. A real separately installed Granite resource remains an explicit optional deployment step and is not required for lexical operation or Phase 5 verification.

---
_Verifier: Codex inline GSD verifier fallback; subagents were not authorized._
