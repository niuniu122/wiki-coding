---
phase: MMX-01-contract-foundation
plan: "01"
subsystem: compatibility
tags: [fixtures, typescript, provider-protocol, contracts]
requires: []
provides:
  - Versioned command, provider, and compatibility-status manifests
  - Language-neutral valid and invalid Provider stream fixtures
  - TypeScript enforcement for uniqueness, evidence, coverage, and secret safety
affects: [MMX-01-contract-foundation, protocol, provider, compat-harness]
tech-stack:
  added: []
  patterns: [language-neutral fixtures, evidence-gated parity]
key-files:
  created:
    - fixtures/compat/commands.v1.json
    - fixtures/compat/providers.v1.json
    - fixtures/compat/provider-streams/invalid-cases.v1.json
    - test/rust-rewrite-compat-manifest.test.ts
  modified: []
key-decisions:
  - "Command compatibility is measured over canonical names plus aliases, so /quit remains an alias of /exit without becoming a second behavior implementation."
  - "Every Rust product behavior begins pending and can only become matched after executable Rust evidence exists."
patterns-established:
  - "Parity state is explicit: matched requires evidence; pending never implies implementation."
  - "Cross-language Provider cases contain normalized expected events and no real endpoint or credential."
requirements-completed: [COMP-01, COMP-02, COMP-03]
coverage:
  - id: D1
    description: "Locked slash commands and Provider profile classes are stored in versioned, secret-free manifests."
    requirement: COMP-01
    verification:
      - kind: integration
        ref: "test/rust-rewrite-compat-manifest.test.ts#Rust rewrite compatibility fixtures preserve the locked public baseline"
        status: pass
    human_judgment: false
  - id: D2
    description: "Responses and Chat Completions share deterministic valid and invalid protocol fixtures."
    requirement: COMP-02
    verification:
      - kind: integration
        ref: "test/rust-rewrite-compat-manifest.test.ts#Rust rewrite compatibility fixtures preserve the locked public baseline"
        status: pass
    human_judgment: false
  - id: D3
    description: "The existing TypeScript suite rejects duplicate commands, secret-like values, incomplete stream coverage, and unsupported parity claims."
    requirement: COMP-03
    verification:
      - kind: integration
        ref: "npm test"
        status: pass
      - kind: other
        ref: "npm run check"
        status: pass
    human_judgment: false
duration: 7min
completed: 2026-07-15
status: complete
---

# Phase 1 Plan 1: Compatibility Baseline Summary

**Executable, secret-free command and Provider fixtures now define what the Rust rewrite must preserve before it can claim parity.**

## Performance

- **Duration:** 7 min
- **Started:** 2026-07-15T16:20:45+08:00
- **Completed:** 2026-07-15T08:27:06Z
- **Tasks:** 3
- **Files modified:** 7

## Accomplishments

- Inventoried the locked slash-command surface, aliases, argument shapes, Provider classes, supported protocols, feature matrix, and target two-mode permission surface.
- Added language-neutral Responses and Chat Completions cases for visible text, filtered reasoning, usage, stable tool identity, completion, and five protocol failures.
- Added auto-discovered TypeScript validation that prevents duplicate identities, secret material, incomplete protocol coverage, and evidence-free `matched` claims.

## Task Commits

1. **Task 1: Freeze public command and provider manifests** - `631ce79`
2. **Task 2: Convert provider streams into shared fixtures** - `15b718d`
3. **Task 3: Enforce manifests from the TypeScript suite** - `0c4fe54`
4. **Task 3 hardening: Require complete event coverage** - `58d13f7`

## Files Created/Modified

- `fixtures/compat/commands.v1.json` - Public commands, aliases, argument shapes, outcomes, and the two target permission modes.
- `fixtures/compat/providers.v1.json` - Provider classes, protocols, credential binding names, and feature support.
- `fixtures/compat/baseline-status.v1.json` - Honest TypeScript evidence and pending Rust parity state.
- `fixtures/compat/provider-streams/responses.valid.jsonl` - Responses visible/tool/terminal fixture sequences.
- `fixtures/compat/provider-streams/chat-completions.valid.jsonl` - Chat Completions visible/tool/terminal fixture sequences.
- `fixtures/compat/provider-streams/invalid-cases.v1.json` - Typed malformed, identity, EOF, and terminal-order failures.
- `test/rust-rewrite-compat-manifest.test.ts` - Cross-language baseline validation and negative tests.

## Decisions Made

- Kept `/quit` as an alias of `/exit`; the compatibility set is the union of canonical names and aliases.
- Recorded only binding names such as `MINIMAX_API_KEY`, never values or live endpoints.
- Left all Rust product items pending until later plans attach executable Rust evidence.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing Critical] Made valid-stream coverage an executable invariant**

- **Found during:** Final plan verification
- **Issue:** The first validator version proved stream records were non-empty but did not require all five promised normalized event categories for both protocols.
- **Fix:** Added per-protocol assertions for visible text, filtered reasoning, usage, stable tool identity, and completion.
- **Files modified:** `test/rust-rewrite-compat-manifest.test.ts`
- **Verification:** Targeted 4-test suite and TypeScript typecheck pass.
- **Committed in:** `58d13f7`

**Total deviations:** 1 auto-fixed (1 missing critical verification invariant).
**Impact:** Stronger enforcement only; no product behavior or architecture changed.

## Issues Encountered

- The first full `npm test` run hit one pre-existing Windows file-lock race in `workspace-lease.test.ts` (`EPERM` during concurrent stale rename). The new targeted suite passed, and two subsequent full runs passed all 432 tests without changing the unrelated lease code.

## User Setup Required

None - all fixtures and checks are offline.

## Next Phase Readiness

- The language-neutral baseline is ready for the Rust workspace and fixture consumers in Plans 01-02 and 01-03.
- No Rust behavior has been promoted to matched.

## Self-Check: PASSED

- All seven claimed files exist.
- All four task/hardening commits exist.
- `npm run check`, targeted compatibility tests, two clean full `npm test` runs, and `git diff --check` pass.

---
*Phase: MMX-01-contract-foundation*
*Completed: 2026-07-15*
