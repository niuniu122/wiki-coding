# Phase 9: Capability Workspace and Non-Programmer Harness - Specification

**Created:** 2026-07-17  
**Ambiguity score:** 0.05 (gate: <= 0.20)  
**Requirements:** CAPW-01 through CAPW-08

## Goal

Provide a dedicated, auditable external capability workspace where non-programmers can describe a need and receive explainable project, Skill, or MCP matches from three isolated BM25 indexes with optional candidate-only semantic reranking and truthful readiness guidance.

## Acceptance Criteria

- [ ] `capabilities/catalogs/{projects,skills,mcp}.v1.json` are strict, fingerprinted, source-only catalogs and `capabilities/README.md` explains the source/runtime boundary.
- [ ] One `CapabilityCard` contract represents all three kinds while typed project/Skill/MCP documents make cross-kind index construction fail at compile time.
- [ ] Unknown fields, unsafe URLs, duplicate IDs, invalid fingerprints, executable command strings, control characters, excessive collections, and cross-kind catalog entries are rejected.
- [ ] Exact or BM25 runs before any embedding request; the request contains at most the lexical candidate union and a returned outsider invalidates semantic output.
- [ ] Semantic resource or helper failure preserves lexical order and reports the existing stable degraded reason.
- [ ] An immutable inventory overlay derives exactly three readiness states with installation taking precedence over authorization.
- [ ] The CLI supports workspace status and search across all kinds or one kind, in text or strict JSONL, with no install/execute flag.
- [ ] Each result includes kind, readiness, readiness reason, next action, source/license/platform/permission/authorization facts, and deterministic retrieval evidence.
- [ ] Agent prompt augmentation marks capability evidence read-only and explicitly forbids automatic download, installation, authorization, or execution.
- [ ] Mixed Chinese/English intent, empty Skill/MCP catalogs, unsafe input, candidate isolation, readiness, rendering, and no-side-effect tests pass offline.

## Must Not

- MUST NOT place external project, Skill, or MCP implementations inside `crates/tools`.
- MUST NOT let catalogs contain secrets, local credential values, mutable installed flags, shell commands, or process state.
- MUST NOT run embedding before lexical recall or let semantic output introduce an outsider.
- MUST NOT fetch catalogs, download models, install packages, request authorization, start MCP processes, or call a Provider during discovery.
- MUST NOT claim an unknown license, permission, maintenance state, install method, or authorization is known.
- MUST NOT add a database or external RAG framework to the base product.

## Verification Strategy

Rust unit and integration tests are authoritative for schema validation, typed isolation, BM25-first ordering, readiness derivation, strict serialization, rendering parity, and side-effect absence. A compact labeled fixture covers ordinary Chinese/English non-programmer needs; no LLM judge is required for deterministic fields and rankings.
