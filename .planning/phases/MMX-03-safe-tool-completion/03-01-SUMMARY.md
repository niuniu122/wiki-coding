---
phase: MMX-03-safe-tool-completion
plan: "01"
subsystem: tool-runtime
tags: [rust, tools, approval, recovery, responses, chat-completions, jsonl]
requires:
  - phase: MMX-02-usable-rust-agent-shell
    provides: provider-neutral streaming, durable sessions, recovery, and shared CLI driver
provides:
  - strict Provider-neutral native tool conversation records
  - exactly-two-mode approval and invocation state machines
  - durable request-decision-start-terminal journal boundaries
  - idempotent cancelled/indeterminate restart recovery
  - bounded serial model-tool-model coordinator for both Provider protocols
affects: [tool-adapters, cli-commands, tui-approval, compatibility, vault-runtime]
tech-stack:
  added: []
  patterns: [typed conversation items, persist-before-effect, serial tool batches, stable recovery identity, process-local permission]
key-files:
  created:
    - crates/protocol/src/tool.rs
    - crates/core/src/tool.rs
    - crates/cli/tests/tool_loop.rs
    - fixtures/compat/tools/provider-roundtrip.v1.json
  modified:
    - crates/provider/src/responses.rs
    - crates/provider/src/chat_completions.rs
    - crates/core/src/runtime.rs
    - crates/core/src/session.rs
    - crates/vault/src/runtime/recovery.rs
    - crates/cli/src/driver.rs
key-decisions:
  - "Provider tool calls and results remain native typed conversation items; no tool envelope is flattened into ordinary prompt text."
  - "Full access synthesizes only a normal approved decision after preflight; it is process-local, uses the same reducer, and serializes no active permission mode."
  - "A crash before started recovers cancelled; a crash after started recovers indeterminate, and neither path automatically executes the adapter."
patterns-established:
  - "Every invocation persists request, decision, started, and terminal facts before the next dependent action."
  - "All calls in one Provider batch retain order and execute serially before their durable results enter the next Provider request."
requirements-completed: [TOOL-01, TOOL-02, TOOL-03]
requirements-progressed: [TOOL-05]
coverage:
  - id: D1
    description: Responses and Chat Completions preserve native call/result identity and ordered multi-call history across two Provider rounds.
    requirement: TOOL-01
    verification:
      - kind: integration
        ref: "crates/cli/tests/tool_loop.rs#confirm_mode_preserves_order_ids_and_durability_for_both_provider_protocols"
        status: pass
      - kind: integration
        ref: "crates/provider/tests/provider_fixtures.rs"
        status: pass
    human_judgment: false
  - id: D2
    description: Confirm requires one explicit matching decision; rejection, unavailable approval, and pre-start cancellation execute zero adapters.
    requirement: TOOL-02
    verification:
      - kind: integration
        ref: "crates/cli/tests/tool_loop.rs#unavailable_approval_returns_rejection_and_invokes_zero_tools"
        status: pass
      - kind: integration
        ref: "crates/cli/tests/tool_loop.rs#cancellation_during_confirmation_persists_cancelled_and_executes_nothing"
        status: pass
    human_judgment: false
  - id: D3
    description: Full access skips only the prompt, still preflights, and leaves no active permission mode in journal or index bytes.
    requirement: TOOL-03
    verification:
      - kind: integration
        ref: "crates/cli/tests/tool_loop.rs#full_access_skips_prompt_but_still_preflights_persists_and_executes"
        status: pass
      - kind: unit
        ref: "crates/core/tests/tool_machine.rs#permission_mode_has_only_confirm_and_full_access_and_defaults_to_confirm"
        status: pass
    human_judgment: false
  - id: D4
    description: Restart converges requested/approved work to cancelled and started work to one indeterminate result without replay.
    requirement: TOOL-05
    verification:
      - kind: integration
        ref: "crates/vault/tests/runtime_store.rs#approved_but_not_started_tool_recovers_cancelled_once_without_execution"
        status: pass
      - kind: integration
        ref: "crates/vault/tests/runtime_store.rs#started_tool_recovers_indeterminate_once_and_never_claims_success"
        status: pass
    human_judgment: false
duration: 73min
completed: 2026-07-15
status: complete
---

# Phase 3 Plan 1: Native Tool Runtime and Durable Recovery Summary

**Rust now completes a bounded native model-tool-model loop whose approvals, side-effect boundary, results, and restart outcome remain auditable and truthful.**

## Performance

- **Duration:** 73 min
- **Started:** 2026-07-15T12:42:25Z
- **Completed:** 2026-07-15T13:55:22Z
- **Tasks:** 3
- **Files modified:** 37

## Accomplishments

- Added strict schema-v1 tool definitions, calls, invocations, decisions, results, effects, and complete assistant call batches with bounded validation.
- Mapped the shared conversation natively to Responses function calls/outputs and Chat Completions assistant tool calls/tool messages while preserving exact call IDs and order.
- Added a pure invocation reducer, exactly `confirm` and `full-access`, serial batch execution, cancellation semantics, and finite Provider-round/tool-call/elapsed/result-byte budgets.
- Added append-synced invocation facts and stable startup recovery that never retries an unknown side effect or fabricates success.
- Added a scripted CLI coordinator proving rejection, hard-gate denial, both permission modes, both Provider protocols, budget exhaustion, cancellation, durable-before-next-round ordering, and restart idempotence.

## Task Commits

1. **Task 1: Define strict native tool conversation contracts and Provider mappings** - `46d588c`
2. **Task 2: Implement the invocation reducer, budgets, and exactly two permission modes** - `eec7cf0`
3. **Task 3: Persist invocation boundaries, recover uncertainty, and compose the bounded Provider loop** - `b0a20bd`

## Files Created/Modified

- `crates/protocol/src/tool.rs` - strict tool schema, call, decision, result, and validation contracts.
- `crates/protocol/src/runtime.rs` - typed conversation history, tools, finite limits, and budget terminal classification.
- `crates/provider/src/responses.rs` - native Responses function definitions, calls, and outputs.
- `crates/provider/src/chat_completions.rs` - native Chat assistant tool calls and matching tool results.
- `crates/core/src/tool.rs` - two permission modes, invocation reducer, uniqueness registry, and finite budgets.
- `crates/core/src/runtime.rs` - complete ordered call-batch pause/resume behavior.
- `crates/protocol/src/session.rs` - request, decision, started, and terminal journal records plus projection.
- `crates/vault/src/runtime/recovery.rs` - stable cancelled/indeterminate abandoned-invocation recovery.
- `crates/cli/src/driver.rs` - injected approval/tool ports and serial multi-round coordinator.
- `crates/cli/tests/tool_loop.rs` - offline two-protocol, two-mode, ordering, cancellation, and budget proof.

## Decisions Made

- Kept approval and tool adapters behind core-owned ports; core still imports no Provider, filesystem, process, terminal, or Vault implementation.
- Used a synthesized `policy_approved` decision for full access so the normal decision/start path is retained without persisting the active permission name.
- Counted elapsed budget from real wall time in the production driver while retaining deterministic core budget tests.
- Treated invalid adapter IDs, names, codes, or oversized results as bounded typed failures rather than leaving an invocation half-finished.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Moved the tool-call budget gate before the durable started boundary**

- **Found during:** Task 3 coordinator review
- **Issue:** Consuming the tool budget inside `Execute` could persist `started` even when no adapter call was allowed.
- **Fix:** Added a pre-start typed failure transition; budget exhaustion now persists a terminal failure without `started` or `Execute`.
- **Files modified:** `crates/core/src/tool.rs`, `crates/cli/src/driver.rs`, `crates/core/tests/tool_machine.rs`
- **Verification:** `a_pre_start_budget_failure_is_terminal_without_started_or_execute_effects` passes.
- **Committed in:** `b0a20bd`

**2. [Rule 1 - Bug] Distinguished identical physical journal replay from conflicting reuse**

- **Found during:** Task 3 restart acceptance review
- **Issue:** The session reducer allowed identical record replay, but the journal loader rejected every repeated record ID before comparing payload identity.
- **Fix:** The loader now accepts byte-equivalent logical replay and rejects the same ID with a different payload.
- **Files modified:** `crates/vault/src/runtime/journal.rs`, `crates/vault/tests/runtime_store.rs`
- **Verification:** Physical identical replay opens successfully; conflicting replay fails recovery.
- **Committed in:** `b0a20bd`

**3. [Rule 3 - Blocking] Expanded the TypeScript compatibility validator for the new strict negative fixtures**

- **Found during:** Full baseline verification
- **Issue:** Phase 3 added invalid-object-arguments and duplicate-call-ID fixtures, while the locked validator still required the old five-case list exactly.
- **Fix:** Extended the expected negative baseline to the seven intentional cases; command, Provider, permission, and product-entry baselines were unchanged.
- **Files modified:** `test/rust-rewrite-compat-manifest.test.ts`
- **Verification:** TypeScript baseline returned to 432/432.
- **Committed in:** `b0a20bd`

**Total deviations:** 3 auto-fixed (2 correctness bugs, 1 verification blocker). **Impact:** No new product scope, dependency, external request, permission mode, or storage authority was introduced.

## Issues Encountered

- The unsupported local Windows gnullvm linker remains slower during full test linking; the pinned toolchain nevertheless completed all offline gates.

## User Setup Required

None. The plan used scripted Providers and fake approval/tool ports; no credential, Provider spend, embedding download, SQLite, destructive migration, or npm product-entry change occurred.

## Next Phase Readiness

- The runtime transaction and recovery substrate is ready for Plan 03-02's bounded read/list, patch/write, process, Git, and npm adapters.
- Plan 03-02 must attach the concrete hard gates and CLI/TUI approval workflow before TOOL-04 and TOOL-05 can be completed.

## Self-Check: PASSED

- Rust workspace: 111/111 tests passed; formatting and workspace Clippy with `-D warnings` passed.
- TypeScript reference: type checking and build passed; 432/432 tests passed.
- Retrieval evaluation: 175 cases passed; Provider conformance passed all 8 checks for each protocol.
- Rust compatibility verifier and `git diff --check` passed.
- No real Provider, credential, model download, SQLite, deletion, migration, PR, merge, or npm-entry cutover was used.

---
*Phase: MMX-03-safe-tool-completion*
*Completed: 2026-07-15*
