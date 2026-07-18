---
phase: MMX-09-capability-workspace-and-non-programmer-harness
plan: "01"
subsystem: capability-catalog-and-retrieval
tags: [rust, retrieval, bm25, embedding, catalog]
provides:
  - dedicated source-only project, Skill, and MCP catalogs
  - one strict capability-card contract and three typed indexes
  - bounded BM25 candidate union with optional verified semantic reranking
affects: [protocol, retrieval, capability-workspace]
key-decisions:
  - "External metadata remains source-only under capabilities/ and never enters crates/tools."
  - "BM25 is authoritative recall; semantic output cannot introduce an outsider."
requirements-completed: [CAPW-01, CAPW-02, CAPW-03, CAPW-04]
completed: 2026-07-17
status: complete
---

# Phase 9 Plan 1: Typed Capability Workspace Summary

Added fingerprinted project, Skill, and MCP catalogs under `capabilities/`, one strict `CapabilityCard` schema, and compile-time-separated document/index types. Search performs exact/BM25 independently per kind, merges at most 20 lexical candidates deterministically, and permits an optional verified embedding helper to rerank only that union.

Strict validation rejects unknown fields, kind mismatches, duplicates, invalid IDs/URLs/fingerprints, unsafe install guidance, and malformed facts. Semantic helper failures or outsider IDs preserve the BM25 result and stable degraded reason.

## Verification

- Capability workspace retrieval integration tests passed.
- Mixed-language fixture, kind isolation, no-match, readiness, and semantic outsider cases passed.
- Retrieval architecture remained offline and Provider-free.
