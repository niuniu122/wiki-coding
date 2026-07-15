# MiniMax Codex Rust Rewrite

## What This Is

MiniMax Codex is a Codex-style local CLI/TUI for MiniMax and OpenAI-compatible providers. This milestone replaces the TypeScript product path with a mature Rust command-line tool while preserving its behavior, adding a per-project Obsidian-compatible knowledge Vault, and retaining the non-programmer workflow that finds suitable open-source projects from a natural-language need.

## Core Value

A non-programmer can describe a goal and safely use one local, recoverable CLI to find the right open-source capability or project and complete the task without losing evidence or long-term knowledge.

## Requirements

### Validated

- The TypeScript baseline at `84784f5` can run interactive MiniMax/OpenAI-compatible sessions.
- Exact + BM25 capability retrieval is active in production construction.
- Provider/tool event identity and deterministic evaluation fixtures exist and serve as the compatibility reference.

### Active

- [ ] Replace the main product path with a Windows/Linux Rust binary while preserving public workflows.
- [ ] Separate protocol, core, providers, tools, retrieval, Vault, TUI, and CLI behind one-way ports.
- [ ] Provide only `confirm` and session-scoped `full-access` permission modes.
- [ ] Store recoverable raw evidence and main-model-compiled Wiki knowledge in a per-project Obsidian-compatible Vault without SQLite.
- [ ] Preserve BM25-first, embedding-second open-source project discovery for non-programmers.
- [ ] Migrate safely, meet release performance gates, and cut over only after parity is proven.

### Out of Scope

- SQLite, SQLx, Diesel, ORM, connection pools, or an external database service — the Vault is ordinary files.
- MCP, plugins, subagents, daemons, and unrestricted shell in Rust v1 — they would widen the safety and compatibility surface.
- Application-layer Vault encryption — transparent Markdown is intentional; OS permissions and disk encryption protect the device.
- Bundling an embedding model in the base executable — semantic resources are separately installed and verified.
- macOS v1 support — it follows after keyring, terminal, file replacement, and packaging tests pass.
- Cross-project writable knowledge — every project has one isolated writable Vault.

## Context

The existing repository is TypeScript + Ink and contains no Rust source at milestone start. The current capability pipeline constructs exact + BM25 retrieval; embedding abstractions, Granite runtime, vectors, and RRF exist but are not wired through the production factory. The rewrite uses OpenAI Codex's thin surfaces and typed operation/event boundaries, claw-code-style deterministic parity fixtures, and Karpathy Wiki's raw-evidence/compiled-knowledge separation without copying Codex's SQLite layer.

## Constraints

- **Compatibility**: Preserve current slash commands, provider protocols, built-in/custom provider profiles, and explicitly migrate durable user data.
- **Architecture**: Core depends on ports only; CLI/TUI do not parse Vault Markdown, Vault never calls Provider, and tools never own the agent loop.
- **Data**: Raw evidence finalizes before Wiki evaluation; every durable Wiki claim has source IDs and a recoverable transaction receipt.
- **Retrieval**: BM25 remains the no-model baseline; project discovery always recalls with BM25 before embedding rerank.
- **Permissions**: Public modes are exactly `confirm` and `full-access`; hard safety gates remain in both.
- **Credentials**: Environment variables override OS keyring; headless systems without keyring accept env only; plaintext persistence is forbidden.
- **Performance**: Cold start <= 500 ms excluding recovery/model load, idle RSS <= 150 MB, base compressed artifact <= 50 MB, and BM25 p95 <= 100 ms at 10k Wiki pages.
- **Execution**: Work occurs on `codex/rust-rewrite` from `84784f5`, with an atomic local commit after each verified slice.
- **Authorization**: No push/PR, real API spend, embedding model download, or destructive migration without fresh approval.

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Full Rust replacement | Mature distribution and enforceable architecture boundaries | Locked |
| Per-project file Vault, no SQLite | Obsidian readability and no heavyweight state database | Locked |
| Raw evidence plus compiled Wiki | Auditability without flooding everyday context | Locked |
| Main model owns a separate Wiki synthesis workflow | Knowledge summaries should reflect the active agent while writes remain validated | Locked |
| Exactly two permission modes | Match the user's Codex-like mental model without a confusing third tier | Locked |
| Three isolated retrieval indexes | Prevent capability, project, and Wiki results from contaminating each other | Locked |
| BM25 recall before embedding project matching | Preserve the non-programmer discovery behavior and keep a truthful fallback | Locked |
| Vertical parity slices | Keep TypeScript usable until every Rust boundary is proven | Locked |

---
*Last updated: 2026-07-15 after final rewrite decisions and canonical SPEC ingest*
