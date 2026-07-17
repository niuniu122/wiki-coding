---
phase: MMX-10-rust-authority-and-source-boundaries
verified: 2026-07-17T14:49:26Z
status: passed
score: 12/12 must-haves verified
behavior_unverified: 0
overrides_applied: 0
re_verification:
  previous_status: gaps_found
  previous_score: 10/12
  gaps_closed:
    - "Every supported and legacy CLI/TUI product path executes Rust, and transitional TypeScript is non-executable."
    - "No supported or legacy command can create or mutate .mini-codex after the authority cutover."
  gaps_remaining: []
  regressions: []
deferred:
  - truth: "Replace transitional TypeScript behavioral, Provider, and retrieval verification and disposition each legacy diagnostic fixture."
    addressed_in: "Phase 11"
    evidence: "Phase 11 success criteria require deterministic Rust coverage/evaluations and explicit retirement decisions before TypeScript-covered source is removed."
  - truth: "Rebase compatibility and migration evidence on immutable fixtures without executing TypeScript."
    addressed_in: "Phase 12"
    evidence: "Phase 12 success criteria require fixture-only compatibility reports and complete source-preserving Rust migration coverage."
  - truth: "Remove TypeScript-only npm metadata/dependencies and prove normal npm/npx installation paths."
    addressed_in: "Phase 13"
    evidence: "Phase 13 success criteria remove TypeScript compiler/runtime and React/Ink dependencies from the packed package and add offline installed-package corruption checks."
  - truth: "Delete the transitional TypeScript tree and refresh final hosted Windows MSVC/Linux GNU evidence for one final fingerprint."
    addressed_in: "Phase 14"
    evidence: "Phase 14 success criteria own TypeScript removal and final hosted target/fingerprint closure; local GNU-LLVM evidence is development-only."
---

# Phase 10: Rust Authority and Source Boundaries Verification Report

**Phase Goal:** Users and maintainers have one executable product and writable runtime authority in Rust, while remaining JavaScript is limited to reviewed distribution orchestration.

**Verified:** 2026-07-17T14:49:26Z
**Status:** passed
**Re-verification:** Yes - previous status gaps_found, previous score 10/12

## Re-verification Verdict

Both prior blockers are closed. Plan 10-04 changed the live development command to the Rust CLI, added one shared Rust-owned package-script policy to both authority preflights, and covered the route with adversarial and real-repository tests. No regression was found in the ten truths that passed the initial verification.

### Closed Gaps

| Previous failed truth | Exists | Substantive | Wired and exercised | Verdict |
|---|---|---|---|---|
| Every product-facing package route executes Rust; transitional TypeScript cannot be a product entry | package.json now has dev = cargo run -p minimax-cli --locked --; start/bin retain the fixed launcher | source_authority.rs rejects incorrect dev/start/bin plus direct TS/TSX, dist/cli.js, named legacy, and arbitrary equivalent product aliases while preserving classified test/eval/smoke commands | baseline.rs reuses the policy; main.rs runs source authority before both verify modes; 10/10 source-authority tests, exact baseline/headless tests, and live npm dev all pass | CLOSED |
| No supported or legacy command can reach a writable .mini-codex runtime | the manifest retains .minimax as the sole writable root and .mini-codex only as readOnlyMigrationInput | the escaped TS package route is gone and future package mutations fail closed without executing the legacy writer | state-authority tests pass 3/3, the shared preflight rejects a second writable root and legacy route mutations, and verify-candidate passes | CLOSED |

## Goal Achievement

### Observable Truths

| # | Truth | Status | Current evidence |
|---|---|---|---|
| 1 | Every supported CLI/TUI, Provider, session, tool, Vault/Wiki, retrieval, capability, migration, and compatibility product route executes Rust; transitional TypeScript is not a product entry. | VERIFIED | dev runs Cargo for minimax-cli, start/bin use only the fixed launcher, and the shared validator rejects TS/TSX or legacy product routes. Live npm dev reports minimax-codex-rust 0.1.0. |
| 2 | The repository reports the complete JavaScript allowlist and rejects product imports, domain behavior, runtime download, fallback, and unknown JS paths. | VERIFIED | Exact independent inventory found 5 reviewed JS authorities plus 3 diagnostic fixtures, 8 tracked JS paths, zero missing/extra paths, and zero hash drift; negative authority tests pass. |
| 3 | Runtime commands write only Rust-owned .minimax; no supported or legacy command can create or mutate .mini-codex. | VERIFIED | .mini-codex is classified only as readOnlyMigrationInput, the TS product route is closed, route mutations fail closed, and 3/3 state-authority tests prove Rust writes and source-preserving migration behavior. |
| 4 | The Rust CLI and npm-installed command remain usable after the authority boundary. | VERIFIED | Live Rust dev identity passes; 3/3 product-identity tests retain exact direct/installed identity and isolated fixed-sibling wiring. Release and launcher sources were untouched by the gap closure. |
| 5 | A checked-in strict manifest classifies Rust roots, executable/package entries, allowed JS, transitional TS, legacy JS fixtures, immutable fixtures, supported targets, and state roots. | VERIFIED | Independent check found 9 present Rust roots, 1 executable entry, 5 JS authorities, 191 TS entries, 3 legacy fixtures, 7 immutable roots, 2 supported targets, and the exact state-root split. |
| 6 | Transitional TS is an exact hash-pinned shrinking inventory, and new TS/TSX or JS paths fail closed. | VERIFIED | 191 manifest TS/TSX entries exactly equal 191 tracked paths; all 199 TS/JS hash entries match. Inventory negatives pass. |
| 7 | The JavaScript allowlist is exact and limited to launcher/release/package/fingerprint orchestration. | VERIFIED | The same five reviewed sources remain exact and hash-pinned; the gap diff did not touch launcher, release scripts, or CI. |
| 8 | The three diagnostic JS fixtures are hash-pinned outside executable/JS authority and carry later-phase disposition metadata. | VERIFIED | Exactly three tracked fixtures remain in the separate class, their hashes match, and smuggling/second-root negatives pass. |
| 9 | Missing, unsafe, non-executable, unsupported, failed, or signaled Rust binaries fail non-zero with no legacy fallback. | VERIFIED | The fixed launcher and baseline contract remain present and unchanged; the exact compatibility product-baseline test passes. |
| 10 | Native and npm candidates contain one executable Rust product path and no packaged legacy application entry. | VERIFIED | Candidate assembly/release sources and the one-bin package mapping are unchanged; source authority and verify-candidate pass on the current tree. |
| 11 | Direct Rust and installed launcher smokes report the same identity and execute the exact manifest-bound binary. | VERIFIED | 3/3 product-identity tests pass; fixed sibling, isolated environment, and release hash-binding assertions remain intact. |
| 12 | CI runs Rust source authority/contracts before transitional Node checks and packaging, keeps Node non-authoritative, and cannot publish or expand permissions/platforms. | VERIFIED | The CI file is unchanged by 10-04; the current source-authority CI ordering/fail-closed test passes as part of 10/10. |

**Score:** 12/12 truths verified (0 present-but-behavior-unverified)

### Deferred Items

| Item | Addressed in | Why it is not a Phase 10 gap |
|---|---|---|
| Replace/retire transitional TypeScript tests, evaluations, and legacy-fixture diagnostics | Phase 11 | Phase 11 owns deterministic Rust verification replacement and disposition decisions. |
| Fixture-only compatibility and complete migration support-window evidence | Phase 12 | Phase 12 owns immutable fixture rebasing and migration-window closure. |
| Thin npm metadata/dependencies and normal npm/npx install closure | Phase 13 | Phase 13 owns TypeScript/React/Ink package dependency removal and installed-package corruption cases. |
| Delete TypeScript and refresh final hosted Windows MSVC/Linux GNU fingerprints | Phase 14 | Phase 14 owns source deletion and final hosted evidence. Local windows-x86_64-gnullvm-dev evidence cannot satisfy it. |

## Artifact Verification

| Artifact | Status | Re-verification evidence |
|---|---|---|
| package.json | VERIFIED | dev is the exact Cargo Rust route; start and the sole bin map to the fixed launcher; transitional test/eval scripts remain. |
| crates/compat-harness/src/source_authority.rs | VERIFIED | Substantive shared script classifier is called by executable-link validation and rejects TS/legacy product routes. |
| crates/compat-harness/src/baseline.rs | VERIFIED | Product baseline delegates to the same policy and then verifies launcher safety/fallback absence. |
| crates/compat-harness/tests/source_authority.rs | VERIFIED | Adversarial dev/start/equivalent-alias matrix, positive transitional scripts, exact inventory, shared preflight, state-root and CI negatives all pass. |
| crates/compat-harness/tests/compat_report.rs | VERIFIED | Exact real-repository Rust product baseline passes. |
| crates/cli/tests/headless.rs | VERIFIED | Exact real package dev/start/bin contract passes. |
| crates/cli/tests/state_authority.rs | VERIFIED | 3/3 filesystem tests prove .minimax-only writes and read-only legacy migration input. |
| test/ci-contract.test.ts | VERIFIED | Transitional synchronization asserts exact dev/start and retained test/eval routes; its sole refreshed manifest hash matches. |
| fixtures/compat/source-authority.v1.json | VERIFIED | Exact source/JS inventories and all 199 pinned hashes independently match. |
| bin and scripts/release | VERIFIED | Unchanged by gap closure; reviewed JS hashes, fixed launcher mapping, and identity contracts remain valid. |
| .github/workflows/ci.yml | VERIFIED | Unchanged by gap closure; current mutation/order test passes. |

The 10-04 declared artifact query passed 5/5 with no substance issues.

## Key Link Verification

| From | To | Via | Status |
|---|---|---|---|
| package.json | crates/cli | dev invokes cargo run -p minimax-cli --locked -- with npm argv forwarding | WIRED AND EXERCISED |
| package.json | bin/minimax-codex.cjs | start and the sole package bin use the fixed Rust launcher | WIRED |
| source_authority.rs | package.json | validate_executable_links calls validate_package_product_scripts | WIRED AND EXERCISED |
| baseline.rs | source_authority.rs | validate_product_entry reuses the shared script policy | WIRED AND EXERCISED |
| main.rs | source_authority.rs | verify_repository validates source authority before compatibility loading for verify and verify-candidate | WIRED AND EXERCISED |
| test/ci-contract.test.ts | source-authority.v1.json | exact reviewed SHA-256 entry | WIRED |
| state-authority manifest | Rust state/migration tests | .minimax read-write and .mini-codex read-only migration input | WIRED AND EXERCISED |

The 10-04 declared key-link query passed 4/4.

## Behavioral Spot-Checks

All Rust commands below used local 1.97.0-x86_64-pc-windows-gnullvm with rust-lld and target/gnullvm-dev. This is development-only evidence.

| Check | Result |
|---|---|
| cargo test -p minimax-compat-harness --test source_authority --locked | 10 passed, including product-script mutation matrix, exact repository inventories, shared preflight, CI ordering, and second-root rejection |
| exact compat_report Rust product-baseline test | 1 passed |
| exact headless npm product-entry test | 1 passed |
| cargo test -p minimax-cli --test state_authority --locked | 3 passed |
| cargo test -p minimax-cli --test product_identity --locked | 3 passed |
| npm run dev -- --version | exit 0; executed target/gnullvm-dev/debug/minimax-cli.exe and reported minimax-codex-rust 0.1.0 |
| cargo run -p minimax-compat-harness --locked -- verify-candidate | exit 0 |
| independent manifest/source/hash comparison | 191/191 TS, 8/8 JS, 199 hashes checked, zero drift, 9/9 Rust roots present |
| 10-04 artifact and key-link queries | artifacts 5/5; links 4/4 |

The verifier did not repeat the full npm suite. Re-verification used the focused behavioral contracts above plus direct current-tree inventory and wiring evidence; no executor summary claim was counted as proof.

No phase probe files exist. No hosted workflow, network/Provider call, package install/publication, push, model download, or hosted fingerprint refresh was performed.

## Requirements Coverage

| Requirement | Status | Evidence |
|---|---|---|
| RUST-01 - Rust is the only executable product implementation | SATISFIED | dev, start, bin, and equivalent package routes are Rust-only and guarded by both authority preflights. |
| RUST-02 - JavaScript is restricted to reviewed distribution orchestration | SATISFIED | Exact 5-file allowlist, 8 total tracked JS paths, zero drift, and negative authority checks pass. |
| RUST-03 - .minimax is the only writable runtime authority | SATISFIED | Legacy writer route is closed; .mini-codex is read-only migration input; route/state negatives and 3/3 filesystem tests pass. |

All Phase 10 requirements are covered by plans and current evidence.

## Prohibition Verification

| Prohibition | Status | Evidence |
|---|---|---|
| Do not leave a package script that starts the TypeScript/legacy product or its .mini-codex writer | VERIFIED | Exact Cargo dev route plus adversarial product-script mutations and shared preflight. |
| Do not broadly ban transitional TypeScript verification before Phase 11 | VERIFIED | Positive source-authority fixture retains build/check/test/eval/smoke classes and current scripts remain present. |
| Do not delete or port the transitional TypeScript tree in the gap closure | VERIFIED | Gap diff changes one transitional contract test and one exact hash; 191 TS/TSX paths remain. |
| Do not refresh hosted fingerprints or present GNU-LLVM as hosted release evidence | VERIFIED | Hosted evidence/release/CI files are untouched; this report labels GNU-LLVM development-only. |
| Do not allow JavaScript product behavior, fallback, or runtime download | VERIFIED | Exact reviewed allowlist and launcher/source negatives remain green. |
| Do not write .mini-codex after cutover | VERIFIED | No package product route reaches the TS writer; Rust state/migration filesystem evidence passes. |

## Anti-Patterns and Human Verification

No blocker, warning, stub, placeholder, TODO, FIXME, XXX, or TBD marker was found in the 10-04 production/test surface. src/runtime/application-kernel.ts still contains the historical .mini-codex default as hash-pinned transitional source, but it is no longer reachable from a package product route; deletion remains Phase 14 scope.

No human verification is required. Both repaired truths and all behavior-dependent state/route invariants have current automated evidence.

## Final Assessment

Phase 10 now achieves its executable and writable authority cutover. The original TypeScript dev escape is replaced by the Rust CLI and guarded by both Rust-owned verification modes; consequently no supported or legacy package command reaches the transitional .mini-codex writer. The ten previously passing authority, inventory, launcher, package, identity, and CI truths remain regression-free.

---

_Verified: 2026-07-17T14:49:26Z_
_Verifier: the agent (gsd-verifier, re-verification mode)_
