---
phase: MMX-11-rust-verification-and-evaluation-authority
plan: "03"
subsystem: retrieval-evaluation-authority
tags: [rust, retrieval, bm25, exact, candidate-only, deterministic-report]
requires:
  - phase: MMX-11-rust-verification-and-evaluation-authority
    provides: Immutable 175-case retrieval corpus and strict responsibility disposition from Plan 11-02
provides:
  - strict Rust consumption and validation of the immutable 175-case retrieval corpus
  - deterministic exact, BM25, workspace, project, hybrid, and degraded retrieval report
  - bounded candidate-only semantic evidence with outsider rejection and BM25 preservation
  - retrieval-eval JSON command and repository verification wiring
affects: [phase-11-ci-authority, phase-14-typescript-removal, hosted-release-refresh]
tech-stack:
  added: [tokio direct runtime dependency for the existing async retrieval APIs]
  patterns: [strict fingerprinted evaluation manifest, production-ranking reuse, deterministic in-memory semantic double]
key-files:
  created:
    - crates/compat-harness/src/retrieval_eval.rs
    - crates/compat-harness/tests/retrieval_eval.rs
    - fixtures/compat/evaluations/retrieval.v1.json
    - fixtures/compat/evaluations/retrieval-report.expected.json
  modified:
    - Cargo.lock
    - crates/compat-harness/Cargo.toml
    - crates/compat-harness/src/lib.rs
    - crates/compat-harness/src/main.rs
    - crates/retrieval/tests/lexical.rs
key-decisions:
  - "The transferred 175-case corpus is the only lexical evaluation input; its stable IDs, fingerprint, exact count, and thresholds fail closed in Rust."
  - "BM25 is authoritative candidate recall; the deterministic semantic double observes and returns only that bounded set, and outsiders degrade to the unchanged BM25 order."
  - "Missing, damaged, failed, malformed, timed-out, and outsider semantic paths publish exact stable reasons without loading a model or using network, Provider, or credentials."
patterns-established:
  - "Retrieval report bytes are golden-checked after newline normalization and repeat identically under secret-bearing environment variables."
  - "Repository verification requires the Rust retrieval report before compatibility decisions, while npm and CI authority remain unchanged until 11-04."
requirements-completed: [RVE-03]
coverage:
  - id: D1
    description: "The immutable mixed-language 175-case corpus passes strict schema, stable-ID, fingerprint, exact/BM25, no-match, and unchanged-threshold gates through production Rust ranking."
    requirement: RVE-03
    verification:
      - kind: integration
        ref: "crates/retrieval/tests/lexical.rs#existing_typescript_175_case_fixture_meets_capability_gates"
        status: pass
      - kind: integration
        ref: "crates/compat-harness/tests/retrieval_eval.rs#retrieval_evaluation_matches_committed_golden_and_is_repeatable"
        status: pass
    human_judgment: false
  - id: D2
    description: "Optional semantic evaluation can only rerank the bounded lexical candidate union; outsider, failure, malformed, timeout, and unavailable cases preserve BM25."
    requirement: RVE-03
    verification:
      - kind: integration
        ref: "crates/compat-harness/tests/retrieval_eval.rs#report_proves_locked_corpus_metrics_candidate_isolation_and_degradation"
        status: pass
      - kind: integration
        ref: "crates/retrieval/tests/project_discovery.rs#every_semantic_failure_preserves_the_bm25_order"
        status: pass
    human_judgment: false
  - id: D3
    description: "retrieval-eval emits a byte-stable JSON-only golden and repository verification runs it before compatibility decisions without granting npm or CI authority."
    requirement: RVE-03
    verification:
      - kind: integration
        ref: "crates/compat-harness/tests/retrieval_eval.rs#retrieval_eval_command_is_json_only_and_deterministic"
        status: pass
      - kind: integration
        ref: "cargo run -p minimax-compat-harness --locked -- verify-candidate"
        status: pass
    human_judgment: false
duration: 17min
completed: 2026-07-18
status: complete
---

# Phase 11 Plan 03: Rust Retrieval Evaluator Summary

**A fixture-only Rust evaluator now proves the immutable 175-case exact/BM25 baseline, bounded semantic reranking, stable no-match behavior, and byte-deterministic degraded-mode reporting without a model or network.**

## Performance

- **Duration:** 17 min
- **Started:** 2026-07-17T16:10:00Z
- **Completed:** 2026-07-17T16:27:00Z
- **Tasks:** 2
- **Files created/modified:** 9
- **Execution mode:** Generic-agent workaround with the complete `gsd-executor` contract loaded.

## Accomplishments

- Moved Rust lexical evidence fully onto the immutable compatibility corpus and made schema drift, fingerprint drift, duplicate/invalid stable IDs, case loss, threshold changes, manufactured no-match results, and platform-dependent ties fail.
- Added a deterministic report with 175/175 corpus cases, 15 exact cases, 160 BM25/no-match cases, all five locked metrics at `1.0`, 4/4 project cases, and 15/15 mixed-language workspace cases.
- Proved that the semantic double receives exactly the five recalled lexical candidates, returns only those IDs, may rerank only that set, and cannot inject `outside/bm25`.
- Published six exact degradation scenarios (`embedding_missing`, `invalid_manifest`, `helper_unavailable`, `malformed_vector`, `helper_timeout`, outsider-as-`malformed_vector`) with the original BM25 IDs preserved.
- Added `retrieval-eval --format json` and fail-closed repository verification without modifying package scripts, npm authority, CI, or transitional TypeScript evaluator sources.

## Task Commits

Each behavior task followed RED then GREEN:

1. **Task 1 RED: add failing immutable retrieval corpus gates** - `bd58757` (test)
2. **Task 1 GREEN: consume immutable retrieval corpus** - `bae427e` (feat)
3. **Task 2 RED: add failing retrieval evaluator contract** - `ec1d9db` (test)
4. **Task 1 formatting correction: apply workspace rustfmt** - `acfa75c` (style)
5. **Task 2 GREEN: add deterministic Rust retrieval evaluator** - `683f8f9` (feat)

The summary and planning trackers are committed separately in the final metadata commit.

## Files Created/Modified

- `crates/compat-harness/src/retrieval_eval.rs` - Strict manifest/corpus loading, production lexical/workspace/project evaluation, deterministic semantic runner, degradation evidence, report serialization, and golden verification.
- `crates/compat-harness/tests/retrieval_eval.rs` - Golden/repeatability, boundary/degradation, strict mutation, command, credential-negative, and verification-order tests.
- `fixtures/compat/evaluations/retrieval.v1.json` - Fingerprinted evaluator inputs, project cases, candidate query, limits, and ordered degraded scenarios.
- `fixtures/compat/evaluations/retrieval-report.expected.json` - Byte-stable passing retrieval report.
- `crates/retrieval/tests/lexical.rs` - Strict immutable-corpus ownership, metric, mutation, no-match, and stable-tie evidence.
- `crates/compat-harness/src/lib.rs` - Public retrieval evaluator/report exports.
- `crates/compat-harness/src/main.rs` - JSON command dispatch and repository verification gate.
- `crates/compat-harness/Cargo.toml` and `Cargo.lock` - Existing pinned Tokio runtime made a direct compat-harness dependency for async production search.

## Decisions Made

- Kept all recall and ranking behavior in `minimax-retrieval`; the evaluator constructs production catalogs/indexes and records results rather than implementing a second BM25, RRF, or vector-validation kernel.
- Used deterministic in-memory `EmbeddingRunner` outputs only. No resource validation path opens a model, and report evidence fixes network, Provider, download, and model-load counts at zero.
- Applied the transferred corpus thresholds exactly as committed. The report cannot silently lower them because both typed corpus validation and the golden bind the values.
- Kept `src/eval/capability-retrieval-report.ts`, `package.json`, and CI unchanged. Their authority cutover remains isolated to 11-04.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Formatting] Applied workspace rustfmt after Task 1 GREEN**
- **Found during:** Task 2 compile verification
- **Issue:** The completed Task 1 source had two rustfmt-only differences that would fail the plan-wide formatting gate.
- **Fix:** Applied `cargo fmt --all` and committed only the formatting delta.
- **Files modified:** `crates/retrieval/tests/lexical.rs`
- **Verification:** `cargo fmt --all -- --check` passed.
- **Committed in:** `acfa75c`

**2. [Rule 3 - Lock metadata] Recorded the planned direct Tokio dependency in Cargo.lock**
- **Found during:** Task 2 first locked compile
- **Issue:** The already-pinned workspace dependency was newly direct for `minimax-compat-harness`, so `--locked` correctly rejected stale package metadata.
- **Fix:** Regenerated the single lockfile dependency line with Cargo offline; no package was installed or downloaded.
- **Files modified:** `Cargo.lock`
- **Verification:** All later Cargo commands passed with `--locked`.
- **Committed in:** `683f8f9`

**3. [Rule 1 - Tracker correction] Aligned planning progress with the GSD command result**
- **Found during:** Final planning-state verification
- **Issue:** `state update-progress` reported 35/44 plans and 80% but persisted the previous 71% value in `STATE.md`; `roadmap update-plan-progress` updated the file-backed plan checklist and 3/4 count but left the duplicate descriptive 11-03 entry unchecked.
- **Fix:** Corrected the STATE frontmatter percentage to 80% and marked the matching ROADMAP descriptive entry complete.
- **Files modified:** `.planning/STATE.md`, `.planning/ROADMAP.md`
- **Verification:** STATE records 35 completed plans and 80% progress; both ROADMAP 11-03 entries are checked and Phase 11 remains correctly at 3/4 plans.
- **Committed in:** Final metadata commit

---

**Total deviations:** 3 auto-fixed (1 formatting, 1 blocking lock metadata, 1 tracker correction).
**Impact on plan:** All corrections were mechanical and required for the declared gates or accurate planning state; none changed retrieval behavior or expanded authority.

## Issues Encountered

- The exact combined command `cargo test -p minimax-retrieval -p minimax-compat-harness --locked` reached the intentionally stale `hosted_cutover_evidence_matches_current_product` release-fingerprint test and failed with `CutoverEvidence`. This is the locked, pre-existing post-Phase-9 hosted-refresh condition recorded in STATE.md, not retrieval evaluator failure. No hosted fixture was forged from GNU-LLVM evidence. Re-running the same two crates with only that hosted-only test filtered passed every remaining unit, integration, and doc test.

## Verification

- `cargo fmt --all -- --check` - passed.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` - passed.
- `cargo test -p minimax-retrieval --test lexical --locked existing_typescript_175_case_fixture_meets_capability_gates -- --exact` - passed.
- `cargo test -p minimax-compat-harness --test retrieval_eval --locked` - 5 passed.
- `cargo test -p minimax-retrieval -p minimax-compat-harness --locked -- --skip hosted_cutover_evidence_matches_current_product` - all non-hosted unit, integration, and doc tests passed; one hosted-only test filtered as described above.
- `cargo run -p minimax-compat-harness --locked -- retrieval-eval --format json` - emitted the committed deterministic passing golden.
- `cargo run -p minimax-compat-harness --locked -- verify-candidate` - passed with Provider and retrieval evaluators before compatibility decisions.

All Rust development evidence used the already-installed `1.97.0-x86_64-pc-windows-gnullvm` toolchain and bundled `rust-lld`. It is local development evidence only and is not represented as Windows MSVC, Linux hosted, or release evidence.

## Known Stubs

None. The evaluator, fixtures, command, and tests contain no TODO, FIXME, placeholder, coming-soon, unavailable implementation, mock UI data, network client, model loader, or live Provider path.

## Threat Surface

No unplanned threat surface was introduced. Inputs are strict repository-relative fingerprinted files; the evaluator adds no endpoint, credential resolution, HTTP client, keyring access, subprocess, model file access, download, or TypeScript execution.

## User Setup Required

None - evaluation is deterministic, local, offline, fixture-only, credential-independent, and model-free.

## Next Phase Readiness

- Plan 11-04 can switch package/CI evaluation authority to the committed Rust Provider and retrieval reports without changing either evaluator contract.
- Phase 14 can delete transitional retrieval evaluator/test sources against the immutable corpus, exact Rust test names, and golden report.
- Hosted release evidence remains intentionally stale until the planned final Windows/Linux fingerprint refresh.

## Self-Check: PASSED

- All four created artifacts and all five implementation commits resolve on disk/in git history.
- The focused corpus, evaluator, golden, command, candidate verification, formatting, and strict workspace Clippy gates pass.
- No TypeScript evaluator, package script, CI file, Provider credential path, embedding model, or network resource was changed or used.
- The one known hosted-release fingerprint failure was preserved and disclosed rather than weakened or rewritten from local GNU-LLVM evidence.

---
*Phase: MMX-11-rust-verification-and-evaluation-authority*
*Completed: 2026-07-18*
