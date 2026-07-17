---
phase: MMX-11-rust-verification-and-evaluation-authority
verified: 2026-07-17T17:24:46Z
status: gaps_found
score: 5/8 must-haves verified
behavior_unverified: 0
overrides_applied: 0
gaps:
  - truth: "Every historical TypeScript verification responsibility has a semantically correct Rust, package-smoke, or retirement disposition."
    status: failed
    reason: "The 97-path inventory is exact, but unrelated public and safety behaviors are collapsed onto broad tests and several public/safety sources are retired with boilerplate rationales."
    artifacts:
      - path: "fixtures/compat/verification/typescript-responsibilities.v1.json"
        issue: "Eighteen retrieval sources with catalog, policy, dispatcher, refresh, snapshot, adapter, and ranking responsibilities all cite one 175-case lexical ranking test; agent/kernel/model/summary safety sources are retired as having no shipped outcome."
      - path: "crates/compat-harness/src/coverage.rs"
        issue: "Validation proves path/hash membership and that a named Rust function exists, but cannot prove the named test exercises the stated responsibility or that a retirement is genuinely non-public."
    missing:
      - "Split collapsed source rows into their actual distinct responsibilities and bind each public/safety outcome to a behaviorally relevant Rust test."
      - "Reclassify public/safety retirements or document a specific reviewed reason that the exact outcome is no longer part of the shipped contract."
      - "Add reviewable enforcement that rejects behaviorally unrelated evidence, not only absent function names."
  - truth: "New CLI, lifecycle, tool, and TUI Rust evidence exercises each distinct required public outcome rather than aliases or parser stubs."
    status: failed
    reason: "The explicit retry/continue outcome responsibility is bound to a test that only parses /retry and /continue into enum variants; it never executes either outcome."
    artifacts:
      - path: "crates/tui/tests/command_render.rs"
        issue: "parser_covers_every_manifest_command_alias_and_argument_shape asserts command parsing only, then self-binds ts-command-retry-continue-outcomes."
      - path: "fixtures/compat/verification/typescript-responsibilities.v1.json"
        issue: "The matrix treats the parser test as outcome evidence."
    missing:
      - "Add deterministic Rust tests that execute retry and continue and assert their distinct terminal/state outcomes, then point the responsibility rows to those tests."
      - "Audit the remaining many-to-one evidence mappings for the same alias-versus-behavior error."
  - truth: "TypeScript evaluator sources remain inert and Rust reports are the sole package and CI evaluation authority."
    status: failed
    reason: "CI runs npm test; the test runner imports every *.test.ts file, and two discovered tests directly import and execute src/eval/provider-conformance.ts and src/eval/capability-retrieval-report.ts. Their failures therefore still block CI alongside Rust."
    artifacts:
      - path: ".github/workflows/ci.yml"
        issue: "Run transitional TypeScript tests executes npm test before the explicit Rust evaluator steps."
      - path: "test/test-discovery.ts"
        issue: "Discovery recursively imports every .test.ts/.test.tsx module."
      - path: "test/provider-conformance.test.ts"
        issue: "Directly imports and runs the transitional Provider evaluator."
      - path: "test/capability-retrieval-report.test.ts"
        issue: "Directly imports and runs the transitional retrieval evaluator."
      - path: "crates/compat-harness/src/source_authority.rs"
        issue: "The authority scan rejects only direct src/eval or tsx/ts-node eval commands and misses npm test's transitive evaluator execution."
    missing:
      - "Exclude the two evaluator-driving TypeScript tests from package/CI discovery or otherwise make the evaluator modules unreachable while retaining their hash-pinned source until Phase 14."
      - "Add a transitive test-discovery authority check so npm test cannot silently reintroduce TypeScript evaluator execution."
deferred:
  - truth: "Delete the hash-pinned TypeScript/TSX product, tests, evaluator sources, and legacy diagnostic fixtures."
    addressed_in: "Phase 14"
    evidence: "Phase 14 success criterion 1 and plan 14-01 own deletion of replaced TypeScript source, tests, configuration, and legacy references."
  - truth: "Refresh the stale hosted Windows MSVC/Linux GNU product fingerprint and hosted release evidence."
    addressed_in: "Phase 14"
    evidence: "Phase 14 success criterion 3 explicitly requires final hosted evidence and rejects stale or local GNU-LLVM evidence."
---

# Phase 11: Rust Verification and Evaluation Authority Verification Report

**Phase Goal:** Maintainers can decide parity and release readiness from deterministic Rust tests and evaluations before any TypeScript-covered source is removed.
**Verified:** 2026-07-17T17:24:46Z
**Status:** gaps_found
**Re-verification:** No - initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|---|---|---|
| 1 | The responsibility matrix exactly covers the Phase 10 verification subset without confusing it with the complete TypeScript inventory. | VERIFIED | Independent inventory is exactly 91 `test/**/*.ts(x)` + 2 `src/eval` + 1 `src/smoke` + 3 legacy diagnostic JS = 97 paths, with no matrix diff or duplicate; Phase 10 separately hash-pins all 191 TS/TSX files. `coverage.rs:304-324` encodes the same subset. |
| 2 | Every matrix disposition and evidence link is semantically complete, with no public retirement or fake Rust evidence. | FAILED | The matrix has 101 unique responsibility IDs and no unresolved enum value, but semantic evidence is not exact. At `typescript-responsibilities.v1.json:332-638`, catalog/dispatcher/policy/refresh/snapshot/adapter responsibilities all cite the lexical-only test at `lexical.rs:307`; at lines 149-162 and 292-305, agent-loop and application-kernel public/safety behavior is retired as having no shipped outcome. |
| 3 | CLI, lifecycle, tool, and TUI replacement tests exercise distinct required behavior. | FAILED | Lifecycle finalization and permission-reset tests are substantive, but `command_render.rs:17-67` binds retry/continue outcomes to a parser-only test. The named test passes while proving only enum parsing, not retry/continue state or terminal outcomes. |
| 4 | Provider evaluation is fixture-only, credential/network independent, production-Rust-backed, fingerprint/golden strict, and machine-readable. | VERIFIED | `provider_eval.rs:151-272` loads fingerprinted fixtures and compatibility profiles, calls production `minimax_provider::replay_fixture`, checks 20 ordered protocol checks, emits JSON, and fails verification on report/golden drift. The focused golden/repeat test passed. No evaluator transport, credential, keyring, or endpoint call exists. |
| 5 | A Rust Provider failure remains non-zero/release-blocking even if package smoke succeeds. | VERIFIED | CLI dispatch at `main.rs:76-85` uses the Rust report pass flag; `provider_eval.rs:261-289` rejects failed checks and golden drift, and `provider_evaluation_authorizes_release` cannot turn a failed report green with a true package-smoke flag. |
| 6 | Retrieval evaluation consumes the immutable 175-case corpus through production exact/BM25 ranking with strict IDs, fingerprint, thresholds, and no-match labels. | VERIFIED | The source-equivalence test compares every query/expected ID and source SHA-256. `retrieval_eval.rs:521-610` locks thresholds, fingerprint, stable IDs, expected IDs, no-match consistency, and exactly 175 cases; `:680-784` uses production `CapabilityIndex::search`. |
| 7 | Deterministic semantic doubles only rerank lexical candidates, reject outsiders, and preserve BM25 on every degraded path without model/network use. | VERIFIED | `retrieval_eval.rs:950-1010` proves observed/returned semantic IDs equal the lexical set and outsiders restore BM25; `:1014-1074` covers unavailable, damaged, failed, malformed, timeout, and outsider paths. The focused candidate/degradation test passed. The only semantic runner is an in-memory test double over production discovery APIs. |
| 8 | Package and CI use Rust reports exclusively in coverage -> Provider -> retrieval order before build/package/evidence, while TypeScript evaluators remain inert. | FAILED | Direct aliases and ordering are correct (`package.json`, `ci.yml:35-68`), and the Rust CI-order mutation test passed. However `ci.yml:44-45` runs `npm test`; `test-discovery.ts:5-16` imports all test modules; the Provider and retrieval report tests directly execute both transitional evaluators. Rust is blocking, but not sole evaluation authority. |

**Score:** 5/8 truths verified (0 present-but-behavior-unverified)

### Deferred Items

| # | Item | Addressed In | Evidence |
|---|---|---|---|
| 1 | Delete replaced TS/TSX sources/tests/evaluators and the three diagnostic JS fixtures. | Phase 14 | Roadmap Phase 14 criterion 1; plan 14-01. |
| 2 | Replace stale hosted fingerprint with Windows MSVC and Linux GNU hosted evidence. | Phase 14 | Roadmap Phase 14 criterion 3 explicitly rejects stale/local GNU-LLVM evidence. |

### Required Artifacts

| Artifact | Expected | Status | Details |
|---|---|---|---|
| `fixtures/compat/verification/typescript-responsibilities.v1.json` | Exact reviewable disposition matrix | FAILED | Structurally exact (97 sources, 101 unique responsibilities, 71 Rust-covered, 4 package-smoke, 26 retired) but semantically over-collapsed and contains unsupported public/safety retirements. |
| `crates/compat-harness/src/coverage.rs` | Strict completeness/evidence/disposition validator | PARTIAL | Substantive and wired into both verify modes. It rejects schema/hash/path/disposition/evidence-name errors, but a function-name substring is accepted as proof even when the test does not exercise the responsibility. |
| `crates/compat-harness/tests/coverage.rs` | Positive and negative matrix gates | VERIFIED | Six tests exist; named repository validation passed. Negatives cover missing/hash/duplicate/unresolved/TS evidence/public retirement shape, not evidence semantics. |
| `crates/compat-harness/src/provider_eval.rs` | Offline Provider report runner | VERIFIED | 651 substantive lines, production replay/profile validation, strict manifest/fingerprints, typed report, golden gate. |
| `fixtures/compat/evaluations/provider.v1.json` and golden | Immutable protocol contract/report | VERIFIED | Two protocols, ten checks each, fixture hashes, stable 20/20 golden. |
| `fixtures/compat/retrieval/capability-cases-expanded.v1.json` | Immutable 175-case corpus | VERIFIED | Preserves source SHA-256, queries, expected IDs/no-match, thresholds; adds stable IDs and corpus fingerprint. |
| `crates/compat-harness/src/retrieval_eval.rs` and golden | Exact/BM25/hybrid/degraded report | VERIFIED | 1,264 substantive lines using production `CapabilityIndex`, `ProjectDiscovery`, and `CapabilityWorkspace`; strict golden and failure gate. |
| `package.json` and `.github/workflows/ci.yml` | Sole Rust evaluation authority | FAILED | Exact Rust aliases/order are present, but CI's transitional `npm test` still reaches both TS evaluators transitively. |

### Key Link Verification

| From | To | Via | Status | Details |
|---|---|---|---|---|
| Coverage matrix | Phase 10 source authority | Exact path + SHA-256 set | WIRED | Exact 97-path subset; source authority separately validates the 191-file TS baseline and live hashes. |
| Compatibility `verify` / `verify-candidate` | Coverage -> Provider -> retrieval | `main.rs:103-111` | WIRED | All three fail-closed gates precede compatibility manifest decisions. |
| Provider evaluator | Production Provider replay/profile behavior | `replay_fixture`, compatibility provider validation | WIRED | Both protocols replay immutable frames and use shared Rust profile validation. |
| Retrieval evaluator | Production exact/BM25/workspace/project behavior | `CapabilityIndex`, `ProjectDiscovery`, `CapabilityWorkspace` | WIRED | Corpus and candidate/degradation reports use production ranking/search paths. |
| Package aliases | Rust evaluator subcommands | exact cargo commands | WIRED | `eval:provider` and `eval:retrieval` are Rust-only and propagate Cargo exit status. |
| CI | Coverage -> Provider -> retrieval -> build/package/evidence | named ordered steps | WIRED | Direct order is correct and mutation-tested. |
| CI `npm test` | Transitional TS evaluators | recursive test discovery and direct imports | NOT_WIRED_AS_PROMISED | This forbidden transitive link makes the TS evaluators active release gates. |

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
|---|---|---|---|---|
| Provider report | protocol checks/totals/pass | Fingerprinted JSONL -> production replay -> typed report -> golden | Yes | FLOWING |
| Retrieval report | corpus metrics/candidates/degradations/pass | Immutable corpus/catalogs -> production search -> deterministic runner -> typed report -> golden | Yes | FLOWING |
| Coverage decision | source/responsibility/evidence rows | Phase 10 authority + matrix + Rust source text | Structurally yes; behavior relationship is not evaluated | HOLLOW SEMANTIC LINK |
| Transitional TS evaluator result | Node test result | `npm test` -> recursive imports -> `src/eval/*` functions | Yes, contrary to inert-source requirement | FORBIDDEN FLOW |

### Behavioral Spot-Checks

All Rust commands used the already-installed `1.97.0-x86_64-pc-windows-gnullvm` toolchain with bundled `rust-lld` and `CARGO_NET_OFFLINE=true`. This is local GNU-LLVM development evidence only, not Windows MSVC, Linux GNU, hosted, or release evidence.

| Behavior | Command | Result | Status |
|---|---|---|---|
| Matrix accepts committed 97-source/101-row data | `cargo test -p minimax-compat-harness --test coverage --locked repository_matrix_validates_with_no_unresolved_responsibility -- --exact` | 1 passed; 5 filtered | PASS (structural only) |
| Provider report matches golden and repeats | `cargo test -p minimax-compat-harness --test provider_eval --locked provider_evaluation_matches_committed_golden_and_is_repeatable -- --exact` | 1 passed; 7 filtered | PASS |
| Retrieval report proves candidate isolation/degradation | `cargo test -p minimax-compat-harness --test retrieval_eval --locked report_proves_locked_corpus_metrics_candidate_isolation_and_degradation -- --exact` | 1 passed; 4 filtered | PASS |
| CI Rust gates precede package/evidence | `cargo test -p minimax-compat-harness --test source_authority --locked ci_keeps_rust_authority_ahead_of_packaging_and_fails_closed -- --exact` | 1 passed; 11 filtered | PASS |
| Claimed retry/continue outcome evidence | `cargo test -p minimax-tui --test command_render --locked parser_covers_every_manifest_command_alias_and_argument_shape -- --exact` | 1 passed; 6 filtered; source inspection shows parser assertions only | FAIL AS EVIDENCE |
| TypeScript evaluators are inert | Static trace through `ci.yml`, `run-tests.ts`, `test-discovery.ts`, and both evaluator tests | Both TS evaluator modules are imported and invoked by CI `npm test` | FAIL |

No full workspace suite was rerun. The known 441-test Node result is not used as authority here; running it necessarily executes the two forbidden transitional evaluators.

### Probe Execution

No Phase 11 plan or summary declares a probe and no `<human-check>` block exists. Probe execution is not applicable.

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|---|---|---|---|---|
| RVE-01 | 11-01 | Required public behavior has deterministic Rust evidence or reviewed retirement | BLOCKED | Path coverage is exact, but responsibility semantics, retirement validity, and retry/continue outcome evidence fail. |
| RVE-02 | 11-02 | Provider conformance is Rust-owned, fixture-only, machine-readable, offline | PARTIAL | Rust evaluator fully satisfies the technical report contract, but the historical TS Provider evaluator remains an active CI gate through `npm test`. |
| RVE-03 | 11-03, 11-04 | Retrieval is Rust-owned with exact/BM25, mixed language, BM25-first, candidate-only reranking, truthful degradation | PARTIAL | Rust retrieval evaluator passes all technical checks and direct release ordering is correct, but the TS retrieval evaluator also remains an active CI gate. |

No Phase 11 requirement is orphaned from the plans.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|---|---:|---|---|---|
| `fixtures/compat/verification/typescript-responsibilities.v1.json` | 332-638 | Many unrelated responsibilities cite one lexical test | BLOCKER | A green ranking test is presented as evidence for catalog safety, dispatch, policy, refresh, snapshot, and adapters it does not exercise. |
| `fixtures/compat/verification/typescript-responsibilities.v1.json` | 149-305 | Boilerplate public/safety retirement | BLOCKER | Agent tool ordering/budget and application lifecycle/permission outcomes are declared non-public despite Phase 11's own required behavior list. |
| `crates/tui/tests/command_render.rs` | 17-67 | Alias/parser test labeled as behavior outcome | BLOCKER | `/retry` and `/continue` parsing does not prove either outcome. |
| `crates/compat-harness/src/source_authority.rs` | 405-424 | Direct-command-only evaluator denial | BLOCKER | Allows `npm test` to execute TS evaluators transitively. |
| `crates/compat-harness/src/coverage.rs` | 185 | `todo`/`tbd` marker strings | INFO | Validator data, not an unresolved debt marker. |

### Human Verification Required

None. The failed invariants are observable through source/data-flow inspection and deterministic tests.

### Gaps Summary

Phase 11 produced strong, deterministic Rust Provider and retrieval evaluators. The 175-case corpus, production ranking/replay reuse, fingerprints, goldens, candidate-only semantic boundary, degraded BM25 preservation, and direct package/CI ordering are all substantive and verified.

The phase goal is nevertheless not achieved. The responsibility matrix is exact only at the file/hash bookkeeping layer: it does not faithfully describe the distinct behaviors inside those files, accepts behaviorally unrelated Rust tests, and retires several public/safety responsibilities. Separately, the TypeScript evaluators are still executed by the release-gating Node suite, so Rust reports are not the sole evaluation authority. These are Phase 11 gaps, not Phase 14 deletion or hosted-evidence work.

---

_Verified: 2026-07-17T17:24:46Z_
_Verifier: Codex acting under the gsd-verifier contract_
