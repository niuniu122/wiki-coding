---
phase: MMX-11-rust-verification-and-evaluation-authority
plan: "05"
subsystem: rust-verification-authority
tags: [rust, coverage, semantic-contracts, retry, durability, retirement-review]
requires:
  - phase: MMX-11-rust-verification-and-evaluation-authority
    provides: Phase 10 source inventory plus Plans 11-01 through 11-04 Rust evaluation authority
provides:
  - 44 closed semantic evidence contracts covering all 101 TypeScript responsibility rows exactly once
  - fail-closed evidence-class, exact-owner, compatible-reuse, and reviewed-retirement validation
  - executed and replayed Rust evidence for distinct retry and continue outcomes
  - persisted Provider/model/protocol session-binding evidence across journal replay
affects: [phase-12-compatibility-migration, phase-14-typescript-removal, verification-audit]
tech-stack:
  added: []
  patterns: [semantic evidence closure, exact test ownership, structured retirement review, durable behavior evidence]
key-files:
  created:
    - .planning/phases/MMX-11-rust-verification-and-evaluation-authority/11-05-SUMMARY.md
  modified:
    - fixtures/compat/verification/typescript-responsibilities.v1.json
    - crates/compat-harness/src/coverage.rs
    - crates/compat-harness/tests/coverage.rs
    - crates/cli/tests/restart.rs
    - crates/core/tests/session_machine.rs
    - crates/tui/tests/command_render.rs
key-decisions:
  - "Use one top-level evidenceContracts registry with a strict bidirectional closure: every responsibility belongs to exactly one contract and each row repeats that contract's exact owner evidence."
  - "Permit owner reuse only within one contract/class/category and deny public or safety retirement even when a structured review is supplied."
  - "Keep parser recognition separate from executed outcome evidence: TUI owns parsing while CLI restart owns durable retry/continue behavior."
patterns-established:
  - "Evidence contracts combine a stable ID, closed class, precise claim, exact owner, responsibility set, and optional source-complete retirement review."
  - "Coverage mutations fail when an existing Rust function is rebound to an incompatible responsibility."
requirements-completed: [RVE-01]
coverage:
  - id: D1
    description: "All 101 responsibility rows are assigned exactly once to 44 semantically specific evidence contracts with no missing, duplicate, orphaned, or mismatched owner."
    requirement: RVE-01
    verification:
      - kind: integration
        ref: "crates/compat-harness/tests/coverage.rs#semantic_audit_rejects_collapsed_unrelated_and_false_retirement_evidence"
        status: pass
      - kind: integration
        ref: "cargo run -p minimax-compat-harness --locked -- verify-candidate"
        status: pass
    human_judgment: false
  - id: D2
    description: "Continue and retry execute as distinct durable terminal turns with distinct request and turn identities, immutable retry source, and exact replay."
    requirement: RVE-01
    verification:
      - kind: integration
        ref: "crates/cli/tests/restart.rs#retry_and_continue_execute_distinct_durable_outcomes"
        status: pass
    human_judgment: false
  - id: D3
    description: "Public and safety responsibilities use behaviorally relevant Rust owners, while non-public retirement requires an exact structured review."
    requirement: RVE-01
    verification:
      - kind: integration
        ref: "crates/compat-harness/tests/coverage.rs#semantic_audit_rejects_collapsed_unrelated_and_false_retirement_evidence"
        status: pass
      - kind: integration
        ref: "cargo test -p minimax-core --test runtime_machine --test session_machine --test tool_machine --test compaction_trace --locked"
        status: pass
      - kind: integration
        ref: "cargo test -p minimax-provider --test config_credentials --test provider_fixtures --locked"
        status: pass
    human_judgment: false
duration: 1h 5min
completed: 2026-07-18
status: complete
---

# Phase 11 Plan 05: Semantic Responsibility Evidence Summary

**The compatibility matrix now has a fail-closed semantic contract for every responsibility, and retry/continue authority comes from executed durable Rust behavior rather than parser recognition.**

## Performance

- **Duration:** 1h 5min
- **Started:** 2026-07-17T17:47:00Z
- **Completed:** 2026-07-17T18:52:00Z
- **Tasks:** 2
- **Files created/modified:** 7
- **Execution mode:** Generic-agent workaround with the complete `gsd-executor` contract loaded.

## Accomplishments

- Evolved the responsibility matrix to schema version 2 with 44 semantic evidence contracts covering all 101 starting rows exactly once; read-only closure audit reports zero missing, duplicate, orphaned, or owner-mismatched assignments.
- Rebound the collapsed retrieval/capability rows to distinct lexical, evaluation, catalog/policy, command/dispatch, hybrid, corpus-integrity, and snapshot/refresh owners.
- Reclassified the named agent/kernel, permission/tool/budget/fail-closed, model/profile/credential, and summary/redaction responsibilities as Rust-covered and restricted retirement to exact source-complete non-public reviews.
- Added executed retry-versus-continue evidence proving distinct request/turn IDs, terminal outcomes, immutable retry source, and persisted replay.
- Added an independent session-machine contract proving the selected Provider/model/protocol binding populates `TurnRequest` and survives journal replay.

## Task Commits

1. **Task 1 RED: Reject collapsed mappings, parser aliases, and false retirements** - `75a1777` (test)
2. **Task 2 GREEN: Audit all responsibilities and bind exact behavioral owners** - `b59facf` (feat)

The summary and planning trackers are committed separately in the final metadata commit.

## Files Created/Modified

- `fixtures/compat/verification/typescript-responsibilities.v1.json` - Schema-v2 contract registry, exact owners, audited dispositions, and structured retirement reviews.
- `crates/compat-harness/src/coverage.rs` - Closed evidence classes plus bidirectional assignment, category/disposition, exact-owner reuse, and retirement validation.
- `crates/compat-harness/tests/coverage.rs` - RED family audit, closure assertions, incompatible-owner mutation, and schema-v2 regressions.
- `crates/cli/tests/restart.rs` - Executed durable retry-versus-continue outcome test.
- `crates/core/tests/session_machine.rs` - Provider/model/protocol binding and replay persistence test.
- `crates/tui/tests/command_render.rs` - Parser matrix assertion rebound to the parser/rendering responsibility rather than the behavior-outcome responsibility.
- `.planning/phases/MMX-11-rust-verification-and-evaluation-authority/11-05-SUMMARY.md` - Result, evidence, deviations, and handoff.

## Decisions Made

- The contract registry is top-level rather than duplicated inline on every row, but validation enforces a strict double bijection: all 101 row IDs appear exactly once, every contract points only to existing rows, and row evidence must equal its contract's exact evidence.
- Multiple historical sources may share one owner only when they intentionally share the same contract, evidence class, and category; an owner reused by a different semantic contract fails closed.
- A reviewed retirement requires a closed status enum, exact source set, specific absent outcome, and reason; public and safety responsibility IDs remain denylisted from retirement.
- Parser recognition remains parser-only evidence. The retry/continue responsibility cites the CLI runtime test that actually executes, persists, and replays both outcomes.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing Critical] Added direct model-binding replay evidence**
- **Found during:** Task 2 evidence audit
- **Issue:** Existing Provider/config tests proved selection and protocol fixtures, but no exact test proved that the selected Provider/model/protocol binding was persisted in session state, copied into `TurnRequest`, and retained after journal replay.
- **Fix:** Added one session-machine test at the durable state boundary instead of creating a filename-parity port.
- **Files modified:** `crates/core/tests/session_machine.rs`
- **Verification:** `session_binding_and_turn_request_model_identity_survive_replay` passed alone and in the full core suite.
- **Committed in:** `b59facf`

**2. [Rule 1 - Contract consistency] Rebound the TUI self-check to parser ownership**
- **Found during:** Task 2 TUI verification
- **Issue:** The TUI parser test still asserted that the retry/continue behavior-outcome row cited the parser itself, contradicting the repaired semantic boundary.
- **Fix:** Kept all parser assertions and changed only the matrix responsibility ID to the existing chat-input parser/rendering responsibility.
- **Files modified:** `crates/tui/tests/command_render.rs`
- **Verification:** TUI command-render suite passed 7/7; retry/continue behavior remained independently green in CLI restart.
- **Committed in:** `b59facf`

---

**Total deviations:** 2 auto-fixed (1 missing-critical evidence gap, 1 contract-consistency correction).
**Impact on plan:** Both changes strengthen the requested semantic ownership boundary; neither adds product behavior, filename-parity ports, evaluator changes, or external dependencies.

## Issues Encountered

- The installed default Windows Rust linker path was unavailable. All Rust checks were rerun offline with the already-installed `1.97.0-x86_64-pc-windows-gnullvm` toolchain, bundled `rust-lld`, and isolated `target/phase11-05-gnullvm`; no repository toolchain setting or release evidence changed.
- The first focused coverage GREEN run exposed two test-adaptation errors: the unknown-field mutation still targeted schema version 1, and expected semantic contract IDs did not match the committed registry names. Both were corrected without weakening the assertions; coverage then passed 7/7.

## Validation Results

- Task 1 inverse RED command - failed as required and named the shared lexical owner, parser-only retry/continue evidence, false-retirement families, missing semantic contracts, and incompatible owner acceptance.
- `cargo fmt --all -- --check` - passed.
- `cargo test -p minimax-compat-harness --test coverage --locked` - 7 passed.
- `cargo test -p minimax-cli --test restart --test index_commands --test tool_loop --locked` - 18 passed.
- `cargo test -p minimax-core --test runtime_machine --test session_machine --test tool_machine --test compaction_trace --locked` - 27 passed.
- `cargo test -p minimax-provider --test config_credentials --test provider_fixtures --locked` - 8 passed.
- `cargo test -p minimax-retrieval --test lexical --test project_discovery --test capability_workspace --locked` - 13 passed.
- `cargo test -p minimax-tui --test command_render --locked` - 7 passed.
- Exact headless route and both embedding-resource/fail-closed owners referenced by newly rebound rows - 3 passed.
- `cargo run -p minimax-compat-harness --locked -- verify-candidate` - passed.
- Read-only JSON closure audit - 44 contracts, 101 rows, 101 assignments, zero missing, duplicate, orphaned, or evidence-mismatched rows.
- `git diff --check` - passed before both task commits.

All Rust results above are local development evidence only. No network, hosted runner, Provider, credential, model download, publication, package release, fingerprint refresh, or release-evidence refresh was used or claimed.

## Known Stubs

None. The semantic contracts reference executable Rust tests or explicit reviewed non-public retirement evidence; no placeholder, fallback, or permissive unknown path was introduced.

## Threat Surface

No runtime threat surface was added. The changes are verification schema/tests only, strengthen fail-closed handling for permissions, credentials, budgets, resources, and retirement, and add no endpoint, secret access, subprocess authority, writable root, network path, or model-loading path.

## User Setup Required

None - verification remains deterministic, local, offline, credential-free, Provider-free, and model-free.

## Next Phase Readiness

- Plan 11-06 can audit final evaluator authority without the RVE-01 semantic-evidence gap.
- Phase 12 can consume the exact contract registry when enforcing compatibility migration gates.
- Phase 14 can remove transitional TypeScript sources against explicit per-responsibility Rust ownership and reviewed non-public retirements.

## Self-Check: PASSED

- RED commit `75a1777` and GREEN commit `b59facf` both exist.
- The audited matrix parses as schema version 2 and closes 44 contracts over all 101 rows exactly once.
- Every planned Rust suite, the exact newly cited owners, formatting, candidate verification, and diff checks pass.
- Provider/retrieval evaluator authority and TypeScript evaluator discovery were not changed.
- No hosted fingerprint, release evidence, package artifact, external Provider, credential, model, download, network resource, push, publication, or PR was used or changed.

---
*Phase: MMX-11-rust-verification-and-evaluation-authority*
*Completed: 2026-07-18*
