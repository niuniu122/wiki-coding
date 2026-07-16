# Phase 5 Context: Retrieval and Project Discovery

## User outcome

A non-programmer describes what they want to accomplish and gets understandable open-source project choices. The visible order is fixed: BM25 first identifies useful keywords and candidate projects; an optional, locally verified embedding resource may then rerank only that bounded candidate set.

## Locked decisions

- **D-501:** One generic Rust retrieval engine implements normalization, exact matching, BM25, cosine ranking, and reciprocal-rank fusion, while capability, project, and Wiki documents remain distinct Rust types and distinct index identities.
- **D-502:** A domain mismatch is impossible through the public typed API and is also rejected when loading serialized snapshots.
- **D-503:** Mixed Chinese/English normalization is versioned and deterministic. Han runs emit the full run, individual characters, and adjacent bigrams; Latin identifiers retain useful separators and sub-tokens.
- **D-504:** Exact identity/alias/command matches win. Otherwise BM25 1.2/0.75 produces a stable score order with document ID as the final tie-break.
- **D-505:** Empty/stopword-only/no-overlap queries return no match rather than manufacturing a low-confidence result.
- **D-506:** Capability lexical behavior is measured against the existing TypeScript 175-case fixture; the Rust engine must meet the same recall/top-1/no-match gates before Phase 5 closes.
- **D-507:** Project discovery always runs BM25 before any embedding call. The embedding input contains only the query and BM25 candidate IDs/documents; a test port records and enforces that ordering.
- **D-508:** BM25 exposes contribution-derived keywords, not an LLM-generated interpretation. Explanations name matched terms and stable catalog facts.
- **D-509:** The initial project catalog is a strict local snapshot with source URL, repository URL, license, description, topics, supported platforms, last activity, release, and maintenance signals. Refresh/network acquisition is a separate adapter boundary, never required for offline search.
- **D-510:** Catalog source facts are not model claims. Missing license or maintenance facts remain `unknown` and lower confidence; they are never guessed.
- **D-511:** Embedding is optional and separately installed. No weights, runtime, dynamic library, or vector bundle is included in the base executable or downloaded automatically.
- **D-512:** The accepted semantic resource is the existing Granite multilingual qint8 x64-AVX2 contract. Its package ID, model ID, revision, runtime ABI, architecture, quantization, license, tokenizer version, file hashes, dimensions, document fingerprint, and platform health must all verify.
- **D-513:** A fixed-argument local helper-process adapter is the concrete embedding boundary. It receives bounded JSON on stdin, has no shell/network/credential input, and is killed on its deadline.
- **D-514:** Hybrid mode is true only after resource verification, helper health, finite/dimension-valid query vectors, and a vector-index fingerprint matching the current catalog/index.
- **D-515:** Missing, corrupt, incompatible, stale, malformed, non-finite, wrong-dimension, timed-out, or crashed embedding resources return BM25 results plus one stable degraded reason.
- **D-516:** RRF fuses BM25 order with semantic order only inside the BM25 candidate set. Semantic ranking cannot introduce a project BM25 did not recall.
- **D-517:** Wiki retrieval indexes only `status: current` pages for ordinary search. Superseded pages remain available to provenance/audit APIs but never ordinary results.
- **D-518:** Capability, project, and Wiki snapshots use expected hashes and complete immutable publication. A failed refresh retains the last-known-good snapshot and reports staleness.
- **D-519:** CLI text and JSONL show the same query, keywords, mode, degraded reason, project source/license/maintenance facts, and per-result explanation.
- **D-520:** `/capabilities` becomes available in Phase 5. Project search is additionally exposed as an explicit `index projects search` workflow and never starts a tool execution automatically.
- **D-521:** All retrieval and embedding evaluation is offline and deterministic. No live Provider, remote project search, credential, model download, or API spend is used.
- **D-522:** The npm product entry remains `dist/cli.js`; release/cutover remains Phase 6.

## Boundaries

In scope: shared lexical/vector primitives, three typed indexes, strict local project catalog, BM25-first discovery workflow, verified optional embedding helper/resource, truthful status/explanations, CLI/TUI surfaces, parity and performance fixtures.

Out of scope: automatic internet refresh, arbitrary repository crawling, bundled weights, GPU support, executing a discovered repository, migration, packaging/cutover, PR, or merge.

## Prior architecture influence

The design follows the earlier Codex/claw-code comparison: thin UI/composition surfaces, typed events and receipts, deterministic fixtures, and replaceable adapters. Retrieval owns ranking; CLI owns composition; Vault owns Wiki parsing; no second persistence authority is introduced.
