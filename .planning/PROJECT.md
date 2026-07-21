# MiniMax Codex Rust Rewrite

## What This Is

MiniMax Codex is a Codex-style local CLI/TUI for MiniMax and OpenAI-compatible providers. The Rust product path preserves a recoverable per-project Obsidian-compatible knowledge Vault and gives non-programmers a plain-language way to find open-source projects, Skills, and MCP servers without treating discovery as permission to install or run them.

## Core Value

A non-programmer can describe a goal and safely use one local, recoverable CLI to find the right open-source capability or project and complete the task without losing evidence or long-term knowledge.

## Requirements

### Validated

- The Rust CLI/TUI is the only product path on supported Windows x64 MSVC and Linux x64 GNU hosts; TypeScript-era data remains only as immutable migration fixtures for the two-release support window.
- Exact + BM25 retrieval is active across schema-isolated command, project, and Wiki indexes; optional embedding can only rerank BM25 project candidates.
- Provider/tool event identity, the per-project Vault/Wiki workflow, deterministic fixtures, and the confirm-mode subprocess boundary are release-gated and verified.
- Project, Skill, and MCP source metadata lives in a dedicated `capabilities/` workspace outside internal executable adapters.
- Three typed exact/BM25 indexes preserve kind isolation; optional verified embedding can rerank only their bounded lexical candidate union.
- Read-only workspace search reports `ready`, `needs_install`, or `needs_authorization` with source facts and a safe next action.
- Discovery and prompt augmentation cannot download, install, authorize, start, or execute a discovered capability.
- Phase 10 establishes Rust as the sole executable product and writable runtime authority: npm `dev`, `start`, and the supported bin resolve only to Rust, `.minimax` is the only writable state root, and reviewed JavaScript is limited to distribution orchestration.
- Phase 11 establishes Rust as the sole executable verification and evaluation authority: all 101 historical responsibilities have semantic contracts, Provider/retrieval reports are Rust-owned, and `npm test` cannot reach the hash-pinned TypeScript evaluators.
- Phase 12 establishes fixture-owned Rust compatibility and source-preserving TypeScript-era migration: the complete compatibility module closure is enforced, rollback ownership is durably provenance-bound, and target symlink escapes fail closed.
- Phase 13 establishes a dependency-free npm/native Rust distribution path: deterministic candidates, explicit current fingerprints, eleven fail-closed corruption categories, separate offline installed smokes, and strict CI ordering are release-gated without fallback or runtime download.
- Phase 14 removes the inert TypeScript implementation and test tree, finalizes Rust-only documentation, and binds reproducible hosted Windows MSVC/Linux GNU candidate plus strict evidence to one 235-file product fingerprint.

### Active

- [ ] Shell tool definitions and execution authority are available only while the current process is in `full-access`; `confirm` rejects forged Shell calls before approval or execution.
- [ ] Users can execute one-shot commands through the native platform shell with an optional working directory and bounded initial output.
- [ ] Long-running and interactive commands continue as process-scoped PTY sessions that can be polled, written to, and explicitly stopped.
- [ ] Session count, unread output, aggregate output, command, path, input, and per-result output limits fail predictably without leaking sensitive content.
- [ ] Permission downgrade, normal exit, cancellation, and explicit stop clean up the complete child process tree without leaving background sessions.
- [ ] Shell traces expose safe metadata only, while tests and user documentation cover Windows PowerShell and Linux shell behavior.

### Out of Scope

- SQLite, SQLx, Diesel, ORM, connection pools, or an external database service — the Vault is ordinary files.
- Automatic installation, authorization, or execution of discovered projects, Skills, or MCP servers — discovery must not grant authority.
- A general plugin runtime, model subagents, or background daemon — those are separate execution surfaces and are not required for direct full-access Shell support.
- Application-layer Vault encryption — transparent Markdown is intentional; OS permissions and disk encryption protect the device.
- Bundling an embedding model in the base executable — semantic resources are separately installed and verified.
- macOS v1 support — it follows after keyring, terminal, file replacement, and packaging tests pass.
- Cross-project writable knowledge — every project has one isolated writable Vault.
- Removing npm/Node distribution entirely — npm remains a supported convenience channel, but not a product implementation.
- Adding macOS, ARM, new Providers, new runtime tools, or a capability installer during convergence — this milestone removes duplicate authority rather than expanding features.

## Context

The v1 Rust rewrite, Phase 8 subprocess hardening, Phase 9 capability workspace, Phase 10 executable/source authority boundary, Phase 11 Rust verification authority, Phase 12 fixture-backed migration, Phase 13 thin npm/native release, and Phase 14 final cutover are implemented. Rust is the only product, writable-runtime, executable-evaluation, compatibility, migration, and installed-package authority. The TypeScript product/test tree is gone; only immutable TypeScript-era migration fixtures remain for the machine-enforced two-release support window. Hosted candidate and strict Windows MSVC/Linux GNU evidence now bind the final Rust-only product fingerprint.

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
- **Shell authority**: Arbitrary Shell execution exists only in process-scoped `full-access`; `confirm` neither advertises nor executes Shell tools.
- **Shell implementation**: Use Rust PTY/ConPTY support and native platform shells; do not add Pi, Node/TypeScript, tmux, or an external terminal runtime.
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
| Semantic evidence contracts own TypeScript responsibility retirement | Keep every historical public/safety outcome reviewable without filename-parity ports or broad boilerplate retirement | Locked |
| Discovered TypeScript tests are graph-preflighted before import | Retain transitional static checks while preventing direct or transitive execution of `src/eval/**` | Locked |
| Candidate evidence precedes strict closure | Require a remotely visible pending record before the ordinary strict push run | Locked |
| Windows MSVC builds use reproducible linking | Require `/Brepro` and exact candidate/strict artifact equality | Locked |
| Full-access Shell uses two model tools | Keep one-shot launch separate from poll/write/stop session control | Locked |
| Shell sessions are process-scoped PTYs | Support interactive commands while guaranteeing downgrade and shutdown cleanup | Locked |
| Pi is design reference only | Preserve one Rust runtime and avoid a second Node/TypeScript execution authority | Locked |

## Current Milestone: v3.1 Full Access Shell

**Goal:** Let users run arbitrary one-shot or interactive Shell commands directly in process-scoped `full-access`, with bounded output and reliable session cleanup.

**Target features:**

- `shell_command` for native-shell one-shot execution that yields a session when work continues.
- `shell_session` for incremental polling, stdin writes, and explicit stop.
- Windows ConPTY with PowerShell and Linux PTY with the configured absolute shell plus safe fallbacks.
- Full-access-only schemas and execution preflight, with no per-command confirmation.
- Fixed per-session and aggregate resource bounds, safe trace metadata, process-tree cleanup, TUI status, documentation, and cross-platform verification.

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
*Last updated: 2026-07-21 when milestone v3.1 Full Access Shell started*
