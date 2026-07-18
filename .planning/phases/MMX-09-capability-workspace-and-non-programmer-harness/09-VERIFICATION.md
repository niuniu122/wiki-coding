---
phase: MMX-09-capability-workspace-and-non-programmer-harness
verified: 2026-07-17
status: passed_local
hosted_release_evidence: pending_refresh
---

# Phase 9 Verification

## Result

All Phase 9 functional and local mandatory gates passed. The capability workspace is physically separate from internal executable tools, uses three typed BM25-first indexes, limits optional embedding to the lexical candidate union, and exposes truthful readiness without install, authorization, process, network, credential, or Provider authority.

## Functional Evidence

| Gate | Result |
|---|---|
| Strict fingerprinted project/Skill/MCP catalogs | Passed |
| Compile-time typed index isolation | Passed |
| BM25-first bounded candidate union | Passed |
| Embedding outsider and failure fallback | Passed |
| Install-before-access readiness precedence | Passed |
| Plain text and strict JSONL parity | Passed |
| Prompt evidence is bounded and read-only | Passed |
| Forbidden install/authorize/execute/start CLI flags | Passed |
| Mixed Chinese/English fixture and no-match behavior | Passed |
| Offline retrieval architecture boundary | Passed |

## Local Evidence

- `cargo fmt --all -- --check` — passed.
- Workspace Clippy with `-D warnings` — passed under the installed Windows GNU-LLVM toolchain and bundled `rust-lld`.
- `cargo test --workspace --locked -- --skip hosted_cutover_evidence_matches_current_product` — passed, including doc tests.
- `cargo run -p minimax-compat-harness --locked -- verify-candidate` — passed.
- `npm run check` — passed.
- `npm test` — 440/440 passed.
- `npm run build` — passed.
- `npm run eval:retrieval` — 175 cases; lexical and fused Recall@5/Top-1/MRR 1.0; no-match precision and ID validity 1.0; passed.
- `npm run eval:provider` — both Provider protocol suites passed.
- Current pre-hosted product fingerprint — `f599aa324e135d30db744d86c497d67196d5d170d469aaa03941aed64d0a74f7` across 414 product files.
- `git diff --check` — passed before the implementation commits.
- The fallback GNU-LLVM release build and archive creation passed. `verify:rust-release` could not validate the installed Windows package because the non-MSVC binary exited `0xC0000135` (missing runtime); this artifact is not accepted as hosted Windows MSVC evidence.
- `verify:milestone-flow` passed its cross-phase Rust chain under GNU-LLVM and then stopped at the intentionally absent current release-evidence record.

## Hosted Evidence Boundary

The strict hosted-evidence comparison is expected to fail until its deterministic product fingerprint is refreshed. The existing fixture belongs to the Phase 8 main tree and was intentionally left unchanged. This machine also lacks the MSVC linker/runtime needed to substitute for the hosted Windows job. Before merge or release, manually dispatch the candidate CI matrix, bind the resulting Windows/Linux artifacts to the new fingerprint, commit that record, and require a subsequent ordinary strict push to pass.

No push, pull request, publication, tag, live Provider request, credential read, model download, capability installation, authorization request, MCP process start, or real user-data migration was performed.
