# Requirements: MiniMax Codex Rust Rewrite

**Defined:** 2026-07-15
**Core Value:** A non-programmer can safely find the right open-source capability or project and complete work in one recoverable local CLI.

## v1 Requirements

### Architecture

- [ ] **ARCH-01**: Maintainer can build one Cargo workspace containing protocol, core, provider, tools, retrieval, vault, tui, cli, and dev-only compat harness crates on Windows and Linux.
- [ ] **ARCH-02**: Automated dependency checks prove core has no dependency on UI, HTTP, Markdown paths, or concrete tool adapters.
- [ ] **ARCH-03**: All agent operations and observable state changes use typed commands/events with exactly one legal terminal outcome.
- [ ] **ARCH-04**: Deterministic clock, ID, mock Provider, and replay fixtures make runtime behavior reproducible offline.

### Compatibility

- [ ] **COMP-01**: Existing public slash commands retain compatible names and user-visible outcomes.
- [ ] **COMP-02**: Responses and Chat Completions streams parse into the same typed runtime contract and reject malformed or premature terminal sequences.
- [ ] **COMP-03**: Built-in MiniMax official/Hashsight and custom OpenAI-compatible provider profiles remain configurable.
- [ ] **COMP-04**: Every migrated behavior has a TypeScript/Rust parity fixture or an explicitly approved difference.

### Runtime

- [ ] **RUN-01**: One-shot and interactive runs stream visible output, support cancellation, and persist one terminal result.
- [ ] **RUN-02**: Users can create, list, resume, continue, interrupt, retry, and finalize sessions after a process restart.
- [ ] **RUN-03**: Local deterministic compaction produces a stable short summary without an extra model call and reports retained context.
- [ ] **RUN-04**: A single-writer lease, controlled shutdown, and startup recovery prevent concurrent or half-finalized workspace state.
- [ ] **RUN-05**: Folded local trace records safe structured work evidence without credentials or private raw chain of thought.

### CLI and TUI

- [ ] **CLI-01**: TUI supports `/interrupt`, `/new`, `/threads`, `/resume`, `/compact`, `/api`, `/provider`, `/continue`, `/agent`, `/chat`, `/models`, `/model`, `/capabilities`, `/permissions`, `/trace`, `/retry`, and `/exit|/quit`.
- [ ] **CLI-02**: Headless one-shot mode can emit stable JSONL events and meaningful exit codes without TUI dependencies.
- [ ] **CLI-03**: `doctor`, `migrate`, Vault maintenance, and index maintenance commands give actionable diagnostics.
- [ ] **CLI-04**: Typed configuration has one precedence chain, and credentials resolve env first then OS keyring with env-only headless fallback.

### Tools and Permissions

- [ ] **TOOL-01**: Each model tool request has a stable call ID, normalized arguments, durable result, and correct Provider round-trip ordering.
- [ ] **TOOL-02**: `confirm` asks before every external tool invocation and returns a structured rejection when declined.
- [ ] **TOOL-03**: `full-access` auto-approves allowed tools only for the current process and resets to `confirm` on restart.
- [ ] **TOOL-04**: Rust v1 implements read/list, patch/write, bounded shell, Git status/diff, and npm diagnostics.
- [ ] **TOOL-05**: Both modes enforce path, schema, secret, destructive-operation, cancellation, and unknown-side-effect hard gates.

### Vault

- [ ] **VAULT-01**: First run lets the user select a per-project Vault, recommends a sibling path outside Git, and binds it with a stable project ID.
- [ ] **VAULT-02**: Human-owned inbox and Agent-owned raw/wiki/internal directories have explicit ownership and fail closed on conflicting external edits.
- [ ] **VAULT-03**: Raw sessions append recoverable events, finalize before knowledge work, and become immutable with stable hashes.
- [ ] **VAULT-04**: Wiki file transactions use manifests, per-file atomic replace, expected hashes, and idempotent roll-forward recovery.
- [ ] **VAULT-05**: Inbox import is content-addressed, provenance-preserving, idempotent, and safe for unsupported binary assets.
- [ ] **VAULT-06**: GC only reports by default; referenced/pending/pinned evidence is protected, trash is reversible for 7 days, purge reconfirms, and privacy deletion uses `vault forget`.

### Wiki

- [ ] **WIKI-01**: Every terminal session receives a durable local durability evaluation and a no-op or pending receipt.
- [ ] **WIKI-02**: A separate `MainModelWikiWorkflow` uses the session's pinned main model, reports separate usage, and produces only a structured KnowledgePatch.
- [ ] **WIKI-03**: Core validates source IDs, size, ownership, operation, and expected hashes before the Vault writer applies a patch.
- [ ] **WIKI-04**: Wiki exposes one current truth per topic, retains supersession provenance, supports lint/rebuild, and excludes superseded pages from normal retrieval.

### Retrieval and Project Discovery

- [ ] **RETR-01**: One shared exact/BM25/embedding/RRF engine serves three schema-isolated indexes: capability, open-source project, and Wiki.
- [ ] **RETR-02**: Exact + BM25 works offline without an embedding resource and preserves the TypeScript capability ranking baseline within fixture tolerance.
- [ ] **RETR-03**: A non-programmer's natural-language need first yields BM25 keywords/candidate projects, then embedding semantically matches and reranks only those candidates.
- [ ] **RETR-04**: A concrete local embedding provider validates model ID, version, hash, license, vectors, fingerprint, and Windows/Linux health before activation.
- [ ] **RETR-05**: Retrieval reports `exact+bm25`, verified hybrid, or an explicit degraded reason and never treats a feature flag alone as enabled.
- [ ] **RETR-06**: Project results explain the match and include source, license, maintenance signals, actual retrieval mode, and deterministic benchmark coverage.

### Migration

- [ ] **MIGR-01**: Migration inventory and dry-run report what can move, what is excluded, collisions, target schema, and expected hashes before writing.
- [ ] **MIGR-02**: Migration imports safe configuration, sessions, messages, tool events, and capability metadata without secrets, private reasoning, or derived caches.
- [ ] **MIGR-03**: Migration is idempotent, leaves source data unchanged, writes receipts, verifies target hashes, and supports rollback by removing only the new target.

### Release and Cutover

- [ ] **REL-01**: Windows and Linux receive versioned base artifacts with checksum, install, upgrade, and rollback instructions; embedding stays separate.
- [ ] **REL-02**: Recorded benchmarks enforce cold start <= 500 ms, idle RSS <= 150 MB, base compressed artifact <= 50 MB, and 10k-page BM25 p95 <= 100 ms.
- [ ] **REL-03**: Offline unit, contract, parity, recovery, security, migration, and cross-platform CI pass without real credentials or API spend.
- [ ] **REL-04**: Rust becomes the default entry only after mandatory acceptance gates pass; TypeScript remains usable until cutover and its source data is never deleted automatically.

## v2 Requirements

### Extensions

- **EXT-01**: Add macOS support after its full platform matrix passes.
- **EXT-02**: Add MCP and plugin extension points without weakening the core/tool boundary.
- **EXT-03**: Add explicit read-only global knowledge above isolated project Vaults.

## Out of Scope

| Feature | Reason |
|---------|--------|
| SQLite or another application database | Conflicts with the chosen transparent file Vault |
| Background daemon | v1 is one foreground process and one writer |
| Application-layer Vault encryption | Obsidian-readable files are protected by OS controls |
| Bundled embedding model | Base distribution must remain small and functional without it |
| Unrestricted shell, MCP, plugins, subagents | Outside the v1 safety and compatibility contract |

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| ARCH-01 | Phase 1 | Pending |
| ARCH-02 | Phase 1 | Pending |
| ARCH-03 | Phase 1 | Pending |
| ARCH-04 | Phase 1 | Pending |
| COMP-01 | Phase 1 | Pending |
| COMP-02 | Phase 1 | Pending |
| COMP-03 | Phase 1 | Pending |
| COMP-04 | Phase 1 | Pending |
| RUN-01 | Phase 2 | Pending |
| RUN-02 | Phase 2 | Pending |
| RUN-03 | Phase 2 | Pending |
| RUN-04 | Phase 2 | Pending |
| RUN-05 | Phase 2 | Pending |
| CLI-01 | Phase 2 | Pending |
| CLI-02 | Phase 2 | Pending |
| CLI-03 | Phase 2 | Pending |
| CLI-04 | Phase 2 | Pending |
| TOOL-01 | Phase 3 | Pending |
| TOOL-02 | Phase 3 | Pending |
| TOOL-03 | Phase 3 | Pending |
| TOOL-04 | Phase 3 | Pending |
| TOOL-05 | Phase 3 | Pending |
| VAULT-01 | Phase 4 | Pending |
| VAULT-02 | Phase 4 | Pending |
| VAULT-03 | Phase 4 | Pending |
| VAULT-04 | Phase 4 | Pending |
| VAULT-05 | Phase 4 | Pending |
| VAULT-06 | Phase 4 | Pending |
| WIKI-01 | Phase 4 | Pending |
| WIKI-02 | Phase 4 | Pending |
| WIKI-03 | Phase 4 | Pending |
| WIKI-04 | Phase 4 | Pending |
| RETR-01 | Phase 5 | Pending |
| RETR-02 | Phase 5 | Pending |
| RETR-03 | Phase 5 | Pending |
| RETR-04 | Phase 5 | Pending |
| RETR-05 | Phase 5 | Pending |
| RETR-06 | Phase 5 | Pending |
| MIGR-01 | Phase 6 | Pending |
| MIGR-02 | Phase 6 | Pending |
| MIGR-03 | Phase 6 | Pending |
| REL-01 | Phase 6 | Pending |
| REL-02 | Phase 6 | Pending |
| REL-03 | Phase 6 | Pending |
| REL-04 | Phase 6 | Pending |

**Coverage:**
- v1 requirements: 45 total
- Mapped to phases: 45
- Unmapped: 0

---
*Requirements defined: 2026-07-15*
*Last updated: 2026-07-15 after canonical SPEC ingest and final decision lock*
