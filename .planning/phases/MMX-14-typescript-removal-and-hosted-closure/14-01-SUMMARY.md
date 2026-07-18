---
phase: 14-typescript-removal-and-hosted-closure
plan: "01"
subsystem: source-cutover
tags: [rust, typescript, deletion, source-authority, offline, packaging]

requires:
  - phase: 13-thin-npm-and-native-release
    provides: Offline Rust replacement, package corruption, and dual installed-path gates
provides:
  - Zero TypeScript/TSX product and test tree with no compiler config or generated legacy output
  - Permanent zero transitional TypeScript and legacy-JavaScript fixture authority
  - Sealed historical responsibility evidence independent of deleted executable sources
  - Current-commit offline native/npm candidate evidence from the Rust-only tree
affects: [14-02-permanent-zero-gates, 14-03-hosted-closure, release, compatibility]

tech-stack:
  added: []
  patterns:
    - Delete only after a clean replacement-first preflight records exact removal and retention hashes
    - Keep historical TypeScript responsibility data immutable while pinning its exact source inventory digest
    - Reject any reintroduced executable TypeScript instead of classifying it as transitional evidence

key-files:
  created: []
  modified:
    - crates/compat-harness/src/source_authority.rs
    - crates/compat-harness/tests/source_authority.rs
    - crates/compat-harness/src/coverage.rs
    - crates/compat-harness/tests/coverage.rs
    - crates/compat-harness/tests/provider_eval.rs
    - fixtures/compat/source-authority.v1.json
  deleted:
    - src/
    - test/
    - tsconfig.json

key-decisions:
  - "Advance the Phase 14 zero-authority validator with deletion because the old validator made a green post-delete tree impossible."
  - "Preserve the 97-source responsibility matrix byte-for-byte as historical evidence and pin its canonical source inventory digest after current authority reaches zero."
  - "Treat local GNU-LLVM release evidence only as development_only; hosted MSVC/Linux authority remains deferred to 14-03."

patterns-established:
  - "Final source authority: transitionalTypeScript.entries and transitionalLegacyTestFixtures.entries are both exactly empty."
  - "Historical coverage closure: deleted source hashes remain sealed evidence, while all replacement evidence must still exist as Rust or reviewed package orchestration."

requirements-completed: [RCUT-01]

coverage:
  - id: D1
    description: Every replaced TypeScript/TSX source, legacy test fixture, compiler config, and generated output is absent after a clean replacement preflight.
    requirement: RCUT-01
    verification:
      - kind: integration
        ref: target/phase14-delete-preflight/deletion-preflight.json
        status: pass
      - kind: integration
        ref: target/phase14-post-delete/evidence/windows-x86_64-gnullvm-dev.json
        status: pass
    human_judgment: false
  - id: D2
    description: Reintroduced TypeScript, non-empty transitional authority, legacy fixture reclassification, and fallback product routes fail closed.
    requirement: RCUT-01
    verification:
      - kind: unit
        ref: crates/compat-harness/tests/source_authority.rs
        status: pass
      - kind: integration
        ref: npm run verify:rust-contracts:candidate
        status: pass
    human_judgment: false
  - id: D3
    description: Immutable compatibility, migration, evaluation, and responsibility data survives deletion and remains linked to exact Rust replacement evidence.
    requirement: RCUT-01
    verification:
      - kind: unit
        ref: crates/compat-harness/tests/coverage.rs
        status: pass
      - kind: integration
        ref: cargo test --workspace --locked -- --skip hosted_cutover_evidence_matches_current_product
        status: pass
    human_judgment: false

duration: 1h
completed: 2026-07-18
status: complete
---

# Phase 14 Plan 01: Verified TypeScript Removal Summary

**The replaced TypeScript implementation and test tree are gone, while the Rust product, immutable migration/compatibility evidence, and both offline installation paths remain green.**

## Performance

- **Duration:** 1 hour
- **Completed:** 2026-07-18T12:03:00+08:00
- **Tasks:** 2
- **Tracked files deleted:** 203

## Accomplishments

- Ran a clean deletion preflight under `target/phase14-delete-preflight/`, covering format, Clippy, workspace/doc tests, Provider and retrieval evaluation, package corruption, release build, candidate verification, installed smoke, and milestone flow.
- Deleted `src/`, `test/`, `dist/`, and `tsconfig.json`; the dependency-free package/lock contract required no residual package edits.
- Set both transitional source-authority lists to zero and removed the obsolete Rust TypeScript dependency-graph parser.
- Kept all static `fixtures/compat/` evidence intact except the required zero-state source-authority update; the other 39 retained fixture hashes match preflight exactly.
- Sealed the unchanged 97-source historical responsibility matrix with canonical inventory SHA-256 `49c08ba55c6bedaaa3a5f0260913f8cd5a4dd5084c659879f629741fcf8d09c8`.
- Rebound final native/npm evidence to commit `01113cb` fingerprint `c87e8620dc9d51631cc6bfb7763cb445c0e9ff1786021b7eedbd08bec4ec8e5c` across 235 files.

## Task Commits

1. **Task 1: Clean replacement-first deletion preflight** - generated evidence only; no tracked mutation.
2. **Task 2: Remove replaced TypeScript and reach zero authority** - `01113cb` (refactor)

## Final Local Evidence

- Product fingerprint: `c87e8620dc9d51631cc6bfb7763cb445c0e9ff1786021b7eedbd08bec4ec8e5c` across 235 files.
- Binary SHA-256: `bda4c167614070b00329f113da1abdbe097e0f68a3731a23fba35f4389332fea`.
- Native archive SHA-256: `40173e525532882b5a993ae8b7119a5dc7ae61dfb2fe33e3945a08d8d12b4ba6`.
- npm archive SHA-256: `961d94896118e80da0719558938d673b8ee8411504156c95530089377afc8d96`.
- Native and npm installed identities: `minimax-codex-rust 0.1.0`; identical capability output hash `7aba6c...b3b0`.
- Offline/provider/credential/model-download counters: `true / 0 / 0 / 0`.
- Performance: 45.645 ms cold-start p95, 4,882,432-byte max idle RSS, and 1.576 ms Wiki BM25 p95.

This evidence is `windows-x86_64-gnullvm-dev` / `development_only`. It is not hosted Windows MSVC or Linux GNU authority.

## Deviations from Plan

### Auto-fixed Issues

**1. The old authority validator required deleted transitional entries**
- **Found during:** zero-contract RED test
- **Issue:** `transitionalTypeScript` could not be empty and the three diagnostic JS fixtures were mandatory, making the planned post-delete verification impossible.
- **Fix:** Pulled the zero-list validator and negative reintroduction tests forward from 14-02, then removed the obsolete TypeScript graph parser.
- **Committed in:** `01113cb`

**2. The coverage matrix was coupled to current transitional sources**
- **Found during:** clean post-delete workspace tests
- **Issue:** The unchanged historical matrix rejected all 97 deleted paths as unknown after authority reached zero.
- **Fix:** Kept the matrix byte-identical, pinned its canonical historical source digest, and continued validating every responsibility/evidence owner.
- **Committed in:** `01113cb`

**3. A Rust test still opened the deleted retrieval corpus copy**
- **Found during:** clean post-delete workspace tests
- **Issue:** The test read `test/fixtures/...` even though the immutable v1 corpus was already authoritative.
- **Fix:** Validated the immutable corpus, historical provenance hash, stable query IDs, thresholds, fingerprint, and Rust evidence without opening the deleted source copy.
- **Committed in:** `01113cb`

**4. Strict hosted freshness is intentionally unavailable locally**
- **Found during:** verification design
- **Issue:** Fresh hosted Windows MSVC/Linux evidence can only exist after the 14-03 authorized workflow.
- **Fix:** Ran the complete candidate workspace/compatibility path and skipped only `hosted_cutover_evidence_matches_current_product`.
- **Committed in:** N/A; authorization boundary preserved.

No Provider/API call, credential read, dependency/model download, push, PR, publication, tag, merge, or hosted workflow trigger occurred.

## Verification

- Offline GNU-LLVM workspace format and Clippy (`--all-targets`, `-D warnings`) - passed.
- Offline GNU-LLVM full workspace and doc tests - passed; only hosted-freshness was skipped.
- Rust Provider evaluation - 20/20 passed.
- Rust retrieval evaluation - 175 cases and all locked metrics at 1.0; disabled-path external counters zero.
- Package corruption suite - 19/19 passed.
- Candidate source authority, compatibility, migration support, release verification, dual installed smoke, and milestone flow - passed.
- Retained fixture comparison - 39/39 immutable files byte-identical; only the required source-authority zero-state file changed.

## Next Phase Readiness

- 14-02 can now make zero-TS/compiler/dependency/fallback enforcement permanent in CI and replace the commit-snapshot v2 fingerprint with final-tree v3 coverage.
- Hosted evidence remains reserved for 14-03 and requires explicit authorization.

## Self-Check: PASSED

- Commit `01113cb` exists on `codex/rust-convergence-v3`.
- The repository has no `src/`, `test/`, `dist/`, `tsconfig.json`, TS/TSX authority entry, or legacy transitional fixture entry.
- Final local packages and evidence bind the current 235-file commit fingerprint and remain offline development-only evidence.

---
*Phase: 14-typescript-removal-and-hosted-closure*
*Completed: 2026-07-18*
