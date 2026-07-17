---
phase: MMX-11-rust-verification-and-evaluation-authority
verified: 2026-07-17T20:01:36Z
status: passed
score: 8/8 must-haves verified
behavior_unverified: 0
overrides_applied: 0
re_verification:
  previous_status: gaps_found
  previous_score: 5/8
  gaps_closed:
    - "Historical TypeScript responsibilities now have exact semantic contracts, behaviorally relevant owners, and source-complete reviewed retirements; the previously false public/safety retirements are Rust-covered."
    - "Retry and continue now execute as distinct runtime operations with unique turn/request identities, immutable retry provenance, terminal outcomes, journal persistence, and restart replay evidence."
    - "The npm test graph is preflighted before imports by TypeScript and independently by Rust; the two former evaluator-driving tests are static hash/authority checks and cannot execute src/eval modules."
  gaps_remaining: []
  regressions: []
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
**Verified:** 2026-07-17T20:01:36Z
**Status:** passed
**Re-verification:** Yes - the three gaps from the 2026-07-17 initial verification are closed.

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|---|---|---|
| 1 | The responsibility matrix exactly covers the Phase 10 verification subset without confusing it with the complete TypeScript inventory. | VERIFIED | Schema v2 contains exactly 97 source path/hash entries and 101 unique responsibility rows. Coverage/source-authority validation passed against the live repository. |
| 2 | Every matrix disposition and evidence link is semantically complete, with no public retirement or fake Rust evidence. | VERIFIED | The matrix now has 45 reviewable evidence contracts covering all 101 rows exactly once: 89 Rust-covered, 4 package-smoke, and 8 narrow reviewed retirements. The formerly collapsed retrieval family is separated into seven semantic contracts; agent/kernel/budget/recovery/model/credential/summary families are Rust-covered, not retired. `coverage.rs:228-396` enforces exact contract assignment, category/class compatibility, owner equality, source-complete retirement review, and incompatible owner reuse. The semantic mutation/audit test passed. |
| 3 | CLI, lifecycle, tool, and TUI replacement tests exercise distinct required behavior. | VERIFIED | `restart.rs:168` runs a source turn, normal continue, and retry through `RuntimeDriver`, checks unique identities and distinct retry provenance, then reopens `RuntimeStore` and checks all three terminal turns survive replay. The parser-only evidence link is gone. |
| 4 | Provider evaluation is fixture-only, credential/network independent, production-Rust-backed, fingerprint/golden strict, and machine-readable. | VERIFIED | The production-backed Provider evaluator and its committed 20-check golden remain unchanged. The exact golden/repeat test passed, and candidate verification passed offline. |
| 5 | A Rust Provider failure remains non-zero/release-blocking even if package smoke succeeds. | VERIFIED | The Rust failure/non-authority contract remains in the Provider evaluator tests; package aliases still invoke the exact Rust command, and coverage/Provider/retrieval remain ordered before release work. No regression was introduced by Plans 11-05/11-06. |
| 6 | Retrieval evaluation consumes the immutable 175-case corpus through production exact/BM25 ranking with strict IDs, fingerprint, thresholds, and no-match labels. | VERIFIED | The committed Rust retrieval evaluator, immutable corpus, and golden remain authoritative. The exact candidate/metrics/degradation test passed. |
| 7 | Deterministic semantic doubles only rerank lexical candidates, reject outsiders, and preserve BM25 on every degraded path without model/network use. | VERIFIED | The exact retrieval report test again proved locked corpus metrics, candidate isolation, outsider rejection, and truthful degraded modes through production retrieval APIs. |
| 8 | Package, CI, repository verification, and npm test use Rust reports as the sole executable evaluation authority while TypeScript evaluators remain inert. | VERIFIED | `run-tests.ts:10-12` validates the full discovered dependency graph before the first test import. `test-discovery.ts:21-63` walks local dependencies transitively and rejects `src/eval/**`; `source_authority.rs:305,349-385` independently enforces the same boundary from both verify modes. The two evaluator tests at `provider-conformance.test.ts:10` and `capability-retrieval-report.test.ts:10` perform only static source/hash/manifest/package/golden checks. Rust graph mutation tests and the focused TypeScript graph/static suite passed. |

**Score:** 8/8 truths verified (0 present-but-behavior-unverified)

## Closed Gap Evidence

### 1. Semantic responsibility ownership and retirement

- The matrix's 45 contracts are the review layer over the 101 historical responsibility rows; every row is assigned once and must carry exactly its contract owner's evidence.
- The 18 formerly collapsed retrieval sources now map to seven relevant contracts: evaluation authority, exact/BM25, catalog/policy, command/dispatch, hybrid candidate isolation, corpus manifest, and snapshot/refresh.
- Reuse of the 175-case lexical test is limited to the compatible BM25/exact/query-normalization contract.
- The prior false-retirement families are now bound to substantive Rust state-machine, runtime, configuration, credential, and compaction tests.
- The eight remaining retirements are narrow and source-complete: live credential-bearing Provider smoke, three legacy diagnostic fixtures, TypeScript test helper plumbing, npm diagnostic harness execution, the unshipped token heuristic, and React/Ink ownership timing.
- Mutation evidence rejects unrelated lexical ownership for retry/continue, duplicate or missing semantic assignment, incompatible contract class/category, false public/safety retirement, and incomplete retirement review.

### 2. Real retry/continue execution and durability

`retry_and_continue_execute_distinct_durable_outcomes` proves:

1. A source prompt completes through the real runtime driver.
2. Continue creates a normal new turn with a new request identity.
3. Retry creates another new turn linked to the immutable source turn.
4. Both outcomes are terminal, neither rewrites the source, and the two identities differ from one another.
5. Reopening the journal reconstructs the same three turns and retry provenance.

This is execution/state evidence, not command parsing or enum matching.

### 3. TypeScript evaluator inertness

- The npm test entry discovers tests, validates their complete local dependency graph, and only then imports them.
- Both graph implementations cover static imports, side-effect imports, re-exports, import-equals/`require`, literal dynamic imports, `.js`-to-`.ts/.tsx` resolution, normalized dot segments, Windows separators, cycles, and unsafe/unresolved/ambiguous/symlinked failure cases.
- The Provider and retrieval TypeScript tests no longer import or call either evaluator. They read source bytes, verify authority hashes/dispositions, confirm exact Rust aliases/evidence, and inspect committed Rust report goldens.
- `package.json:29-31` retains exact Rust evaluator aliases and aggregate order. CI runs the Rust authority gate before transitional TypeScript tests and the explicit Rust evaluator steps before build/package/evidence.

## Required Artifacts

| Artifact | Expected | Status | Details |
|---|---|---|---|
| `fixtures/compat/verification/typescript-responsibilities.v1.json` | Exact reviewable semantic disposition matrix | VERIFIED | 97 sources, 101 rows, 45 exact semantic contracts, 89 Rust-covered, 4 package-smoke, 8 reviewed retirements; no unresolved or false public/safety retirement. |
| `crates/compat-harness/src/coverage.rs` | Strict completeness/evidence/disposition validator | VERIFIED | Schema v2 validates exact contract closure, semantic class/category, exact owner equality, compatible reuse, forbidden retirement IDs, and source-complete retirement review. |
| `crates/compat-harness/tests/coverage.rs` | Positive and negative semantic matrix gates | VERIFIED | Seven tests passed, including the collapsed/unrelated/false-retirement semantic audit. |
| `crates/cli/tests/restart.rs` | Executed retry/continue outcome evidence | VERIFIED | Uses production runtime/store paths and proves terminal state plus restart replay. |
| `crates/compat-harness/src/source_authority.rs` | Rust-owned test-graph authority | VERIFIED | Invoked from source authority before compatibility decisions; transitive fail-closed graph walk is substantive and mutation-tested. |
| `test/run-tests.ts` and `test/test-discovery.ts` | Pre-import npm test graph guard | VERIFIED | Graph validation precedes `importTestFiles`; focused graph suite passed. |
| Two evaluator `.test.ts` files | Static inert-source/authority checks | VERIFIED | No evaluator import or call; both source inputs remain present and hash-pinned for Phase 14. |
| Provider evaluator, manifest, and golden | Offline Rust Provider authority | VERIFIED | Initial verification evidence remains valid; exact golden/repeat regression passed. |
| Retrieval evaluator, corpus, manifest, and golden | Deterministic Rust retrieval authority | VERIFIED | Initial verification evidence remains valid; exact candidate/degradation regression passed. |
| `package.json` and `.github/workflows/ci.yml` | Rust-only evaluation authority and fail-closed order | VERIFIED | Exact aliases/order remain in place and source-authority tests passed. |

## Key Link Verification

| From | To | Via | Status |
|---|---|---|---|
| Coverage rows | Semantic evidence contracts | Exact responsibility IDs, class/category, and owner equality | WIRED |
| Semantic contracts | Rust behavior tests/reviewed retirement | Exact path + test owner or exact source-complete review | WIRED |
| `/continue` and retry APIs | Durable journal replay | `RuntimeDriver` -> `RuntimeStore` | WIRED |
| npm test | Discovered dependency preflight | `discoverTestFiles` -> `validateDiscoveredTestGraph` -> `importTestFiles` | WIRED |
| Rust verify modes | Independent test graph authority | `validate_source_authority` -> `validate_discovered_test_graph` | WIRED |
| Former evaluator tests | Rust authority artifacts | Static hashes, matrix contracts, package aliases, and Rust goldens | WIRED |
| Package/CI | Rust Provider/retrieval evaluators | Exact Cargo commands before downstream release work | WIRED |

## Behavioral Verification

All Rust commands used the already-installed `1.97.0-x86_64-pc-windows-gnullvm` toolchain with bundled `rust-lld` and `CARGO_NET_OFFLINE=true`. This is local GNU-LLVM development evidence only, not Windows MSVC, Linux GNU, hosted, or release evidence.

| Command / check | Result |
|---|---|
| `cargo test -p minimax-compat-harness --test coverage --test source_authority --locked` | 7 coverage + 13 source-authority tests passed |
| Exact semantic audit test | 1 passed before the complete coverage suite |
| Exact `retry_and_continue_execute_distinct_durable_outcomes` | 1 passed |
| Focused `test-discovery` + two static evaluator tests | 12 passed |
| Exact Rust discovered-test graph mutation test | 1 passed before the complete source-authority suite |
| Exact Provider golden/repeat test | 1 passed |
| Exact retrieval metrics/candidate/degradation test | 1 passed |
| Core semantic-owner suites (`tool_machine`, `runtime_machine`, `session_machine`, `compaction_trace`) | 27 passed |
| `npm run check` | passed |
| `cargo run -p minimax-compat-harness --locked -- verify-candidate` | exited 0 |

The full Rust workspace suite was not repeated because it still includes the intentionally stale hosted fingerprint test deferred to Phase 14. The candidate path and all Phase 11 focused gates passed. The already-green full npm suite was not duplicated; the direct closure evidence is the two independent graph validators plus the focused 12-test Node suite.

## Requirements Coverage

| Requirement | Source Plans | Status | Evidence |
|---|---|---|---|
| RVE-01 | 11-01, 11-05 | SATISFIED | Exact semantic responsibility closure, reviewed retirements, substantive public/safety Rust owners, and executed durable retry/continue outcomes. |
| RVE-02 | 11-02, 11-04, 11-06 | SATISFIED | Fixture-only Rust Provider evaluation remains authoritative; no discovered TypeScript test can execute its evaluator. |
| RVE-03 | 11-03, 11-04, 11-06 | SATISFIED | Production Rust retrieval evaluation remains authoritative and the TypeScript evaluator is transitively inert. |

No Phase 11 requirement is orphaned from its plans.

## Anti-Patterns and Human Verification

No blocking stub, placeholder, evaluator fallback, TypeScript authority path, network/credential/model-download path, or unrelated semantic owner remains in the Phase 11 artifacts. The `todo`/`placeholder` strings in `coverage.rs` are validator rejection markers, not implementation debt.

No human verification is required. No Phase 11 plan declares a probe or `<human-check>` block.

## Deferred Items

| Item | Addressed In | Reason |
|---|---|---|
| Delete inert TypeScript/TSX sources, tests, evaluators, and legacy diagnostic fixtures | Phase 14 / 14-01 | Phase 11 changes authority while retaining exact hash-pinned deletion inputs. |
| Refresh hosted Windows MSVC/Linux GNU product fingerprints and release evidence | Phase 14 | Local GNU-LLVM evidence must not be represented as hosted/release evidence. |

## Final Assessment

Phase 11 now achieves its stated goal. Historical verification ownership is semantically reviewable, the two previously parser-only commands have real durable execution evidence, and both TypeScript evaluators are unreachable from npm test while their source remains hash-pinned. The Rust Provider and retrieval reports are the sole executable evaluation authority, and all three prior verification gaps are closed without regression.

---

_Verified: 2026-07-17T20:01:36Z_
_Verifier: Codex acting under the gsd-verifier contract_
