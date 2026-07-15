# Phase 2 Pattern Map: Usable Rust Agent Shell

**Mapped:** 2026-07-15
**Rule:** Preserve observable contracts, not the TypeScript class graph. New Rust code follows Phase 1 crate ownership and keeps core free of HTTP, filesystem, terminal, keyring, and async-runtime dependencies.

## Data Flow

```text
CLI/TUI intent
  -> protocol command
  -> pure core run/session machine
  -> effects consumed by CLI composition driver
       -> provider async stream adapter
       -> vault session journal adapter
       -> TUI or JSONL projector
  -> one protocol terminal outcome
```

## Closest Existing Analogs

| Rust target | Role | TypeScript analog | Contract to preserve |
|-------------|------|-------------------|----------------------|
| `crates/protocol/src/runtime.rs` | provider-neutral commands/events/receipts | `src/providers/provider-protocol.ts`, `src/runtime/runtime-application.ts` | strict event variants, usage, safe failures, command identity |
| `crates/core/src/runtime.rs` | pure turn reducer/effect planner | `src/runtime/turn-engine.ts`, `src/runtime/command-arbiter.ts` | one active mutation, one terminal outcome, partial output semantics |
| `crates/provider/src/sse.rs` | bounded SSE framing | `src/providers/provider-gateway.ts::readSseData` | CRLF/chunk handling, DONE/terminal discipline, no raw-frame retention |
| `crates/provider/src/client.rs` | HTTPS request/cancellation/timeout | `src/providers/http-transport.ts::FetchHttpStreamTransport` | caller abort differs from timeout, redirects disabled, bounded stream |
| `crates/provider/src/{responses,chat_completions}.rs` | wire normalization | `src/providers/provider-protocol.ts::{ResponsesProtocol,ChatCompletionsProtocol}` | both protocols converge on the same event contract |
| `crates/core/src/session.rs` | session/turn state machine | `src/runtime/session-service.ts::SessionService` | create/list/resume/retry links and terminal history |
| `crates/core/src/compaction.rs` | deterministic local summary | `src/runtime/summary-generator.ts::StructuredLocalSummaryGenerator` | no model call, coverage boundary, no sliced entry/private trace |
| `crates/core/src/trace.rs` | safe allowlisted trace | `src/runtime/trace-recorder.ts::SafeTraceRecorder` | known codes/facts only; unknown facts dropped |
| `crates/vault/src/runtime/journal.rs` | durable append/recovery | `src/storage/session-repository.ts`, `src/storage/storage-provider.ts` | versioned JSONL, final-fragment repair, middle corruption failure |
| `crates/vault/src/runtime/lease.rs` | single writer | `src/runtime/workspace-lease.ts::WorkspaceLease` | live-owner exclusion and retryable release |
| `crates/tui/src/command.rs` | slash parser | `src/ui/chat-input-policy.ts::classifyChatInput` | full command inventory, aliases, explicit unknown command |
| `crates/tui/src/render.rs` | presentation only | `src/ui/format-runtime-event.ts` | folded trace, history/threads formatting, no runtime policy |
| `crates/cli/src/headless.rs` | stable JSONL projection | no direct old equivalent; derive from `RuntimeEvent` | one schema-v1 object per line and stable exit classes |
| `crates/cli/src/doctor.rs` | actionable diagnostics | current slash/report services | source/status only; never credential values |
| `crates/provider/src/config.rs` | profile/config validation | `src/config/provider-config.ts`, `src/config/provider-security.ts` | profile identity, HTTPS/loopback policy, unknown-field failure |
| `crates/provider/src/credentials.rs` | env/keyring resolution | `src/config/credential-store.ts::CredentialStore` | env first, typed keyring failure, no plaintext fallback |

## Concrete Contract Excerpts

### Provider normalization

The baseline separates transport, protocol parsing, and gateway assembly:

```text
ProviderProtocol.parseEvent(raw) -> ProviderStreamEvent
StrictProviderGateway.stream(request) -> AsyncIterable<ProviderGatewayEvent>
FetchHttpStreamTransport.stream(request) -> Response body stream
```

Rust should keep the same three responsibilities but expose them as `HttpProviderClient`, `SseDecoder`, and protocol-specific normalizers. Only normalized `StreamEvent` values reach `RunMachine`.

### Runtime ownership

The baseline `TurnEngine` owns one `ActiveTurn`, flushes deltas before terminal checkpointing, and never treats premature completion as success. Rust should express this as a pure `RunMachine::apply(RunInput) -> Vec<RunEffect>` so core retains policy without importing Tokio or adapters.

### Local compaction

The baseline `StructuredLocalSummaryGenerator` selects completed visible messages, strips reasoning, redacts secrets, deduplicates categorized entries, and stops on an entry boundary. Rust should keep those semantics in a deterministic `CompactionRecord` whose serialization is byte-stable.

### Credential resolution

The baseline distinguishes `unavailable`, `locked`, `denied`, and `unknown` keyring failures. Rust should retain those categories, but Phase 2 removes plaintext fallback entirely: interactive resolution is environment then keyring; headless is environment only.

## Rust-Specific Adjustments

- Tokio, Reqwest, and cancellation tokens live in `provider`/`cli`, never `core`.
- `core` remains limited to protocol plus Serde and models all external work as inputs/effects.
- File locking and JSONL recovery live in `vault`; terminal raw mode and colors live in `tui`.
- Secrets use a non-serializable redacted wrapper. Error types carry fixed codes and allowlisted details, not source error bodies.
- Tests inject mock event streams, clocks, IDs, journal failures, and cancellation points; no real endpoint is required.

## Landmines

1. Reusing the TypeScript `ApplicationKernel` shape would recreate the oversized composition object the rewrite is meant to remove.
2. Letting core call async Provider/storage traits would require runtime/external dependencies and weaken the Phase 1 architecture gate.
3. Persisting provider frames for replay would couple recovery to vendor schemas and risk secret/reasoning leakage.
4. A line-oriented interactive shell must never enable raw mode when stdin/stdout is not a TTY; headless JSONL must not initialize Crossterm.
5. Keyring compile support does not guarantee runtime availability on Linux; unavailable is an expected typed state, not a reason to write plaintext.
