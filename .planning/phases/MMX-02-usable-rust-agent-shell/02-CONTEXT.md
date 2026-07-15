# Phase 2 Context: Usable Rust Agent Shell

**Gathered:** 2026-07-15
**Status:** Ready for planning
**Source:** Master SPEC express path (`.planning/SPEC.md`)

<domain>
## Phase Boundary

Deliver the first actually usable Rust conversation path without switching the public npm entry. A user can run a prompt through a real Provider adapter, observe safe streaming events, interrupt it, restart the process, recover/list/resume/retry/compact sessions, and use either a terminal shell or stable JSONL output. Phase 2 owns runtime/session/CLI behavior only; tool execution, Vault Wiki compilation, retrieval, migration, packaging, and product cutover remain in their roadmap phases.

</domain>

<decisions>
## Implementation Decisions

### Runtime and Provider

- **D-201:** Use a native Rust runtime and direct HTTP adapters. Tokio/cancellation live in adapters and the CLI composition driver; core stays a pure synchronous state machine. Do not add an external agent framework or Provider SDK that duplicates the protocol/core boundary.
- **D-202:** `minimax-core` owns provider-neutral run/session reducers and effects; `minimax-provider` owns async HTTP, SSE, Responses, and Chat Completions wire details.
- **D-203:** One Provider stream produces safe typed events and exactly one terminal outcome. Premature EOF, duplicate terminal, data after terminal, cancellation, timeout, and HTTP/protocol failures stay distinct and redacted.
- **D-204:** Provider tests are deterministic fixtures or loopback transports. Default tests never use a real credential, remote endpoint, or paid request.

### Session Durability and Context

- **D-205:** `minimax-vault` implements the concrete project-local session journal because it is the sole durable file adapter. Phase 2 stores runtime sessions under a caller-selected project root; Phase 4 extends the same ownership boundary into raw evidence and Wiki transactions rather than adding another store.
- **D-206:** A workspace lease is process-scoped and released by the OS on crash. JSONL event appends plus atomically replaced indexes/checkpoints recover a truncated final record but reject corruption in the middle.
- **D-207:** Startup converts a persisted non-terminal model turn into one durable interrupted outcome before accepting new work. Retry creates a new turn identity; it never mutates or duplicates the old terminal turn.
- **D-208:** Compaction is a deterministic local structured reducer. It records a stable short summary, coverage boundary, retained recent turns, and before/after estimates without a Provider call.
- **D-209:** Folded trace contains allowlisted event codes and small safe facts only. Provider frames, credentials, tool output bodies, and private raw reasoning are prohibited.

### CLI, TUI, Configuration, and Credentials

- **D-210:** `minimax-cli` remains the composition root. Headless JSONL and interactive terminal rendering consume the same `RuntimeEventV1` values; core has no terminal formatting.
- **D-211:** Phase 2 uses Clap for stable command/exit-code parsing and Crossterm for a lightweight cross-platform interactive shell. Full-screen visual polish may evolve later without changing commands or core events.
- **D-212:** The Rust binary is a development path through Phase 5. `package.json` and the npm `dist/cli.js` entry remain unchanged until Phase 6 cutover gates pass.
- **D-213:** Non-secret configuration precedence is CLI flag, environment override, project config, user config, then built-in default. Unknown fields and insecure non-loopback HTTP endpoints fail closed.
- **D-214:** Credential precedence is environment first, then OS keyring only for interactive mode. Headless mode is environment-only. No plaintext fallback, serialized secret, or secret-bearing diagnostic is allowed.
- **D-215:** Phase 2 recognizes the complete compatibility command inventory. Commands owned by later phases return an explicit typed `not_available` result rather than executing placeholder side effects.
- **D-216:** Stable headless exit classes are: `0` completed, `2` usage/configuration, `3` Provider/protocol, `4` interrupted, and `5` workspace/recovery/busy.

### the agent's Discretion

- Exact internal module/file split inside each crate.
- Exact bounded sizes for trace facts, JSONL lines, and recent-turn retention, provided the SPEC gates and deterministic tests enforce them.
- Exact terminal colors and prompt glyphs, provided non-TTY/headless behavior remains stable and accessible.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Product and architecture

- `.planning/SPEC.md` — locked rewrite contracts, authorization limits, edge coverage, and prohibitions.
- `.planning/REQUIREMENTS.md` — RUN-01..05 and CLI-01..04 atomic acceptance requirements.
- `.planning/ROADMAP.md` — Phase 2 boundary, three-plan shape, and success criteria.
- `.planning/phases/MMX-01-contract-foundation/01-CONTEXT.md` — crate ownership, pinned toolchain, and TypeScript-entry boundary.
- `.planning/phases/MMX-01-contract-foundation/01-VERIFICATION.md` — verified protocol, architecture, and cross-platform baseline.

### Existing Rust contracts

- `crates/protocol/src/event.rs` — strict schema-v1 stream events, identifiers, usage, and terminal outcomes.
- `crates/core/src/sequence.rs` — exactly-one-terminal reducer and deterministic replay.
- `crates/core/src/ports.rs` — abstract clock/ID port pattern.
- `crates/provider/src/fixture_protocol.rs` — existing wire-to-protocol normalization pattern.
- `crates/compat-harness/src/architecture.rs` — dependency and source-boundary enforcement.

### TypeScript behavioral baseline

- `src/provider/` — Provider profiles, protocols, streaming behavior, and credential-free configuration shape.
- `src/runtime/` — application lifecycle, command routing, recovery, and shutdown behavior.
- `src/session/` — session/index/journal durability baseline.
- `src/ui/` — slash commands and visible event semantics.
- `test/` — compatibility evidence; Rust behavior may differ internally but not silently at the public boundary.

</canonical_refs>

<specifics>
## Specific Ideas

- The first end-to-end proof is a one-shot mock Provider conversation that persists user input, streams two visible deltas, records usage, writes one completed terminal event, and replays as byte-stable JSONL.
- Interactive `/trace` expands only safe structured work evidence; its default view is folded.
- `doctor` reports workspace lease, journal/index recovery, config source, credential source presence (never value), Provider profile validity, and terminal capabilities.
- Real Provider adapter construction is implemented, but automated acceptance uses mock/fixture transports until explicit API-spend authorization exists.

</specifics>

<deferred>
## Deferred Ideas

- Filesystem/shell/Git/npm tool execution and the two permission modes — Phase 3.
- Vault binding, immutable raw evidence, Wiki synthesis, GC, and forget — Phase 4.
- Capability/Wiki/project indexes and embedding resource — Phase 5.
- TypeScript data import, packaging, benchmarks, and default entry cutover — Phase 6.

</deferred>

---

*Phase: MMX-02-usable-rust-agent-shell*
*Context gathered: 2026-07-15 via master SPEC express path*
