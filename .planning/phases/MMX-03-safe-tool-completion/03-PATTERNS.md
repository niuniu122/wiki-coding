# Phase 3 Pattern Map: Safe Tool Completion

**Mapped:** 2026-07-15
**Source:** `03-CONTEXT.md`, `03-AI-SPEC.md`, current Rust/TypeScript code

## Data Flow

```text
TurnRequest + static ToolDefinition[]
  -> minimax-provider request mapping
  -> complete StreamEvent::ToolCallFragments
  -> minimax-core InvocationMachine effects
  -> RuntimeStore append before every boundary
  -> CLI ApprovalPort + minimax-tools ToolPort
  -> durable ToolResult
  -> Provider-native tool-result conversation item
  -> next Provider round
```

Core remains synchronous. Provider, filesystem/process tools, terminal approval, and JSONL storage stay in adapter/composition crates.

## Closest Existing Analogs

| New/modified role | Closest analog | Pattern to preserve | Important difference |
|-------------------|----------------|---------------------|----------------------|
| `crates/protocol/src/tool.rs` | `crates/protocol/src/runtime.rs`, `event.rs` | Validated IDs, `SchemaVersion`, Serde `deny_unknown_fields`, snake_case enums, bounded `validate()` | Add complete normalized invocation/result and native conversation items; do not expose Provider JSON. |
| `crates/core/src/tool.rs` | `crates/core/src/runtime.rs` | Pure `apply(input) -> Vec<Effect>`, illegal transition errors, persistence before publication/external effect | Tool machine has request/decision/start/terminal states and explicit `indeterminate`. |
| Provider request mapping | `crates/provider/src/responses.rs`, `chat_completions.rs` | Small deterministic `build_request`, protocol-specific mapping only | Responses and Chat need different native call/result history shapes but share protocol inputs. |
| Durable tool boundaries | `crates/core/src/session.rs`, `crates/vault/src/runtime/mod.rs` | Commands preview state, emit `Persist`, append+sync journal, update derived index only after append | Add invocation records and recovery without making core depend on Vault or tools depend on Vault. |
| Recovery | `crates/vault/src/runtime/recovery.rs` | Stable recovery record IDs derived from durable identity; idempotent startup convergence | Post-`started` invocation becomes `indeterminate`; pre-start work never auto-executes. |
| Async composition | `crates/cli/src/driver.rs` | Injectable ports, child cancellation token, persist-before-publish loop, fake-Provider tests | Add repeated Provider rounds, approval/tool ports, serial multi-call execution, and finite budgets. |
| Approval presentation | `crates/tui/src/command.rs`, `shell.rs` | Presentation-only crate, strict slash parsing, terminal capability fallback | Make Phase 3 commands available and bind one neutral explicit decision to one call ID. |
| Path/read safety | `src/capabilities/executors/workspace-read-executor.ts` | Canonical root containment, binary/output limits, deterministic listing | Rust also covers nearest-existing-ancestor writes, protected metadata/secrets, and reparse/symlink boundaries. |
| Direct process safety | `src/capabilities/executors/npm-diagnostic-executor.ts` | `shell=false`, safe env, bounded combined output, cancellation/timeout, raw stderr not returned | Rust uses finite diagnostic action enums plus dedicated Git/npm adapters; no generic script descriptor. |
| Dispatcher ordering | `src/capabilities/capability-dispatcher.ts` | Persist request before policy/execution and persist terminal result afterward | Rust records approval and `started`, survives restart, and has exactly two permissions. |

## Concrete Patterns

### Pure reducer and ordered effects

`RunMachine::begin` establishes the ordering Phase 3 should mirror:

```rust
Ok(vec![
    RunEffect::Persist(started.clone()),
    RunEffect::Publish(started),
    RunEffect::OpenProvider(request),
])
```

For a tool request the order becomes `Persist(requested)`, optional `RequestApproval`, `Persist(decision)`, `Persist(started)`, `Execute`, `Persist(terminal)`, `Publish/ContinueProvider`. The coordinator must execute effects strictly in order.

### Preview then append

`RuntimeStore::apply_command` applies to a cloned machine, then appends every emitted record:

```rust
let mut preview = self.machine.clone();
let effects = preview.apply(command)?;
for effect in &effects {
    if let SessionEffect::Persist(record) = effect {
        self.append(record.clone())?;
    }
}
```

Extend this pattern rather than writing tool JSONL directly from CLI/tools.

### Idempotent recovery identity

`recover_abandoned_turns` derives `recovery-{hash}` from stable session/turn identity. Tool recovery should include session, turn, and `ToolCallId` so reopening the store emits the same recovery record and never duplicates an outcome.

### Provider-specific native mapping

- Responses: tool definition `{type:"function",name,description,parameters,strict:true}`; prior call item and `{type:"function_call_output",call_id,output}`.
- Chat Completions: `tools:[{type:"function",function:{...}}]`; full assistant message with `tool_calls`; then `{role:"tool",tool_call_id,content}`.
- Both retain `ToolCallId` unchanged and serialize result content as bounded JSON text.

### Port injection

`RuntimeDriver<P: ProviderPort>` proves the project pattern: interfaces live at the composition boundary and tests provide scripted ports. Phase 3 should add `ApprovalPort` and `ToolPort` with spy/scripted implementations and avoid real terminal/process/network dependencies in core tests.

## File/Artifact Map

| Plan | Files | Role |
|------|-------|------|
| 03-01 | `crates/protocol/src/tool.rs`, `runtime.rs`, `session.rs`; `crates/core/src/tool.rs`, `runtime.rs`, `session.rs`, `ports.rs`; Provider adapters/tests; Vault runtime tests; CLI driver tests | Typed conversation, state machine, persistence/recovery, permission policy, Provider round trip. |
| 03-02 | `crates/tools/src/{lib,policy,path,read,write,process,git,npm}.rs`; CLI/TUI/headless integration; compat fixtures/tests | Shared preflight, eight adapters, approval UI, `/agent`/`/continue`/`/permissions`, full E2E and parity evidence. |

## Landmines

- `RunMachine` currently treats any tool call as terminal `ToolUnavailable`; ordinary `/chat` behavior must remain intact while `/agent` enters the new loop.
- `TurnRequest.messages` is text-only. Adding tool history must not silently serialize call/results as ordinary assistant/user content.
- MiniMax requires the complete assistant tool-call message in subsequent Chat history; retaining only call ID/name is insufficient.
- `RuntimeStore` record IDs are idempotent: tool record identities and duplicate handling must not allow a different payload under the same ID.
- `.minimax/runtime/v1` is inside the project but must be unreadable/unwritable by model tools.
- Local Windows gnullvm lacks full supported Windows SDK imports. Process/filesystem behavior must also compile and run on hosted Windows/MSVC and Ubuntu before Phase 3 closes.
- Do not port TypeScript's legacy third permission form. Compatibility fixtures and static scans should assert the Rust enum has exactly two variants.
- Schema-push hook is not applicable: Phase 3 touches no ORM/database schema and the project explicitly forbids SQLite/ORM dependencies.
