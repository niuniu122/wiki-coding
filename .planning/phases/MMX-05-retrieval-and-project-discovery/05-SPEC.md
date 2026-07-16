# Phase 5: Retrieval and Project Discovery - Specification

**Created:** 2026-07-16
**Ambiguity score:** 0.03 (gate: <= 0.20)
**Requirements:** RETR-01 through RETR-06

## Goal

Provide one offline-first Rust retrieval subsystem whose three typed indexes cannot contaminate one another, preserve exact/BM25 capability behavior, and help non-programmers find explainable open-source projects through BM25 candidates followed only optionally by verified semantic reranking.

## Requirements

1. **RETR-01 - isolated shared engine:** Capability, project, and Wiki document types share exact/BM25/vector/RRF algorithms but have distinct index IDs, snapshot schemas, and result types.
2. **RETR-02 - lexical baseline:** Exact + BM25 works with no semantic resource, handles mixed Chinese/English deterministically, returns truthful no-match, and passes the current 175-case TypeScript capability fixture gates.
3. **RETR-03 - BM25-first project workflow:** Natural-language project discovery exposes BM25 keywords/candidates before an embedding adapter receives only that candidate set; reranking cannot introduce another project.
4. **RETR-04 - concrete verified embedding:** A separately installed local Granite resource/helper validates identity, version, hashes, license, vector dimensions/finiteness, fingerprint, CPU/platform health, and bounded helper execution before activation.
5. **RETR-05 - truthful mode:** Every response reports `exact+bm25`, `hybrid_verified`, or one explicit degradation reason based on runtime proof, not configuration intent.
6. **RETR-06 - explainable projects:** Results carry catalog source, repository URL, license, maintenance signals, actual mode, matched keywords, and a deterministic explanation; unknown facts remain unknown.

## Acceptance criteria

- [ ] Generic compile-time isolation and strict snapshot tests prevent cross-domain documents/results.
- [ ] Exact precedence, BM25 scores/ties, Chinese/English tokens, RRF, empty/no-match, repeat, and 175-case parity tests pass.
- [ ] A recording embedding port proves BM25 completes first and receives no non-candidate project.
- [ ] Missing/corrupt/incompatible/stale/slow/crashed/wrong-vector resource cases all preserve useful lexical results with stable reasons.
- [ ] A resource flag alone never reports hybrid; verified helper health plus matching model/vector fingerprint is mandatory.
- [ ] Wiki ordinary search returns current pages only and retains source/page identifiers.
- [ ] CLI text and JSONL contain identical stable facts and do not execute discovered projects.
- [ ] A 10,000-page BM25 benchmark records p95 <= 100 ms on the declared local environment.
- [ ] Rust/TypeScript/offline eval/architecture/compatibility/diff gates pass without Provider/network/model download.
- [ ] `package.json` remains `dist/cli.js`.

## Must not

- MUST NOT run embedding before BM25 project candidate recall.
- MUST NOT let semantic ranking add a project outside the BM25 candidate set.
- MUST NOT mix capability/project/Wiki schemas or index namespaces.
- MUST NOT invent license, popularity, maintenance, compatibility, or security facts.
- MUST NOT bundle/download a model, contact a remote project service, read credentials, or call a Provider in default tests.
- MUST NOT treat a feature flag, manifest alone, or stale vector file as verified hybrid availability.

## Verification strategy

Code-based deterministic metrics are authoritative: fixture recall/top-1/MRR/no-match/id validity, candidate-subset assertions, strict manifest/vector checks, stable serialization, failure matrices, command rendering equality, and recorded latency. No LLM judge is required because the ranked IDs and explanations are fully rule-derived.
