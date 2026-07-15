# Phase 2: Usable Rust Agent Shell — Specification

**Created:** 2026-07-15
**Ambiguity score:** 0.06 (gate: <= 0.20)
**Requirements:** 9 locked

## Goal

A Rust development binary can complete, persist, recover, and present a model conversation through either an interactive terminal shell or deterministic headless JSONL, while preserving strict provider, secret, trace, and architecture boundaries.

## Requirements

1. **Streaming run lifecycle (RUN-01)**
   - Core accepts a provider-neutral stream, emits visible deltas incrementally, honors cancellation, and persists exactly one terminal status.
   - Provider/protocol errors are typed and redacted; partial output is retained locally but is never promoted to completed assistant context.
   - Acceptance: deterministic mock runs cover completion, interruption, timeout, malformed data, premature EOF, and duplicate terminal behavior.

2. **Recoverable sessions (RUN-02)**
   - Users can create, list, resume, continue, interrupt, retry, and finalize sessions after restart.
   - Session and turn IDs are stable; retry creates a new turn linked to the prior turn and never rewrites the original terminal record.
   - Acceptance: restart tests reconstruct history and convert one in-flight model turn into one durable interrupted outcome.

3. **Deterministic local compaction (RUN-03)**
   - Compaction uses a local structured reducer with no Provider call.
   - Summary records include coverage boundary, retained recent turns, stable categories, and before/after estimates.
   - Acceptance: identical inputs produce byte-identical summaries; trace/private reasoning/partial assistant output never enters the summary.

4. **Lease, recovery, and shutdown (RUN-04)**
   - One writer owns the project runtime journal; a second live writer fails closed.
   - Startup repairs only an interrupted final JSONL record, rejects middle corruption, reconciles non-terminal turns, and exposes recovery diagnostics.
   - Controlled shutdown cancels the active run, flushes safe deltas, writes the terminal outcome once, and releases the lease.

5. **Safe folded trace (RUN-05)**
   - Trace stores allowlisted structured event codes and bounded safe facts.
   - No credential, raw provider frame, full tool body, or private chain of thought may enter trace or diagnostics.
   - Acceptance: adversarial secret/reasoning fixtures are absent from serialized trace and terminal output.

6. **Interactive command surface (CLI-01)**
   - The command parser recognizes `/interrupt`, `/new`, `/threads`, `/resume`, `/compact`, `/api`, `/provider`, `/continue`, `/agent`, `/chat`, `/models`, `/model`, `/capabilities`, `/permissions`, `/trace`, `/retry`, and `/exit|/quit`.
   - Phase-owned commands perform real behavior. Later-phase commands return a typed, visible `not_available` result without side effects.

7. **Headless JSONL and exit codes (CLI-02)**
   - One-shot mode does not depend on terminal rendering and writes one strict schema-v1 JSON object per line.
   - Exit codes are stable: 0 completed, 2 usage/config, 3 provider/protocol, 4 interrupted, 5 workspace/recovery/busy.
   - Acceptance: snapshot tests prove TUI and JSONL project the same core events.

8. **Diagnostics and maintenance routing (CLI-03)**
   - `doctor` performs real Phase 2 checks and returns actionable typed results.
   - `migrate`, Vault maintenance, and index maintenance commands are parsed and routed but explicitly report their owning later phase until implemented.
   - Diagnostics never expose secret values or raw provider bodies.

9. **Configuration and credentials (CLI-04)**
   - Non-secret config has one documented precedence chain and rejects unknown fields, invalid limits, and insecure remote HTTP.
   - Credential resolution is environment first, then OS keyring for interactive mode; headless is environment-only.
   - No plaintext credential fallback exists. Debug/error serialization is redacted by construction.

## Boundaries

**In scope:**

- Tokio-based async orchestration and cancellation.
- Direct Responses and Chat Completions HTTP/SSE adapters with rustls.
- File-backed session journal, snapshot/index, workspace lease, startup recovery, and deterministic compaction.
- Safe trace, typed config, environment/keyring credential ports.
- Interactive terminal shell, one-shot JSONL, doctor, stable command parser, and exit codes.
- Fixture/loopback tests only; real adapter construction without paid acceptance calls.

**Out of scope:**

- Executing model-requested tools or exposing permission behavior beyond an explicit later-phase result.
- Vault Wiki compilation, retrieval indexes, project discovery, migration, packaging, or product cutover.
- A background daemon, subagent runtime, plugin/MCP loading, or unrestricted shell.

## Constraints

- Rust 1.97.0, edition 2024, resolver 3, and locked dependencies remain mandatory.
- Core has no HTTP, filesystem, terminal, keyring, or concrete adapter dependency.
- The existing npm binary entry remains unchanged.
- Default tests consume no credentials, Provider quota, or embedding/model download.
- No SQLite/database dependency or plaintext secret persistence.

## Acceptance Criteria

- [ ] `cargo fmt --all -- --check`, workspace Clippy with `-D warnings`, and `cargo test --workspace --locked` pass.
- [ ] A mock one-shot run streams visible deltas and persists one completed terminal event.
- [ ] Cancellation and startup recovery each persist one interrupted terminal event and preserve partial output only as evidence.
- [ ] Create/list/resume/continue/retry/compact survive process reconstruction from disk.
- [ ] A second writer fails while the first lease is alive; the lease is recoverable after process exit.
- [ ] Final-line JSONL truncation repairs; middle corruption fails closed.
- [ ] Compaction is byte-deterministic and makes zero Provider calls.
- [ ] Every slash command and alias parses; later-phase commands are visibly unavailable, not silently successful.
- [ ] Headless JSONL schema and exit-code snapshots pass without TUI initialization.
- [ ] Config/credential tests prove precedence and absence of plaintext/secret diagnostics.
- [ ] Existing 432 TypeScript tests and Phase 1 compatibility/architecture gates remain green.

## Edge Coverage

**Coverage:** 12/12 applicable edges resolved; 0 unresolved

| Category | Edge | Status | Resolution / Acceptance |
|----------|------|--------|-------------------------|
| Provider | premature EOF or duplicate terminal | covered | typed protocol failure; never completed |
| Provider | visible data after terminal | covered | reject as event-after-terminal |
| Provider | cancellation while reading network body | covered | abort transport, flush safe partial delta, persist interrupted once |
| Provider | timeout versus caller cancellation | covered | distinct typed outcomes and exit classes |
| Session | crash after delta before terminal | covered | startup appends one interrupted terminal record |
| Session | retry after terminal or restart | covered | new linked turn ID; old terminal immutable |
| Journal | truncated final JSONL line | covered | quarantine/trim only the final fragment and continue |
| Journal | corrupt middle JSONL record | covered | fail closed with actionable recovery diagnostic |
| Lease | second live process | covered | non-blocking exclusive lock fails with busy status |
| Compaction | budget smaller than required recent turn | covered | typed budget error; never slice an entry |
| Credential | keyring unavailable in headless mode | covered | environment-only resolution; clear missing-key error |
| Terminal | stdout is not a TTY | covered | choose headless-safe rendering; never enable raw terminal mode |

## Prohibitions

**Coverage:** 10/10 applicable prohibitions resolved; 0 unresolved

| Prohibition | Status | Verification |
|-------------|--------|--------------|
| MUST NOT switch the npm product entry | resolved | package/bin snapshot and TypeScript tests |
| MUST NOT add SQLite/database/ORM dependencies | resolved | architecture dependency scan |
| MUST NOT let core import HTTP/filesystem/TUI/keyring adapters | resolved | cargo metadata and source-boundary tests |
| MUST NOT call a real Provider in default tests | resolved | fixture/loopback transport and CI environment scan |
| MUST NOT execute model tool requests in Phase 2 | resolved | provider/core negative test returns later-phase status |
| MUST NOT use a model call for compaction | resolved | counting mock Provider remains at zero |
| MUST NOT serialize plaintext credentials | resolved | secret fixture scan and Debug/JSON tests |
| MUST NOT persist private raw reasoning or raw provider frames | resolved | adversarial trace/journal fixtures |
| MUST NOT fabricate completion after interruption or malformed stream | resolved | exactly-one-terminal reducer tests |
| MUST NOT make terminal rendering part of headless/core behavior | resolved | dependency gate plus JSONL no-TTY integration test |

## Ambiguity Report

| Dimension | Score | Minimum | Status | Notes |
|-----------|-------|---------|--------|-------|
| Goal Clarity | 0.97 | 0.75 | met | Usable conversation and recovery outcome fixed |
| Boundary Clarity | 0.95 | 0.70 | met | Tool/Vault/retrieval/cutover explicitly deferred |
| Constraint Clarity | 0.94 | 0.65 | met | Runtime, secrets, storage, network, and entry gates explicit |
| Acceptance Criteria | 0.93 | 0.70 | met | Behavior and command-level checks enumerated |
| **Ambiguity** | **0.06** | **<= 0.20** | **pass** | Exact dependency patch versions resolve in Cargo.lock |

---

*Phase: MMX-02-usable-rust-agent-shell*
*Spec created: 2026-07-15*
