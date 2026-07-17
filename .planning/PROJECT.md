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
- Project, Skill, and MCP source metadata lives in a dedicated `capabilities/` workspace outside internal executable adapters.
- Three typed exact/BM25 indexes preserve kind isolation; optional verified embedding can rerank only their bounded lexical candidate union.
- Read-only workspace search reports `ready`, `needs_install`, or `needs_authorization` with source facts and a safe next action.
- Discovery and prompt augmentation cannot download, install, authorize, start, or execute a discovered capability.

### Active

- [ ] Make Rust the only product/runtime implementation and remove the live TypeScript CLI, tests, evaluations, and build dependencies.
- [ ] Keep npm as a thin distribution shell that launches the packaged Rust binary and never falls back to TypeScript.
- [ ] Preserve upgrade safety through the Rust-owned TypeScript-data importer and immutable compatibility fixtures for at least two public releases after cutover.
- [ ] Replace TypeScript verification authority with deterministic Rust tests/evaluations before deleting any covered source.
- [ ] Refresh hosted Windows/Linux release evidence for the final Rust-converged product fingerprint before release.

### Out of Scope

- SQLite, SQLx, Diesel, ORM, connection pools, or an external database service — the Vault is ordinary files.
- Automatic installation, authorization, or execution of discovered projects, Skills, or MCP servers — discovery must not grant authority.
- A general plugin runtime, subagents, daemons, and unrestricted shell in this milestone — they widen the execution surface beyond the capability workspace.
- Application-layer Vault encryption — transparent Markdown is intentional; OS permissions and disk encryption protect the device.
- Bundling an embedding model in the base executable — semantic resources are separately installed and verified.
- macOS v1 support — it follows after keyring, terminal, file replacement, and packaging tests pass.
- Cross-project writable knowledge — every project has one isolated writable Vault.
- Removing npm/Node distribution entirely — npm remains a supported convenience channel, but not a product implementation.
- Adding macOS, ARM, new Providers, new runtime tools, or a capability installer during convergence — this milestone removes duplicate authority rather than expanding features.

## Context

The v1 Rust rewrite, Phase 8 subprocess hardening, and Phase 9 capability workspace are implemented. Rust is already the default product entry, but the repository still contains a live TypeScript implementation, TypeScript tests/evaluations, a legacy CLI entry, and release/compatibility gates that require `dist/cli.js`. That duplicate authority is now the primary reliability risk. v3 removes the live TypeScript product after equivalent Rust verification exists, while retaining a minimal npm launcher/package layer and source-preserving Rust migration support.

## Constraints

- **Compatibility**: Preserve current slash commands, provider protocols, built-in/custom provider profiles, and explicitly migrate durable user data.
- **Architecture**: Core depends on ports only; CLI/TUI do not parse Vault Markdown, Vault never calls Provider, and tools never own the agent loop.
- **Data**: Raw evidence finalizes before Wiki evaluation; every durable Wiki claim has source IDs and a recoverable transaction receipt.
- **Retrieval**: BM25 remains the no-model baseline; project discovery always recalls with BM25 before embedding rerank.
- **Capability workspace**: Project, Skill, and MCP source metadata are physically separate and schema-isolated; runtime installs, credentials, and process state never live in the source catalog.
- **Rust authority**: Product behavior, state ownership, compatibility decisions, tests, and evaluations have one executable authority in Rust.
- **npm boundary**: JavaScript may locate/package a platform Rust binary but may not implement Provider, retrieval, Vault, session, tool, migration, or fallback behavior.
- **Cutover order**: A TypeScript responsibility is deleted only after its Rust replacement and deterministic acceptance gate pass on the current branch.
- **Compatibility support**: TypeScript source data remains importable through Rust for at least two public releases after v3.0; fixtures remain static evidence and are not an executable legacy runtime.
- **Authority**: Search and recommendation are read-only. A result may describe a next action but may never install, authorize, or execute it.
- **Permissions**: Public modes are exactly `confirm` and `full-access`; hard safety gates remain in both.
- **Credentials**: Environment variables override OS keyring; headless systems without keyring accept env only; plaintext persistence is forbidden.
- **Performance**: Cold start <= 500 ms excluding recovery/model load, idle RSS <= 150 MB, base compressed artifact <= 50 MB, and BM25 p95 <= 100 ms at 10k Wiki pages.
- **Execution**: v3 planning starts from the completed Phase 9 branch; implementation proceeds in small verified slices and does not require a branch switch during planning.
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
| Rust is the sole business implementation | Eliminate dual behavior, state, and verification authorities that can drift independently | Locked |
| npm remains a thin Rust distribution shell | Preserve convenient `npm install -g`/`npx` installation without keeping a second product runtime | Locked |
| Current Rust/public contract is the parity baseline | Avoid re-porting dormant or unshipped TypeScript-only behavior | Locked |
| Rust keeps source-preserving legacy-data import | Remove the old executable while protecting existing users and rollback evidence | Locked |
| Windows x64 and Linux x64 remain the release matrix | Keep convergence focused; platform expansion is a separate milestone | Locked |

## Current Milestone: v3.0 Rust Convergence

**Goal:** Make Rust the only executable product and verification authority while retaining npm as a thin, no-fallback distribution shell and preserving safe upgrades from TypeScript-era user data.

**Target features:**

- A source/ownership gate that forbids TypeScript business logic and dual state writers.
- Rust-native tests and deterministic retrieval/Provider evaluations replacing TypeScript verification authority.
- A fixture-driven compatibility harness and Rust migration support that no longer execute `dist/cli.js`.
- An npm package whose only runtime path launches the supported platform Rust binary with clear fail-closed errors.
- Final removal of `.ts/.tsx`, legacy CLI/build dependencies, and TypeScript CI jobs after all replacement gates pass.

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
*Last updated: 2026-07-17 when starting the v3.0 Rust Convergence milestone*
