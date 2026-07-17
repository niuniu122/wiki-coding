# MiniMax Codex Rust Rewrite

## What This Is

MiniMax Codex is a Codex-style local CLI/TUI for MiniMax and OpenAI-compatible providers. The Rust product path preserves a recoverable per-project Obsidian-compatible knowledge Vault and gives non-programmers a plain-language way to find open-source projects, Skills, and MCP servers without treating discovery as permission to install or run them.

## Core Value

A non-programmer can describe a goal and safely use one local, recoverable CLI to find the right open-source capability or project and complete the task without losing evidence or long-term knowledge.

## Requirements

### Validated

- The Rust CLI/TUI is the default product path on Windows and Linux, with the TypeScript path retained explicitly for the support window.
- Exact + BM25 retrieval is active across schema-isolated command, project, and Wiki indexes; optional embedding can only rerank BM25 project candidates.
- Provider/tool event identity, the per-project Vault/Wiki workflow, deterministic fixtures, and the confirm-mode subprocess boundary are release-gated and verified.

### Active

- [ ] Separate source-controlled project, Skill, and MCP metadata into one dedicated capability workspace outside `crates/tools`.
- [ ] Search three typed external-capability indexes with exact/BM25 first and optional verified embedding over candidates only.
- [ ] Show non-programmers whether a result is ready, needs installation, or needs authorization, with a safe next action.
- [ ] Keep discovery read-only; installation, authorization, and execution remain separate explicitly confirmed workflows.

### Out of Scope

- SQLite, SQLx, Diesel, ORM, connection pools, or an external database service — the Vault is ordinary files.
- Automatic installation, authorization, or execution of discovered projects, Skills, or MCP servers — discovery must not grant authority.
- A general plugin runtime, subagents, daemons, and unrestricted shell in this milestone — they widen the execution surface beyond the capability workspace.
- Application-layer Vault encryption — transparent Markdown is intentional; OS permissions and disk encryption protect the device.
- Bundling an embedding model in the base executable — semantic resources are separately installed and verified.
- macOS v1 support — it follows after keyring, terminal, file replacement, and packaging tests pass.
- Cross-project writable knowledge — every project has one isolated writable Vault.

## Context

The v1 Rust rewrite and Phase 8 subprocess hardening are complete. External discovery currently has only a bundled project catalog; internal commands live in the capability index, while Skills and MCP servers have no typed catalog, isolated index, or user-facing readiness state. The new workspace combines Codex-style progressive capability metadata with the existing BM25-first, candidate-only embedding contract without copying third-party runtimes into the core tool boundary.

## Constraints

- **Compatibility**: Preserve current slash commands, provider protocols, built-in/custom provider profiles, and explicitly migrate durable user data.
- **Architecture**: Core depends on ports only; CLI/TUI do not parse Vault Markdown, Vault never calls Provider, and tools never own the agent loop.
- **Data**: Raw evidence finalizes before Wiki evaluation; every durable Wiki claim has source IDs and a recoverable transaction receipt.
- **Retrieval**: BM25 remains the no-model baseline; project discovery always recalls with BM25 before embedding rerank.
- **Capability workspace**: Project, Skill, and MCP source metadata are physically separate and schema-isolated; runtime installs, credentials, and process state never live in the source catalog.
- **Authority**: Search and recommendation are read-only. A result may describe a next action but may never install, authorize, or execute it.
- **Permissions**: Public modes are exactly `confirm` and `full-access`; hard safety gates remain in both.
- **Credentials**: Environment variables override OS keyring; headless systems without keyring accept env only; plaintext persistence is forbidden.
- **Performance**: Cold start <= 500 ms excluding recovery/model load, idle RSS <= 150 MB, base compressed artifact <= 50 MB, and BM25 p95 <= 100 ms at 10k Wiki pages.
- **Execution**: v2 work occurs on `codex/capability-workspace-v2` from the completed Phase 8 baseline, with atomic local commits after verified slices.
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
| Dedicated external capability workspace | Keep third-party metadata and future install state out of the fixed internal tool adapters | Locked |
| Three readiness labels | Translate runtime prerequisites into ready, needs install, or needs authorization for non-programmers | Locked |
| Discovery never grants execution authority | Prevent a recommendation from silently becoming a download, credential request, or process launch | Locked |

## Current Milestone: v2.0 Capability Workspace

**Goal:** Let a non-programmer search one safe external-tool workspace for projects, Skills, and MCP servers while preserving typed isolation, BM25-first retrieval, and explicit authority boundaries.

**Target features:**

- Source-controlled `capabilities/` catalogs for projects, Skills, and MCP servers, separate from runtime tools and user state.
- One strict capability-card contract with three isolated indexes and optional candidate-only embedding reranking.
- Readiness and next-action output that says ready, needs installation, or needs authorization without taking the action.

## Evolution

This document evolves at phase transitions and milestone boundaries.

**After each phase transition:**
1. Move requirements proven by tests to Validated.
2. Move rejected or deferred work to Out of Scope with a reason.
3. Record new decisions that constrain later installer or runtime work.
4. Confirm the product description and authority boundary remain accurate.

**After each milestone:**
1. Recheck the Core Value and all active requirements.
2. Audit Out of Scope items before promoting them.
3. Update context with shipped behavior and evaluation evidence.

---
*Last updated: 2026-07-17 after starting the v2.0 Capability Workspace milestone*
