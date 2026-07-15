# MiniMax Codex Rust Rewrite — Master Specification

**Created:** 2026-07-15
**Canonical design:** `docs/superpowers/specs/2026-07-15-rust-vault-rewrite-design.md`
**Baseline:** TypeScript `84784f5`
**Ambiguity score:** 0.0455 (gate: <= 0.20)
**Requirements:** 45 locked

## Goal

Replace the current TypeScript product path with a Windows/Linux Rust CLI/TUI that preserves user-facing behavior, safely operates a bounded tool set, maintains a per-project Obsidian-compatible evidence/Wiki Vault, and preserves BM25-first open-source project discovery with optional verified embedding.

## Current State

The repository has a working TypeScript + Ink CLI and no Rust source. Exact + BM25 is active for capability retrieval. Embedding interfaces, Granite runtime, vector types, and RRF exist, but no production factory constructs and validates a complete semantic retrieval path. Session, provider, tools, and UI behavior provide the executable compatibility baseline.

## Locked Product Contracts

### 1. Compatibility

The Rust product preserves existing slash commands, Responses and Chat Completions behavior, built-in MiniMax official/Hashsight profiles, custom OpenAI-compatible providers, and explicitly migratable user data. Internal TypeScript structure is not preserved.

### 2. Permissions

The only public values are:

```text
confirm      ask before every external tool invocation
full-access  auto-run allowed tools for this process only
```

Every new process starts in `confirm`. Both modes enforce the same path, schema, secret, destructive-operation, cancellation, and unknown-side-effect hard gates. Rust v1 does not claim an OS sandbox.

### 3. Non-Programmer Open-Source Project Discovery

This workflow is a first-class interface and must not be collapsed into generic capability search:

```text
natural-language need
  -> normalize locally
  -> BM25 extracts/recalls keywords and candidate projects
  -> embedding matches and reranks only the candidate set
  -> policy filters source/license/maintenance
  -> explain why each project matches and report actual mode
```

Without embedding, the workflow still returns BM25 candidates with an explicit degraded reason. Capability, project, and Wiki documents occupy three isolated indexes over one shared retrieval engine.

### 4. Main-Model Wiki Workflow

Every finalized session receives a deterministic local durability evaluation. A no-value session writes a no-op receipt without a model call. A durable session enters an independent `MainModelWikiWorkflow` that uses the session's pinned main Provider/model and displays separate status/model/usage.

The model may only propose a structured `KnowledgePatch`. Core validates raw source IDs, bounds, ownership, operations, and expected hashes. Only the Vault adapter writes files through a recoverable manifest transaction. Vault never calls Provider. A retry uses the same pinned model unless the user explicitly rebinds it.

### 5. Vault Truth Model

Each project binds one writable Vault. `inbox/` is human-owned, finalized `raw/` is immutable evidence, `wiki/` is Agent-compiled current knowledge, `log.md` is operational metadata, and `.minimax/indexes/` is disposable derived data. Wiki claims require raw provenance and one current conclusion per topic; superseded conclusions leave normal retrieval but remain auditable.

### 6. CLI, Tools, and Context

TUI retains all current slash commands. Headless supports one-shot run, JSONL events, doctor, migrate, Vault maintenance, and index maintenance. Rust v1 tools are read/list, patch/write, bounded shell, Git status/diff, and npm diagnostics. MCP, plugins, subagents, daemons, and unrestricted shell are excluded.

Compaction is a deterministic local structured reducer, not a model call. Model context is bounded to a stable short summary, recent turns, relevant Wiki, and capability/project cards. Safe local trace stays folded by default and excludes private raw chain of thought.

### 7. Credentials and Local Protection

Credential precedence is environment variables then OS keyring. A headless environment without keyring accepts environment variables only. No plaintext credential may enter config, Vault, trace, panic output, logs, fixtures, or migration.

Vault Markdown remains readable; the product explains local plaintext risk and recommends OS permissions plus BitLocker/LUKS or equivalent disk encryption.

## Requirements by Delivery Boundary

1. **Contract Foundation (ARCH-01..04, COMP-01..04):** compile the workspace, enforce one-way dependencies, type protocol events, and prove compatibility offline.
2. **Usable Agent Shell (RUN-01..05, CLI-01..04):** stream/cancel/recover sessions, compact locally, expose TUI/headless surfaces, and resolve safe configuration.
3. **Safe Tool Completion (TOOL-01..05):** preserve call identity and execute the complete v1 set under exactly two modes.
4. **Vault and Wiki (VAULT-01..06, WIKI-01..04):** bind a project Vault, preserve evidence, transact Wiki changes, run the pinned main-model workflow, and clean safely.
5. **Retrieval and Project Discovery (RETR-01..06):** serve three isolated indexes, preserve BM25, complete embedding, and explain project matches.
6. **Migration and Release (MIGR-01..03, REL-01..04):** dry-run/import idempotently, package Windows/Linux, enforce benchmarks, and cut over safely.

The atomic statements and one-to-one phase mapping are in `.planning/REQUIREMENTS.md`.

## Boundaries

**In scope:**

- A full Rust Cargo workspace and default product binary.
- Existing provider/session/command compatibility with fixture-proven differences.
- The v1 bounded tool set and two permission modes.
- Per-project Vault, inbox, raw journal, Wiki, recovery, lint/rebuild, GC, and forget.
- Three-domain exact/BM25/optional-embedding retrieval including the non-programmer project finder.
- Explicit TypeScript data migration, Windows/Linux packaging, benchmarks, and cutover.

**Out of scope:**

- SQLite or another state database.
- Bundled embedding weights or mandatory GPU runtime.
- macOS v1, background services, application-layer Vault encryption, cross-project writable knowledge.
- MCP, plugins, subagents, unrestricted shell, and automatic destructive migration.

## Non-Functional Gates

- Cold start <= 500 ms, excluding recovery and embedding model load.
- Idle RSS <= 150 MB.
- Compressed base release artifact <= 50 MB.
- BM25 query p95 <= 100 ms over 10,000 Wiki pages in a recorded benchmark environment.
- Windows and Linux contract, recovery, path, keyring-fallback, and packaging tests pass.
- Default tests use deterministic fixtures and consume no real credentials or API quota.

## Master Acceptance Criteria

- [ ] `cargo test --workspace` and architecture checks pass on the supported toolchain.
- [ ] Every v1 requirement maps to exactly one phase and has an automated or explicit manual verifier.
- [ ] TypeScript/Rust parity report has no unexplained mandatory difference.
- [ ] `/permissions` exposes only confirm/full-access and restart resets full-access.
- [ ] All five v1 tool categories complete a mocked Provider round trip with stable call IDs.
- [ ] A crash at any journal/Wiki transaction boundary converges on restart without duplicate knowledge.
- [ ] The pinned main model is observably responsible for durable Wiki synthesis, while Vault remains Provider-free.
- [ ] Project discovery tests prove BM25 candidate recall occurs before embedding rerank.
- [ ] Missing embedding yields useful BM25 results and truthful degraded status.
- [ ] Migration dry-run/apply/second-apply/rollback preserve source data and exclude secrets.
- [ ] Release performance, security, license, and Windows/Linux packaging gates pass.
- [ ] Rust is not selected as default before all mandatory gates are green.

## Edge Coverage

**Coverage:** 10/10 applicable edges resolved; 0 unresolved

| Category | Edge | Resolution |
|----------|------|------------|
| Provider | premature EOF or duplicate terminal | typed protocol error; never completed |
| Tool | interrupt after possible side effect | record unknown; never fabricate success |
| Permission | restart after full-access | reset to confirm |
| Vault | second writer or project mismatch | fail closed |
| Vault | crash between raw finalization and evaluation marker | startup creates stable missing job |
| Wiki | original pinned model unavailable | pending plus explicit rebind; no silent model change |
| Retrieval | embedding missing/corrupt/stale | BM25 fallback plus degraded reason |
| Project catalog | missing license/source | filter or visibly mark; never invent metadata |
| GC | referenced or pending raw | protected from ordinary GC |
| Migration | target collision or repeat apply | dry-run conflict or idempotent receipt match |

## Prohibitions

| Must-NOT statement | Verification |
|--------------------|--------------|
| MUST NOT add SQLite/SQLx/Diesel/ORM dependencies | dependency/license scan |
| MUST NOT expose a third public permission tier | CLI schema and snapshot tests |
| MUST NOT run embedding before BM25 project candidate recall | staged retriever contract test |
| MUST NOT let Vault call Provider or TUI parse Vault files | dependency checks |
| MUST NOT write credentials or private raw reasoning | redaction/secret fixtures and review |
| MUST NOT auto-delete raw evidence or run destructive migration | GC/migration negative tests |
| MUST NOT report hybrid from configuration alone | health/fingerprint contract tests |
| MUST NOT push, open a PR, spend API quota, or download model weights under current authorization | execution log and git/network review |

## Ambiguity Report

| Dimension | Score | Minimum | Status | Notes |
|-----------|-------|---------|--------|-------|
| Goal Clarity | 0.98 | 0.75 | met | Full replacement and user value fixed |
| Boundary Clarity | 0.95 | 0.70 | met | v1 tools, platforms, database, and extensions explicit |
| Constraint Clarity | 0.94 | 0.65 | met | data, permission, model, performance, and authorization gates explicit |
| Acceptance Criteria | 0.93 | 0.70 | met | 45 atomic requirements plus master checks |
| **Ambiguity** | **0.0455** | **<= 0.20** | **pass** | Weighted clarity 0.9545 |

## Interview Log

| Round | Question summary | Decision locked |
|-------|------------------|-----------------|
| Architecture | Database or local knowledge files? | Per-project Obsidian Vault, no SQLite |
| Retrieval | Preserve current discovery behavior? | BM25 keywords/candidates then embedding project match |
| Permissions | How many user-facing modes? | Exactly confirm and full-access |
| Knowledge | Which model summarizes Wiki? | Current pinned main model in a separate workflow |
| Scope | Which Rust v1 tools and interfaces? | Bounded five-category tool set, slash TUI, headless JSONL/maintenance |
| Operations | How retain and clean raw data? | Weekly report, reference protection, 7-day trash, separate forget |
| Release | What constitutes mature CLI? | Compatibility, recovery, packaging, security, and fixed performance gates |
| Execution | How to implement? | Isolated branch/worktree, vertical slices, local atomic commits, no push/PR |

---
*Next: Phase 1 SPEC and executable plans*
