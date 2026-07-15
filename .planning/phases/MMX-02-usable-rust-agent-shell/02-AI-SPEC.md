# AI-SPEC — Phase 2: Usable Rust Agent Shell

> AI design contract for the first Rust conversation runtime. Production remains a native Rust state machine; no external agent framework owns session state, tools, or persistence.

## 1. System Classification

**System Type:** Provider-agnostic conversational agent runtime with deterministic lifecycle and recovery

**Description:**
A user prompt becomes one typed turn. A pinned Provider/model receives bounded conversation context and streams provider-specific frames through a wire adapter. The adapter emits only provider-neutral safe events. Core validates their order, persists observable state through ports, supports cancellation/restart, and projects the same events to interactive or headless clients. Tool calls may be observed but are not executed until Phase 3.

**Critical Failure Modes:**

1. A malformed or prematurely closed Provider stream is recorded as completed.
2. Cancellation/crash produces two terminal outcomes or loses all evidence of partial output.
3. A credential, raw Provider frame, or private reasoning field reaches JSONL, trace, logs, config, or panic output.
4. Retry/resume duplicates a turn, changes its model binding silently, or mutates prior terminal history.
5. TUI and headless clients apply different runtime policy instead of rendering the same typed events.

## 1b. Domain Context

**Industry Vertical:** Local developer tooling and command-line AI agents

**User Population:** Non-programmers and developers who need a recoverable, inspectable local Agent shell

**Stakes Level:** High

**Output Consequence:** Users may act on model output or later allow tools to change files. Incorrect lifecycle, hidden partial output, or leaked credentials undermines both safety and recovery.

### What Domain Experts Evaluate Against

| Dimension | Good — expert accepts | Bad — expert flags | Stakes |
|-----------|------------------------|--------------------|--------|
| Terminal truth | Exactly one durable completed/failed/interrupted/stopped outcome | Missing, duplicated, or fabricated completion | Critical |
| Stream fidelity | Visible text and usage preserve order; private reasoning is filtered | Reordered deltas, raw frame leakage, reasoning exposure | Critical |
| Recovery | Restart reconstructs history and resolves in-flight work deterministically | Lost turns, duplicate retries, half-finalized state | Critical |
| Provider neutrality | Both wire protocols yield the same core event contract | Provider-specific branching inside core/UI | High |
| Secret safety | Credentials exist only in transient secret wrappers and authorized stores | Secret in config, JSONL, Debug, trace, or error | Critical |
| User clarity | Interactive/headless output distinguishes progress, partial, failure, and interruption | Spinner forever, ambiguous exit, silent degraded behavior | High |

### Known Failure Modes in This Domain

- Treating HTTP 200 or `[DONE]` alone as semantic completion.
- Confusing caller cancellation with timeout and suggesting unsafe automatic retry.
- Feeding interrupted partial assistant text back as authoritative conversation context.
- Persisting every network frame, which stores sensitive vendor details and makes replay protocol-dependent.
- Hiding keyring unavailability by silently writing a plaintext fallback.
- Letting terminal widgets own concurrency, cancellation, or session state.

### Regulatory / Compliance Context

No domain-specific regulated workflow is assumed. General credential protection, privacy, auditability, local data ownership, and user-controlled deletion expectations apply. Provider endpoints must be HTTPS except explicitly enabled loopback development endpoints.

### Domain Expert Roles for Evaluation

| Role | Responsibility |
|------|---------------|
| Runtime maintainer | Labels stream ordering, retry, recovery, and deterministic fixtures |
| Security reviewer | Labels credential, redaction, endpoint, and private-reasoning cases |
| Product owner/user | Confirms command behavior and whether terminal outcomes are understandable |

## 2. Framework Decision

**Selected Framework:** Native Rust core workflow using Tokio plus direct Reqwest/rustls Provider adapters

**Version:** Workspace contract v1; exact dependency versions are pinned in `Cargo.lock`

**Rationale:**
The project already owns typed Provider events, a strict terminal reducer, and a one-way crate architecture. Phase 2 is a bounded state machine with two HTTP wire adapters, not a multi-agent graph or RAG pipeline. A Python/TypeScript AI framework would create a second runtime, duplicate session/checkpoint policy, and undermine the Rust composition root. Core remains a pure reducer; Tokio supplies cancellation-aware async execution in adapters/the composition driver, Reqwest/rustls supplies cross-platform HTTPS and streaming, and Serde keeps the protocol strict.

**Alternatives Considered:**

| Framework | Ruled Out Because |
|-----------|------------------|
| LangGraph | Adds a second language/runtime and its own checkpoint graph around an already-defined Rust state machine |
| OpenAI Agents SDK | Provider-oriented and duplicates this project's model/session/tool abstractions while weakening MiniMax/custom compatibility |
| LangChain | Broad abstraction overhead without solving Rust durability, secret, or terminal contracts |
| CrewAI | Multi-agent role orchestration is out of scope and its persistence model does not match local Rust recovery |

**Vendor Lock-In Accepted:** No framework lock-in. Each session pins an explicit Provider profile, protocol, endpoint identity, and model.

## 3. Framework Quick Reference

### Installation

```toml
[dependencies]
tokio = { version = "1", features = ["macros", "rt-multi-thread", "signal", "sync", "time"] }
reqwest = { version = "0.13", default-features = false, features = ["json", "rustls", "stream"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

Exact versions must support Rust 1.97.0 and are committed through `Cargo.lock`.

### Core Imports

```rust
use minimax_core::{RunInput, RunMachine};
use minimax_protocol::RuntimeEventV1;
```

### Entry Point Pattern

```rust
pub async fn drive_turn(
    machine: &mut RunMachine,
    adapters: &mut RuntimeAdapters,
    request: TurnRequest,
) -> Result<TurnReceipt, RuntimeError> {
    apply_effects(machine.apply(RunInput::Begin(request))?, adapters).await?;
    drive_provider_events(machine, adapters).await
}
```

### Key Abstractions

| Concept | What It Is | When You Use It |
|---------|------------|-----------------|
| `TurnRequest` | Validated session/model/input/context binding | Before any Provider call |
| `ProviderPort` | Provider-neutral async event stream boundary | Core run orchestration |
| `RuntimeEventV1` | Strict observable event envelope | Persistence, TUI, JSONL, replay |
| `SessionStore` | Append/checkpoint/list/recover port | Every durable transition |
| `CancellationToken` | Shared caller/shutdown cancellation signal | Network read and controlled shutdown |
| `TurnReceipt` | One terminal outcome plus usage and safe evidence | Retry, recovery, diagnostics |

### Common Pitfalls

1. Holding a storage or UI lock across network await points.
2. Accepting Provider EOF without an explicit terminal marker.
3. Serializing `reqwest::Error` or response bodies directly into user-visible diagnostics.
4. Using an unbounded channel for text deltas or trace events.
5. Assuming dropping an async task persists interruption; core must append the terminal record first.

### Recommended Project Structure

```text
crates/protocol/src/{command,runtime,session}.rs
crates/core/src/{runtime,session,compaction,trace,config}.rs
crates/provider/src/{client,sse,responses,chat_completions,credentials}.rs
crates/vault/src/runtime/{journal,index,lease,recovery}.rs
crates/tui/src/{command,render,shell}.rs
crates/cli/src/{main,headless,doctor}.rs
```

## 4. Implementation Guidance

**Model Configuration:** Resolve a validated provider profile and model before creating the turn. The session records provider/profile/model/protocol/endpoint identity and bounded context settings. Retry keeps that identity unless an explicit later command changes the active model for a new turn.

**Core Pattern:** A pure `RunMachine` maps typed inputs to effects. The CLI composition driver persists intent before external I/O, feeds normalized stream events back into the reducer, and persists one terminal outcome before publishing completion. A bounded channel applies backpressure. Core imports no async runtime and never reads terminal state or Provider frames directly.

**Tool Use:** Phase 2 does not execute tools. A normalized tool-call request is recorded as an observable event and ends with an explicit later-phase-required outcome rather than inventing a tool result.

**State Management:** JSONL is append-only evidence; a small atomically replaced index/checkpoint is rebuildable. Workspace lease is exclusive. Startup validates versions/sequences, repairs only a truncated final record, and reconciles any active turn.

**Context Window Strategy:** Use a deterministic compact summary, completed non-partial messages after its boundary, the current user input once, and explicit token/character budgets. Trace and private reasoning never enter model context.

## 4b. AI Systems Best Practices

### Structured Outputs with Rust and a Pydantic Eval Mirror

Production Rust is authoritative:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum RuntimeEvent {
    VisibleTextDelta { turn_id: TurnId, delta: String },
    Usage { turn_id: TurnId, usage: Usage },
    Terminal { turn_id: TurnId, outcome: TerminalOutcome },
}
```

A Pydantic mirror may be used only by offline cross-language fixture validation; it is not a production dependency:

```python
from pydantic import BaseModel, ConfigDict

class RuntimeEventFixture(BaseModel):
    model_config = ConfigDict(extra="forbid")
    schemaVersion: int
    event: dict
```

Validation order is transport status -> bounded SSE framing -> provider-specific JSON parse -> protocol normalization -> sequence validation -> durable append -> client projection. Unknown fields or raw frames do not cross the adapter.

### Async-First Design

Provider streaming, persistence, and client publication are awaited through ports. Cancellation is selected alongside the next stream event. No filesystem/UI lock is held during network I/O. Shutdown owns a single idempotent finalization path.

### Prompt Engineering Discipline

System/user messages and provider feature flags are constructed from typed input. Phase 2 does not introduce hidden self-modifying prompts. Provider-specific request shapes are fixture-tested, and unsupported declared features fail before a request.

### Context Window Management

The local compactor never calls the model. It excludes trace, errors, credentials, raw reasoning, and interrupted partial assistant text; it retains the current user input exactly once and fails clearly if the required input alone exceeds budget.

### Cost and Latency Budget

One ordinary turn makes one initial Provider request; Phase 2 has no autonomous retry after uncertain network failure. Usage is recorded when provided. Default tests make zero paid requests. Cold-start and idle-memory release gates remain visible for Phase 6.

## 5. Evaluation Strategy

### Dimensions

| Dimension | Rubric | Measurement Approach | Priority |
|-----------|--------|----------------------|----------|
| Terminal correctness | Pass only with one legal durable terminal outcome | Code/property fixtures | Critical |
| Stream normalization | Both protocols produce identical safe event sequences | Code/golden fixtures | Critical |
| Recovery/idempotency | Every injected crash boundary converges without duplicate turn/terminal | Fault-injection tests | Critical |
| Cancellation truth | Caller cancellation is interrupted; timeout/provider failure remain distinct | Code/async tests | Critical |
| Secret/reasoning safety | No adversarial marker appears in config, journal, JSONL, trace, Debug, or errors | Code/security fixtures | Critical |
| Context correctness | Summary boundary and retained messages are deterministic and exclude partial content | Golden/property tests | High |
| Interface parity | Interactive and headless projections share command/event semantics | Snapshot/integration tests | High |
| Config clarity | Precedence and credential source are deterministic without plaintext fallback | Table-driven tests | High |

### Eval Tooling

**Primary Tool:** Native Rust fixture/fault runner through `cargo test`

**Observability Override:** Arize Phoenix is intentionally not a Phase 2 dependency. A required Python service conflicts with the single-binary/local-first product. Safe local events and receipts are the tracing baseline; an optional OpenTelemetry/Phoenix exporter may be added later without changing core.

**CI/CD Integration:**

```bash
cargo test --workspace --locked
npm run verify:rust-contracts
```

### Reference Dataset

**Size:** 20 deterministic offline cases initially

**Composition:** 3 successful streams, 2 interruptions, 2 timeouts/provider failures, 3 malformed/terminal-order failures, 3 restart/journal corruption cases, 2 retry/compaction cases, 2 credential/config cases, 2 trace/private-reasoning cases, and 1 TUI/headless parity case.

**Labeling:** Runtime maintainer labels lifecycle/recovery, security reviewer labels secret/reasoning cases, and product owner labels interface clarity. No LLM judge is required for Phase 2 acceptance.

## 6. Guardrails

### Online

| Guardrail | Trigger | Intervention |
|-----------|---------|--------------|
| Endpoint policy | Non-HTTPS non-loopback endpoint | Reject before credential lookup/request |
| Credential boundary | Missing environment key in headless or unavailable keyring in interactive | Fail with redacted setup guidance; never write plaintext |
| Frame/line bound | Oversize SSE or journal record | Abort with typed failure; do not echo body |
| Sequence reducer | EOF, duplicate terminal, data after terminal | Persist failed terminal once |
| Cancellation | Caller or shutdown token fires | Abort transport and persist interrupted once |
| Workspace lease | Another live writer holds lock | Exit busy without touching journals |
| Trace allowlist | Unknown fact/code or secret marker | Drop/redact and emit safe diagnostic |

### Offline

| Metric | Sampling Strategy | Action on Degradation |
|--------|------------------|-----------------------|
| Invalid stream acceptance | Full fixture matrix on every Provider change | Block merge |
| Recovery divergence | Full crash-point matrix on every journal/session change | Block merge and repair reducer |
| Secret marker leakage | Full adversarial corpus on every config/provider/trace change | Block release |
| Command parity drift | Compare every command/alias on each compatibility report | Keep pending or fix; never false-match |

## 7. Production Monitoring

**Tracing Tool:** Built-in safe local `RuntimeEventV1` plus folded trace; optional exporter deferred

**Key Metrics:** turn outcomes, interruption/timeout/provider-failure counts, recovery repairs, journal corruption, lease contention, context estimates, usage, and credential source category without value.

**Alert Thresholds:** Any secret/private-reasoning persistence, duplicate terminal, fabricated completion, silent model/provider switch, or unrecoverable middle corruption is release-blocking.

**Smart Sampling:** Always retain safe diagnostics for hard-guardrail triggers; oversample interrupted/retried/recovered turns; sample ordinary completed turns only through redacted structured receipts.

## Checklist

- [x] System type classified
- [x] Critical failure modes identified
- [x] Domain context and expert roles documented
- [x] Native Rust framework selected with alternatives ruled out
- [x] Framework quick reference and async pattern written
- [x] Rust authority and Pydantic fixture mirror documented
- [x] Evaluation dimensions and 20-case dataset defined
- [x] Guardrails and failure interventions specified
- [x] Local-first tracing override documented
- [x] No real Provider quota required for acceptance

## Primary References

- https://tokio.rs/
- https://docs.rs/reqwest/latest/reqwest/
- https://serde.rs/
- https://docs.rs/crossterm/latest/crossterm/
- https://docs.rs/keyring/latest/keyring/
