# Phase 3: Safe Tool Completion — Specification

**Created:** 2026-07-15
**Ambiguity score:** 0.04 (gate: <= 0.20)
**Requirements:** 5 locked

## Goal

The Rust agent completes every allowed model-requested tool call through a durable Provider round trip while exposing exactly two process-scoped permission modes and enforcing the same non-bypassable safety gates in both.

## Background

Phase 2 can stream and persist ordinary Rust conversations, and both Provider adapters already normalize incoming tool-call fragments with validated call IDs. The TUI recognizes `/agent`, `/continue`, and `/permissions`, but reports their Phase 3 behavior as unavailable. `crates/tools` is still a boundary-only stub, Provider requests carry text messages only, and the runtime currently terminates a turn as `tool_unavailable` after observing a tool call. No Rust approval state machine, durable tool-result record, Provider tool-result round trip, or concrete v1 tool adapter exists.

The TypeScript implementation supplies useful path, process, and persistence evidence, but it also contains a legacy third permission shape that is not part of the Rust product. Phase 3 replaces that gap with the already locked Rust contract: exactly `confirm` and process-scoped `full-access`, a finite five-category tool set, and hard gates that neither mode can disable.

## Requirements

1. **Durable Provider tool loop**: Each model tool request must complete a Provider-neutral, persist-before-progress state machine with one stable call ID and one terminal tool result.
   - Current: Provider streams yield validated `ToolCallFragment` values, but `RunMachine` maps them to `tool_unavailable`; request messages cannot yet carry tool definitions, assistant tool calls, or tool results.
   - Target: Tool definitions are advertised through the shared Provider request contract; complete calls become normalized durable requests; the coordinator records approval and execution state; a durable terminal result is appended before the next Provider round receives that result. Duplicate call IDs, duplicate terminals, illegal transitions, and replay after an execution may have started fail closed.
   - Acceptance: Offline Responses and Chat Completions fixtures each perform a two-round tool call with the same stable call ID; journal assertions prove request-before-decision-before-result-before-next-Provider ordering; duplicate IDs and restart from an in-flight side effect produce a typed failure or `indeterminate` result without a second execution.

2. **Confirm permission mode**: `confirm` must require an explicit decision before every external tool invocation, including read-only tools, and rejection must be a structured tool result.
   - Current: `/permissions` recognizes the name but Phase 2 exposes no mutable permission state or approval prompt, and no Rust adapter can execute.
   - Target: Every new process starts in `confirm`. The coordinator presents normalized tool name, arguments, and workspace scope; only an explicit approval advances that exact call to execution. Rejection, EOF, unavailable interactivity, conflicting decisions, or cancellation before execution produce a durable non-success result and never invoke the adapter.
   - Acceptance: Tests cover approve, reject, EOF/unavailable headless approval, duplicate approval, conflicting approval, and pre-execution cancellation; a spy adapter remains at zero calls for every non-approved case, while an approved call executes exactly once.

3. **Process-scoped full access**: `full-access` must skip prompts only for the fixed allowed tool set and only for the lifetime of the current process.
   - Current: The parser accepts `full-access`, but no runtime permission service exists and no state is changed.
   - Target: `/permissions full-access` changes only in-memory coordinator state; `/permissions confirm` changes it back; the state is never serialized to config, journal, Vault, environment, or credentials. Every newly constructed runtime is `confirm`, and changing the mode does not retroactively alter a call whose approval/execution transition has already been recorded.
   - Acceptance: Construction/restart tests always observe `confirm`; serialization and repository scans find no persistence field for the active mode; repeated mode changes are deterministic; a concurrent/in-flight call keeps the decision recorded for that call; unknown tools remain rejected in `full-access` without prompting or execution.

4. **Complete bounded Rust v1 adapters**: Rust v1 must expose strict schemas and bounded structured results for read/list, patch/write, bounded process execution, Git status/diff, and npm diagnostics.
   - Current: `crates/tools` contains only `CRATE_ROLE`; existing TypeScript read/list and npm diagnostic adapters are not reachable from Rust.
   - Target: The tools crate describes, validates, and executes the finite v1 set once per authorized invocation. Paths are workspace-relative and canonicalized; file reads/writes and combined process output default to 64 KiB, directory listings to 500 entries, and processes to 30 seconds. Patch/write is non-destructive, conflict-aware, and atomic; bounded process execution uses structured program/action arguments with `shell=false` and a finite diagnostic allowlist; Git exposes only status/diff; npm executes only an existing validated diagnostic script with no dependency installation.
   - Acceptance: Contract tests reject unknown fields/actions and cover empty, single, duplicate, binary, invalid UTF-8, oversized, timeout, nonzero, cancellation, symlink/reparse escape, and write-conflict cases; supported calls return stable structured outcomes; list order is deterministic; Provider multi-call execution preserves emitted call order.

5. **Identical hard gates and honest interruption**: Both permission modes must enforce the same schema, path, secret, destructive-operation, cancellation, and unknown-side-effect gates.
   - Current: TypeScript adapters enforce parts of this policy independently, but Rust has no common preflight gate or recovery representation.
   - Target: Approval is never a safety bypass. Unknown tools/fields, absolute or escaping paths, internal metadata targets, binary/invalid text, secret-like write material, destructive or interpreter-style process requests, and out-of-budget work are rejected before execution. Cancellation is checked before and during execution. If interruption occurs after an effect may have started but before a terminal result is durable, recovery records `indeterminate`, never success, and never automatically replays that call.
   - Acceptance: The same malicious/invalid fixture matrix produces byte-equivalent denial codes in `confirm` and `full-access`; secret/path/destructive fixtures prove the executor was not entered; interruption-at-each-boundary tests produce only the allowed durable state and never a fabricated success or automatic second execution.

## Boundaries

**In scope:**

- Provider-neutral tool definitions, assistant tool-call messages, and tool-result messages for both supported Provider protocols.
- A core-owned tool-call state machine, process-local permission state, approval port, budgets, cancellation, and durable recovery rules.
- Exactly `confirm` and `full-access`, exposed through `/permissions` with restart-to-confirm semantics.
- Rust adapters for workspace read/list, atomic patch/write, bounded diagnostic processes, Git status/diff, and npm diagnostics.
- TUI approval presentation, headless typed approval-required behavior, `/agent` and `/continue` execution, and mocked end-to-end Provider loops.
- Compatibility evidence and offline safety fixtures for all five `TOOL-*` requirements.

**Out of scope:**

- Arbitrary shell strings, shell interpreters, unrestricted executable launch, background processes, or daemon control — these cannot satisfy the shared hard-gate contract.
- File deletion, recursive move, Git mutation, package installation, network fetch, or arbitrary npm scripts — destructive or dependency-changing effects are not required by the v1 tool set.
- MCP, plugins, subagents, remote tool servers, and dynamically installed tools — deferred to v2 to avoid widening the capability boundary.
- An OS sandbox claim — Phase 3 provides application-level validation and bounded spawning only.
- Vault/Wiki mutation, retrieval/project discovery, migration, packaging, or npm-entry cutover — owned by Phases 4 through 6.
- A persisted permission preference or a third permission tier — explicitly excluded by the product contract.

## Constraints

- Rust stays pinned to 1.97.0/edition 2024 and supports Windows/MSVC and Linux; the local gnullvm target remains a development fallback, not a release target.
- Core owns the loop and depends on ports; tools only describe, validate, and execute one invocation; Provider adapters never decide policy; Vault never invokes a Provider.
- Public v1 schemas use `deny_unknown_fields`, validated non-empty IDs, stable snake_case wire values, and explicit size/count/time budgets.
- Default limits are 64 KiB per argument/result text budget, 500 directory entries, 30 seconds per child process, and serial tool execution within a run; configurable limits may only tighten or remain within validated project ceilings.
- Tests are offline, use fake Providers/process launchers/filesystems where needed, and never consume credentials, download packages/models, or mutate the TypeScript npm entry.
- `package.json` continues to point at `dist/cli.js` until Phase 6 release gates pass.

## Acceptance Criteria

- [ ] **AC-01:** Responses and Chat Completions fixtures each complete a native tool-call/result/continuation round trip with one unchanged `ToolCallId`.
- [ ] **AC-02:** Durable records prove `requested -> decision -> started -> terminal result -> next Provider request` ordering; pre-execution rejections omit `started`.
- [ ] **AC-03:** Duplicate IDs, duplicate decisions, duplicate terminals, and illegal transitions cannot execute or publish a second result.
- [ ] **AC-04:** A restart with an invocation that may have started but lacks a durable terminal result records `indeterminate` and never auto-replays it.
- [ ] **AC-05:** A fresh runtime exposes only `confirm`; `full-access` is in-memory, reversible, non-serialized, and limited to allowed tools.
- [ ] **AC-06:** In `confirm`, every external tool requires explicit approval; rejection, EOF, unavailable interaction, and pre-start cancellation execute zero adapters and return structured results.
- [ ] **AC-07:** Switching permission mode while a call is in flight does not change that call's already recorded decision.
- [ ] **AC-08:** Read/list accept only contained workspace-relative paths, reject binary/invalid UTF-8 and escapes, cap output at 64 KiB/500 entries, and return deterministically sorted listings.
- [ ] **AC-09:** Patch/write requires conflict evidence for existing content, rejects internal metadata/secret-like/destructive changes, performs atomic replacement, and leaves the original byte-identical on failure.
- [ ] **AC-10:** Bounded process execution uses `shell=false`, a finite diagnostic action allowlist, safe environment projection, a workspace cwd, 30-second/64-KiB limits, cancellation, and no background child.
- [ ] **AC-11:** Git accepts only status/diff operations with hooks/pagers/external diff disabled; npm diagnostics runs only an existing validated script without install/fetch behavior.
- [ ] **AC-12:** Unknown tool/action/field, empty required input, absolute/escaping path, secret fixture, destructive request, timeout, output overflow, and cancellation return stable typed non-success codes.
- [ ] **AC-13:** The complete denial matrix is identical in `confirm` and `full-access`, and spy executors prove preflight-denied work never starts.
- [ ] **AC-14:** Empty and single-call Provider responses are handled without phantom calls; multiple calls execute serially in Provider emission order and equal/duplicate IDs fail closed.
- [ ] **AC-15:** Argument/result text is UTF-8 JSON with byte-based limits; invalid UTF-8, NUL/binary content, and oversize input never reach a write or child process.
- [ ] **AC-16:** `/permissions`, `/agent`, and `/continue` expose the implemented behavior in TUI/headless tests without adding a third mode or changing the npm entry.
- [ ] **AC-17 (must-NOT):** The product never presents `full-access` as disabling hard gates or as an OS sandbox; status/help text states its process-only approval meaning.
- [ ] **AC-18 (must-NOT):** Approval presentation never pressures the user or hides normalized tool, arguments, and scope; neutral explicit approval is required.
- [ ] **AC-19 (must-NOT):** No interruption, recovery, or Provider error path fabricates tool success or silently auto-replays an indeterminate effect.
- [ ] **AC-20 (must-NOT):** No Phase 3 schema or adapter exposes arbitrary shell/interpreter execution, MCP, plugins, subagents, Git mutation, package installation, or network fetch.
- [ ] **AC-21 (must-NOT):** Active `full-access` state is never persisted and no public/internal third permission variant is introduced.
- [ ] `cargo fmt --all -- --check`, workspace Clippy with `-D warnings`, `cargo test --workspace --locked`, TypeScript typecheck/tests, architecture checks, compatibility verification, and hosted Windows/MSVC plus Ubuntu CI all pass.

## Edge Coverage

**Coverage:** 15/15 applicable edges resolved · 0 unresolved

| Category | Requirement | Status | Resolution / Reason |
|----------|-------------|--------|---------------------|
| idempotency | R1 | ✅ covered | AC-03 rejects duplicate call identities, transitions, and terminal results. |
| concurrency | R1 | ✅ covered | AC-02 and AC-04 serialize progress and make interrupted effects indeterminate before another Provider round. |
| idempotency | R2 | ✅ covered | AC-03 and AC-06 allow one recorded decision and one execution only. |
| concurrency | R2 | ✅ covered | AC-07 freezes an in-flight call's recorded decision despite mode changes or conflicting input. |
| idempotency | R3 | ✅ covered | AC-05 makes repeated mode changes deterministic and restart-to-confirm unconditional. |
| concurrency | R3 | ✅ covered | AC-07 defines the permission snapshot boundary for in-flight calls. |
| adjacency | R4 | ✅ covered | AC-14 treats equal/duplicate call IDs as collisions and preserves distinct emitted calls. |
| empty | R4 | ✅ covered | AC-12 and AC-14 define empty required input, zero calls, and one call. |
| encoding | R4 | ✅ covered | AC-15 defines UTF-8 JSON, NUL/binary rejection, and byte-based limits. |
| ordering | R4 | ✅ covered | AC-08 sorts listings and AC-14 preserves Provider emission order. |
| concurrency | R4 | ✅ covered | AC-10 and AC-14 require bounded children and serial execution within a run. |
| empty | R5 | ✅ covered | AC-12 rejects absent/empty required safety inputs before execution. |
| encoding | R5 | ✅ covered | AC-15 fixes the representation and rejection boundary used by both modes. |
| idempotency | R5 | ✅ covered | AC-04 and AC-19 forbid automatic replay once an effect may have started. |
| concurrency | R5 | ✅ covered | AC-04, AC-07, and AC-13 define interruption, mode-change, and policy ordering. |

## Prohibitions (must-NOT)

**Coverage:** 5/5 applicable prohibitions resolved · 0 unresolved

Path traversal, command injection, credential leakage, and related canon security controls remain requirements and are additionally owned by `$gsd-secure-phase`; they are not duplicated here as bespoke prohibition rows.

| Prohibition (must-NOT statement) | Requirement | Status | Verification / Reason |
|----------------------------------|-------------|--------|------------------------|
| MUST NOT describe `full-access` as disabling hard gates or providing an OS sandbox. | R3/R5 | resolved | verification: judgment; AC-17 routes help/status wording to review. |
| MUST NOT pressure a user into approval or conceal the normalized operation and scope. | R2 | resolved | verification: judgment; AC-18 routes approval copy/presentation to review. |
| MUST NOT fabricate success or silently replay an invocation whose side effect is unknown. | R1/R5 | resolved | verification: test; AC-04 and AC-19 require boundary-injected recovery tests. |
| MUST NOT widen bounded execution into arbitrary shell/interpreter, remote tool, Git mutation, installation, or network behavior. | R4/R5 | resolved | verification: test; AC-20 requires schema snapshots and negative adapter tests. |
| MUST NOT persist `full-access` or introduce a hidden/public third permission state. | R3 | resolved | verification: test; AC-05 and AC-21 require construction, serialization, and static variant checks. |

## Ambiguity Report

| Dimension           | Score | Min   | Status | Notes |
|---------------------|-------|-------|--------|-------|
| Goal Clarity        | 0.97  | 0.75  | ✓      | Five roadmap requirements map directly to one measurable tool loop. |
| Boundary Clarity    | 0.98  | 0.70  | ✓      | Fixed tool inventory and explicit v2/Phase 4-6 exclusions. |
| Constraint Clarity  | 0.95  | 0.65  | ✓      | Permission lifetime, safety parity, limits, platforms, and architecture are locked. |
| Acceptance Criteria | 0.94  | 0.70  | ✓      | Positive, negative, recovery, parity, and hosted-platform checks are enumerated. |
| **Ambiguity**       | **0.04** | **<=0.20** | **✓** | Weighted clarity 0.96. |

## Interview Log

The user completed the product interview before Phase 3 began. This run used those explicit decisions in auto mode and did not reopen settled product questions.

| Round | Perspective | Question summary | Decision locked |
|-------|-------------|------------------|-----------------|
| Prior discussion | Researcher | Should Rust reuse the current tool loop and Provider behavior? | Preserve the useful loop contract, but rebuild it across Rust protocol/core/tools boundaries. |
| Prior discussion | Simplifier | What is the irreducible v1 tool surface? | Read/list, patch/write, bounded process, Git status/diff, and npm diagnostics only. |
| Prior discussion | Boundary Keeper | How many permissions and how long does elevated access last? | Exactly `confirm` and process-scoped `full-access`; every new process returns to `confirm`. |
| Prior discussion | Failure Analyst | What must happen after cancellation or an uncertain side effect? | Persist indeterminate state, never fabricate success, and never auto-replay the call. |
| Auto edge probe | Seed Closer | Which boundary, repetition, ordering, and encoding cases remain? | Fifteen applicable edges were converted to AC-02 through AC-15 with none unresolved. |
| Auto prohibition probe | Seed Closer | What could the feature silently become that the user would reject? | Five bespoke transparency/safety prohibitions were locked; canon security items were referred to `$gsd-secure-phase`. |

---

*Phase: MMX-03-safe-tool-completion*
*Spec created: 2026-07-15*
*Next step: `$gsd-discuss-phase 3` — implementation decisions for the locked requirements above*
