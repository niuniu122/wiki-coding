---
phase: MMX-11-rust-verification-and-evaluation-authority
plan: "02"
subsystem: provider-evaluation-authority
tags: [rust, provider, fixtures, deterministic-report, compatibility, retrieval-corpus]
requires:
  - phase: MMX-11-rust-verification-and-evaluation-authority
    provides: Exact TypeScript responsibility matrix and fail-closed Rust coverage validation from Plan 11-01
provides:
  - fixture-only Rust Provider evaluation for Responses and Chat Completions
  - strict fingerprinted manifest and byte-stable machine-readable golden report
  - Rust-authoritative provider-eval command wired into repository verification and npm convenience execution
  - immutable 175-case retrieval corpus with stable IDs, locked thresholds, and source equivalence proof
affects: [phase-11-retrieval-evaluation, phase-11-ci-authority, phase-14-typescript-removal]
tech-stack:
  added: []
  patterns: [strict serde evaluation manifests, production replay reuse, fingerprinted golden reports, Rust-failure-first release gates]
key-files:
  created:
    - crates/compat-harness/src/provider_eval.rs
    - crates/compat-harness/tests/provider_eval.rs
    - fixtures/compat/evaluations/provider.v1.json
    - fixtures/compat/evaluations/provider-report.expected.json
    - fixtures/compat/retrieval/capability-cases-expanded.v1.json
  modified:
    - crates/compat-harness/src/lib.rs
    - crates/compat-harness/src/main.rs
    - fixtures/compat/verification/typescript-responsibilities.v1.json
    - package.json
key-decisions:
  - "Provider evaluation fingerprints the immutable protocol fixtures and provider profile manifest, then runs only production replay/config/profile APIs without HTTP or credential resolution."
  - "Each protocol publishes the same ordered ten-check contract, and repository verification requires both a passing report and byte-identical committed golden."
  - "A passing transitional package smoke cannot authorize release when any Rust Provider check fails."
  - "The expanded retrieval corpus is immutable compatibility input with 175 stable case IDs, locked thresholds, a deterministic corpus fingerprint, and an exact hash link to the retained TypeScript source."
patterns-established:
  - "Machine evaluation stdout is report JSON only; failed Rust checks make the command and verification non-zero."
  - "Historical evaluator responsibilities cite exact Rust test functions plus command/golden evidence rather than TypeScript execution."
requirements-completed: [RVE-02]
coverage:
  - id: D1
    description: "Responses and Chat Completions conformance runs entirely from immutable fixtures through production Rust replay and emits a deterministic 20-check machine report."
    requirement: RVE-02
    verification:
      - kind: integration
        ref: "crates/compat-harness/tests/provider_eval.rs#provider_evaluation_matches_committed_golden_and_is_repeatable"
        status: pass
      - kind: integration
        ref: "cargo run -p minimax-compat-harness --locked -- provider-eval --format json"
        status: pass
    human_judgment: false
  - id: D2
    description: "Rust Provider failure remains release-blocking regardless of package smoke, while credential-bearing environment variables cannot influence report bytes."
    requirement: RVE-02
    verification:
      - kind: integration
        ref: "crates/compat-harness/tests/provider_eval.rs#provider_failure_blocks_release_even_when_package_smoke_succeeds"
        status: pass
      - kind: integration
        ref: "crates/compat-harness/tests/provider_eval.rs#provider_eval_command_is_json_only_deterministic_and_credential_independent"
        status: pass
      - kind: integration
        ref: "cargo run -p minimax-compat-harness --locked -- verify-candidate"
        status: pass
    human_judgment: false
  - id: D3
    description: "The immutable retrieval corpus preserves all 175 transitional queries, expected IDs, no-match labels, and thresholds while adding stable IDs and a deterministic fingerprint."
    requirement: RVE-02
    verification:
      - kind: integration
        ref: "crates/compat-harness/tests/provider_eval.rs#immutable_retrieval_corpus_preserves_transitional_175_case_contract"
        status: pass
      - kind: integration
        ref: "crates/retrieval/tests/lexical.rs#existing_typescript_175_case_fixture_meets_capability_gates"
        status: pass
    human_judgment: false
duration: 45min
completed: 2026-07-17
status: complete
---

# Phase 11 Plan 02: Rust Provider Evaluation Summary

**A strict fixture-only Rust evaluator now produces the release-blocking 20-check Provider report for both supported protocols, with immutable retrieval-corpus ownership transferred ahead of Plan 11-03.**

## Performance

- **Duration:** 45 min
- **Started:** 2026-07-17T15:10:00Z
- **Completed:** 2026-07-17T15:55:00Z
- **Tasks:** 2
- **Files created/modified:** 9
- **Execution mode:** Generic-agent workaround with the full `gsd-executor` contract loaded and followed.

## Accomplishments

- Added a strict fingerprinted Provider evaluation manifest and byte-stable golden with ten required checks for each of Responses and Chat Completions.
- Reused `minimax_provider::replay_fixture` plus the existing provider manifest/config validation instead of introducing another parser, transport, credential path, or Provider implementation.
- Exposed `provider-eval --format json`, made both repository verification modes require the evaluator and golden, and changed `npm run eval:provider` into a Rust-only convenience alias.
- Proved secret-bearing Provider environment variables do not affect report bytes and that a simulated successful package smoke cannot override a failed Rust check.
- Transferred the 175-case mixed-language retrieval source into immutable compatibility ownership with strict metadata, stable case IDs, locked thresholds, deterministic fingerprinting, and byte-preserving source retention until 14-01.

## Task Commits

Each task was committed atomically in TDD order:

1. **Task 1 RED: add failing Provider evaluation contract tests** - `88d4bdb` (test)
2. **Task 1 GREEN: add deterministic Rust Provider evaluator** - `499d0e3` (feat)
3. **Task 2 RED: add failing Provider authority integration tests** - `163eb33` (test)
4. **Task 2 GREEN: make Rust Provider evaluation authoritative** - `7e01aec` (feat)
5. **Task 2 validation correction: close metadata validation gaps** - `2f351cf` (fix)

The summary and planning trackers are committed together in the final plan metadata commit.

## Files Created/Modified

- `crates/compat-harness/src/provider_eval.rs` - Strict manifest loading, fixture hashing, production replay checks, typed report serialization, golden validation, and release authority.
- `crates/compat-harness/src/lib.rs` - Public Provider evaluation/report API exports.
- `crates/compat-harness/src/main.rs` - `provider-eval --format json` dispatch and fail-closed verification wiring.
- `crates/compat-harness/tests/provider_eval.rs` - Golden, mutation, environment-negative, package-non-authority, CLI, and retrieval-transfer tests.
- `fixtures/compat/evaluations/provider.v1.json` - Closed protocol/check declaration with exact fixture hashes.
- `fixtures/compat/evaluations/provider-report.expected.json` - Stable 20/20 machine-readable golden.
- `fixtures/compat/retrieval/capability-cases-expanded.v1.json` - Immutable 175-case corpus with source SHA-256, stable IDs, thresholds, and fingerprint.
- `fixtures/compat/verification/typescript-responsibilities.v1.json` - Exact Rust evaluator and immutable retrieval ownership dispositions.
- `package.json` - Rust-only `eval:provider` convenience command.

## Decisions Made

- The evaluator treats fixture and provider-manifest bytes as review inputs. Any hash drift is an error before checks run, and any report drift is an error in repository verification.
- Valid text, usage, terminal ordering, native tool identity/order, malformed and premature rejection, safe error codes, redaction, unsupported feature policy, and deterministic replay are the stable per-protocol check IDs.
- The command prints a report even when a behavioral check fails and exits non-zero; structural input failures fail before a trustworthy report can be constructed.
- The old retrieval fixture remains present and unchanged at SHA-256 `fd60690849c25a6aca7bb7fc074f724303754006cb5a4075a174f8090e76ba22`; reviewed deletion remains owned by 14-01.

## Deviations from Plan

None - plan executed within 11-02 scope. The final metadata-validation correction tightened explicit strictness and evidence already required by Task 2.

## Issues Encountered

- The first GREEN compile exposed a partial-move ordering error while constructing the report; the fingerprint is now computed before moving the evaluation ID.
- JSON emitted the exact `idValidity` threshold as integer `1`; the equivalence test was aligned to that semantically exact locked value before Task 2 completion.
- The installed GSD progress updater reported 77% but left stale 71% frontmatter and did not mark the descriptive 11-02 roadmap checkbox; both tracker fields were aligned to the updater's 34/44 result before the docs commit.

## Verification

- `cargo fmt --all -- --check` - passed.
- `cargo test -p minimax-provider --locked` - 19 unit/integration tests passed; doc tests passed.
- `cargo test -p minimax-compat-harness --test provider_eval --test coverage --locked` - 14 tests passed.
- `cargo test -p minimax-retrieval --test lexical --locked existing_typescript_175_case_fixture_meets_capability_gates -- --exact` - passed.
- `cargo run -p minimax-compat-harness --locked -- provider-eval --format json` - emitted deterministic 20/20 passing report.
- `npm run eval:provider` - passed through the Rust evaluator alias.
- `cargo run -p minimax-compat-harness --locked -- verify-candidate` - passed with Provider evaluation blocking before compatibility decisions.
- `cargo clippy -p minimax-compat-harness --all-targets --locked -- -D warnings` - passed.

All Rust development evidence used the already-installed `1.97.0-x86_64-pc-windows-gnullvm` toolchain and bundled `rust-lld`. It is development evidence only and is not represented as Windows MSVC release or hosted evidence.

## Known Stubs

None. The created and modified files contain no TODO, FIXME, placeholder, coming-soon, or unavailable implementation paths.

## Threat Surface

No unplanned threat surface was introduced. The evaluator reads only strict, repository-relative, fingerprinted fixture paths declared by the plan; it adds no endpoint, HTTP client, credential lookup, keyring access, Provider call, or TypeScript execution path.

## User Setup Required

None - all work was local, deterministic, offline, credential-independent, and fixture-only.

## Next Phase Readiness

- Plan 11-03 can consume the immutable expanded corpus without relying on the transitional `test/` path as evaluation authority.
- Plan 11-04 can make both Rust evaluator reports final package/CI authority without changing the Provider report contract.
- TypeScript evaluator and source files remain hash-pinned in place for the later reviewed deletion; no source was deleted here.

## Self-Check: PASSED

- The canonical summary and all five created implementation/fixture artifacts exist.
- All five TDD/task-correction commits resolve in git history.
- The plan-wide formatting, Rust tests, evaluator command, npm alias, strict Clippy, and candidate verification gates pass.
- No tracked source was deleted, the transitional 175-case source is byte-unchanged, and no TypeScript evaluator/runtime was executed by the Rust authority path.

---
*Phase: MMX-11-rust-verification-and-evaluation-authority*
*Completed: 2026-07-17*
