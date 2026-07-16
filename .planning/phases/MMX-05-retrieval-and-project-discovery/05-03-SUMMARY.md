---
phase: MMX-05-retrieval-and-project-discovery
plan: "03"
subsystem: retrieval-surfaces
tags: [rust, cli, tui, jsonl, wiki, compatibility, benchmark]
requires:
  - phase: MMX-05-retrieval-and-project-discovery
    plan: "02"
    provides: strict catalog and candidate-only optional semantic reranking
provides:
  - explicit capability, project, and current-Wiki status/search commands
  - identical typed text and JSONL search facts with deterministic explanations
  - available interactive /capabilities search
  - offline compatibility, architecture, and 10k-Wiki performance evidence
affects: [migration, packaging, cutover, release]
key-files:
  created:
    - crates/cli/src/index.rs
    - crates/cli/tests/index_commands.rs
    - crates/retrieval/tests/benchmark.rs
    - fixtures/compat/retrieval/explanations.v1.json
  modified:
    - crates/cli/src/app.rs
    - crates/cli/src/main.rs
    - crates/tui/src/command.rs
    - crates/tui/src/render.rs
    - crates/compat-harness/src/baseline.rs
key-decisions:
  - "Every search is an explicit read-only command; a discovered project is never executed or installed automatically."
  - "Text is rendered from the same strict protocol record serialized as JSONL, so actual mode, degradation, facts, and explanation cannot diverge."
  - "A valid resource manifest is not enough to claim hybrid; hybrid appears only after a successful helper response passes vector and fingerprint validation."
requirements-completed: [RETR-05, RETR-06]
completed: 2026-07-16
status: complete
---

# Phase 5 Plan 3: Retrieval Surfaces Summary

**Users can now inspect and search Rust capabilities, local project catalogs, and current Wiki knowledge through explicit commands that show exactly what evidence and retrieval mode produced each result.**

## Accomplishments

- Added `index capabilities status/search`, `index projects status/search`, and `index wiki status/search`; project and Wiki routes are read-only and accept no execution switch.
- Made `/capabilities` available in the interactive Rust shell, using the same built-in capability index and renderer as the top-level command.
- Added a no-mutation read-only Vault open path so Wiki parsing remains inside the Vault crate while search does not take writer authority or create files.
- Added strict result records and bounded rendering for query, contribution keywords, actual mode, degradation reason, source/repository, truthful unknown license/activity/release/maintenance, ranks, and scores.
- Added retrieval compatibility and architecture gates that reject network/database/credential/model-download paths and retain the TypeScript npm entry.
- Added a 10,000-current-page Wiki BM25 benchmark; recorded local p95 was 12.391 ms against the 100 ms release threshold.

## Task Commit

- **CLI/TUI retrieval surfaces, evidence, and benchmark** - `cb4baed`

## Verification

- Workspace Rust format, Clippy with warnings denied, all tests/doc tests, and compatibility contract verification passed.
- 432 TypeScript tests, TypeScript check/build, 175-case retrieval evaluation, and both Provider protocol evaluations passed.
- Manual Rust commands returned the expected capability and BM25-first project results with explicit `embedding_missing` degradation and unknown catalog facts.
- `package.json` remains unchanged at `dist/cli.js`; no live Provider/network/model resource or automatic project execution was used.

## Self-Check: PASSED

---
*Phase: MMX-05-retrieval-and-project-discovery*
*Plan: 05-03 completed 2026-07-16*
