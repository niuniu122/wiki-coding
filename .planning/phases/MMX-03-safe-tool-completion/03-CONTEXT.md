# Phase 3: Safe Tool Completion - Context

**Gathered:** 2026-07-15
**Status:** Ready for planning
**Mode:** Auto-resolved from the user's completed architecture discussion

<domain>
## Phase Boundary

Complete the Rust model-to-tool-to-model loop for a fixed, local, non-destructive v1 tool set. The phase owns typed tool conversations, durable invocation identity and recovery, exactly two process-scoped approval modes, bounded local adapters, and their TUI/headless integration. It does not add a database, Vault/Wiki behavior, retrieval, arbitrary shell, remote tools, or release cutover.

</domain>

<spec_lock>
## Requirements (locked via SPEC.md)

**5 requirements are locked.** See `03-SPEC.md` for full requirements, boundaries, edge coverage, prohibitions, and acceptance criteria.

Downstream agents MUST read `03-SPEC.md` before planning or implementing. Requirements are not duplicated here.

**In scope (from SPEC.md):**

- Provider-neutral native tool definitions, assistant calls, and tool-result messages for Responses and Chat Completions.
- A core-owned invocation state machine, approval port, process-local permission state, budgets, cancellation, and durable recovery.
- Exactly `confirm` and `full-access`, including `/permissions`, `/agent`, `/continue`, TUI approval, and headless structured rejection.
- Workspace read/list, atomic patch/write, bounded diagnostics, Git status/diff, and npm diagnostics.
- Offline compatibility, denial-parity, recovery, and mocked end-to-end tests.

**Out of scope (from SPEC.md):**

- Arbitrary shell/interpreters, background processes, file deletion, Git mutation, package installation, or network fetch.
- MCP, plugins, subagents, daemons, remote tool servers, and dynamically installed tools.
- OS sandbox claims, Vault/Wiki mutation, retrieval, migration, packaging, or npm-entry cutover.
- Persisted permission preferences or any third permission mode.

</spec_lock>

<decisions>
## Implementation Decisions

### Provider-native tool conversation

- **D-301:** Extend the provider-neutral protocol rather than embedding calls/results in ordinary text. `TurnRequest` carries strict tool definitions; conversation items distinguish ordinary messages, assistant tool calls, and tool results while retaining the Provider-issued `ToolCallId`.
- **D-302:** The Responses adapter emits native function tools and `function_call_output` input items; the Chat Completions adapter emits native `tools`, assistant `tool_calls`, and `role=tool` messages. Provider-specific fields stop at `minimax-provider`.
- **D-303:** Assemble and parse arguments as one bounded JSON object before persistence or policy. Unknown fields, non-object arguments, missing/duplicate IDs, and incomplete fragments become typed failures; raw argument fragments never reach an adapter.
- **D-304:** When one Provider response requests multiple tools, preserve Provider emission order and execute serially. Persist all terminal tool results before the next Provider round; never interleave a new model request with an unfinished call.

### Durable invocation state and recovery

- **D-305:** `minimax-core` owns a pure invocation reducer with legal states `requested`, `approved|rejected`, `started`, and one terminal `succeeded|failed|cancelled|indeterminate`. The coordinator emits persistence effects; `minimax-tools` never owns the loop or retry policy.
- **D-306:** Append one typed session-journal record at each state boundary. The invocation request is durable before approval, the decision is durable before adapter entry, `started` is durable before an external effect, and a terminal result is durable before publication or Provider continuation.
- **D-307:** Treat `ToolCallId` as unique within a session. A duplicate ID or illegal transition fails closed before adapter entry; a terminal call cannot be reopened or returned twice.
- **D-308:** Startup converts any durable `started` invocation without a terminal result to `indeterminate`. It never guesses success and never automatically reruns it. A call recorded only as `requested` or `approved` may be resolved as cancelled/rejected during recovery, but not silently executed.
- **D-309:** Use bounded run budgets (model rounds, tool calls, elapsed time, and accumulated tool-result bytes). Budget exhaustion becomes a durable structured tool/run failure and stops additional effects.

### Approval behavior and process lifetime

- **D-310:** Every coordinator instance constructs with `PermissionMode::Confirm`; the active mode is an in-memory field owned by CLI composition and is absent from config, credentials, journal, and Vault schemas.
- **D-311:** In `confirm`, normalize and persist the call first, then show a neutral approval summary containing the tool name, exact normalized arguments, workspace scope, and safety classification. Only an explicit approval for that call ID advances it.
- **D-312:** Rejection, EOF, unavailable interactivity, conflicting/duplicate decision, or cancellation before `started` yields a durable structured non-success result and zero adapter calls. In headless `confirm`, unavailable approval is returned to the Provider as a rejection; users opt into process-only `full-access` explicitly rather than through a hidden default.
- **D-313:** `full-access` bypasses only the interactive prompt for known v1 tools after preflight succeeds. It does not weaken schemas, path/secret/destructive checks, budgets, or cancellation. Unknown tools fail before any approval decision.
- **D-314:** Permission mode is snapshotted when a specific invocation decision is made. Later `/permissions` changes affect only calls without a recorded decision; they never retroactively authorize or revoke in-flight work.

### Finite v1 tool schemas

- **D-315:** Publish eight concrete tool names grouped into the five product categories: `read_file`, `list_directory`, `apply_patch`, `write_file`, `run_diagnostic`, `git_status`, `git_diff`, and `npm_diagnostic`. Names and JSON Schemas are static, versioned, and reject unknown fields.
- **D-316:** `read_file` and `list_directory` accept one workspace-relative path. Reads require UTF-8 non-binary regular files; listings return typed entry records sorted by normalized name with a 500-entry/64-KiB ceiling.
- **D-317:** `apply_patch` accepts a workspace-relative path, required SHA-256 of the existing bytes, and ordered exact replacement edits with expected occurrence counts. `write_file` requires explicit `create` or `replace`; replacement requires the expected SHA-256. Both cap new UTF-8 content at 64 KiB, write a sibling temporary file, sync/close, atomically replace, and never delete.
- **D-318:** `run_diagnostic` is not a command-string shell. It accepts a finite action enum (`cargo_check`, `cargo_test`, `cargo_clippy`, `cargo_fmt_check`, `node_check`, `rg_search`) with action-specific bounded arguments, launches directly with `shell=false`, a safe environment, workspace cwd, 30-second timeout, 64-KiB combined-output budget, cancellation, and child cleanup.
- **D-319:** `git_status` and `git_diff` construct fixed Git argv only, disable pagers/hooks/external diff and color, and expose no mutation flags. `npm_diagnostic` may invoke only an existing package script that matches the diagnostic allowlist and may not run install, exec/npx, lifecycle mutation, or network-fetch behavior.
- **D-320:** Every adapter returns one common structured terminal envelope: call ID, tool name, status/code, bounded redacted output or metadata, exit code when applicable, and an `effect` classification. Raw stderr counts toward the byte budget but is not persisted or sent to the model.

### Workspace, secret, and destructive boundaries

- **D-321:** One shared preflight service applies to both permission modes and all adapters. It resolves the canonical project root, rejects absolute/NUL/`..` paths, checks the canonical target or nearest existing ancestor, and rejects symlink/junction/reparse escapes.
- **D-322:** Deny writes to `.git/`, `.minimax/`, runtime/Vault metadata, credential files, and secret-like paths. Read/output preflight also blocks known secret files and scans bounded content for credential/private-key markers before it can be persisted or returned to a Provider.
- **D-323:** Destructive operations are structurally absent: no delete/rename tree, shell interpreter, arbitrary executable, background child, Git mutation, package install, or network action exists in a v1 schema. A denylist may supplement but never substitute for the finite allowlist.
- **D-324:** Cancellation is checked before persistence transitions, before adapter entry, during process/file work where possible, and before publication. Once `started` is durable, inability to prove the terminal effect produces `indeterminate`, never `cancelled` or `succeeded` by assumption.

### the agent's Discretion

- Exact Rust module split and private helper names inside the protocol/core/provider/tools/CLI/TUI/Vault crates.
- Exact wording and terminal colors of the neutral approval prompt, provided the normalized operation/scope is visible and approval is never manipulative.
- Exact maximum model rounds/tool calls within conservative finite defaults, provided tests cover exhaustion and the configured value cannot become unbounded.
- Exact secret-pattern implementation and diagnostic-script naming predicate, provided fixtures prove common credentials/private keys and install/network scripts fail closed without false success.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Locked product and phase contracts

- `.planning/phases/MMX-03-safe-tool-completion/03-SPEC.md` — locked requirements, limits, boundaries, edge cases, prohibitions, and acceptance criteria.
- `.planning/SPEC.md` — master rewrite architecture, two-mode permission contract, v1 tool categories, and unknown-side-effect rule.
- `.planning/REQUIREMENTS.md` — TOOL-01 through TOOL-05 traceability.
- `.planning/ROADMAP.md` — Phase 3 goal, two-plan shape, dependencies, and success criteria.
- `.planning/PROJECT.md` — global architecture, exclusions, performance gates, and authorization limits.
- `.planning/phases/MMX-02-usable-rust-agent-shell/02-CONTEXT.md` — runtime/provider/session/CLI ownership decisions carried forward.
- `.planning/phases/MMX-02-usable-rust-agent-shell/02-03-SUMMARY.md` — verified CLI composition, command parser, and hosted CI baseline.

### Existing Rust integration points

- `crates/protocol/src/runtime.rs` — current provider-neutral turn request/message schema to extend.
- `crates/protocol/src/event.rs` — validated `ToolCallId`, tool-call fragments, usage, and terminal protocol.
- `crates/core/src/runtime.rs` — current run reducer/effect pattern and `tool_unavailable` gap.
- `crates/core/src/ports.rs` — port pattern for approval/tool execution and deterministic tests.
- `crates/provider/src/responses.rs` — Responses request builder and stream normalization.
- `crates/provider/src/chat_completions.rs` — Chat Completions request builder and tool-call normalization.
- `crates/cli/src/driver.rs` — Provider loop, persist-before-publish composition, IDs, retry, cancellation, and compaction.
- `crates/tui/src/command.rs` — `/permissions`, `/agent`, and `/continue` parser integration.
- `crates/tui/src/shell.rs` — interactive-vs-line/headless capability boundary for approvals.
- `crates/vault/src/runtime/mod.rs` — recoverable session journal/store to extend with typed invocation records.
- `crates/compat-harness/src/architecture.rs` — one-way dependency and forbidden-source checks.

### TypeScript behavioral evidence, not architecture to copy blindly

- `src/runtime/agent-run-engine.ts` — existing model/tool/model loop and budgets.
- `src/agent/model-action.ts` — current tool-call parsing behavior.
- `src/capabilities/capability-invocation.ts` — normalized invocation identity.
- `src/capabilities/capability-dispatcher.ts` — request/result persistence ordering and stale-snapshot checks.
- `src/capabilities/policy-engine.ts` — policy outcomes and denial codes.
- `src/runtime/permission-service.ts` — legacy behavior to replace with exactly two Rust modes; do not port the third permission shape.
- `src/capabilities/execution-limits.ts` — verified 30-second, 64-KiB, and 500-entry baseline.
- `src/capabilities/executors/workspace-read-executor.ts` — canonical containment/binary/list-order evidence.
- `src/capabilities/executors/npm-diagnostic-executor.ts` — direct spawn, safe environment, timeout/output/cancellation evidence.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets

- `ToolCallFragment` plus Provider assemblers already preserve Provider call IDs and accumulate argument fragments.
- `RunMachine`, `SessionMachine`, effects, `RuntimeStore`, and the JSONL journal already demonstrate pure reducers with persist-before-publish composition and crash recovery.
- `ProviderPort` and injectable `RuntimeDriver` make two-round fake-Provider and spy-adapter tests possible without network access.
- The TUI command parser already owns the complete command inventory and recognizes only `confirm`/`full-access` names.
- TypeScript path/process adapters provide fixture ideas and safe limit values without constraining the new Rust crate ownership.

### Established Patterns

- Protocol types are strict Serde schema-v1 records with validated IDs and snake_case wire values.
- Core is synchronous and adapter-free; async HTTP/files/processes remain in Provider/tools/CLI composition.
- Every externally visible runtime event is persisted before publication; derived indexes are atomic and recovery fails closed.
- Compatibility status stays `pending` until executable Rust evidence exists; default tests remain offline and credential-free.

### Integration Points

- Replace the `ToolCallObserved -> ToolUnavailable` stop in `crates/core/src/runtime.rs` with typed invocation effects while preserving ordinary chat behavior.
- Extend both Provider request builders from `Vec<ModelMessage>` text-only input to the shared conversation-item/tool-definition contract.
- Extend the session journal/index projection so tool boundaries survive restart without making the tools crate depend on Vault.
- Compose approval and tool ports in `crates/cli/src/driver.rs`; render only typed approval/event data in TUI/headless adapters.
- Promote `/permissions`, `/agent`, `/continue`, and tool compatibility entries only after end-to-end Rust tests exist.

</code_context>

<specifics>
## Specific Ideas

- The phase proof is a fake Provider response containing two tool calls: the first is rejected in `confirm`, the second succeeds after explicit approval (or both auto-approve in process-only `full-access`), their durable results return in original order, and a final visible answer completes the turn.
- The crash matrix injects failure after every journal boundary. Only pre-start calls can close without uncertainty; anything past durable `started` but before a durable terminal result becomes `indeterminate`.
- Approval text should read like a receipt: what tool, exactly what normalized input, which project scope, and whether a write/process effect is possible.

</specifics>

<deferred>
## Deferred Ideas

- Arbitrary shell/interpreters, background processes, file deletion, Git mutation, package installation, and network tools — possible later work only after a stronger sandbox/threat model.
- MCP, plugins, subagents, remote tool servers, and dynamic capability installation — v2 extension boundary.
- Vault/Wiki tools — Phase 4 after raw evidence and transaction contracts exist.
- Retrieval/project discovery tools — Phase 5 after index isolation and BM25-first behavior are implemented.
- Npm entry cutover and release packaging — Phase 6 after parity and platform gates.

</deferred>

---

*Phase: MMX-03-safe-tool-completion*
*Context gathered: 2026-07-15*
