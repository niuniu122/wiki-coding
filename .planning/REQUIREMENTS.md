# Requirements: MiniMax Codex Rust Rewrite

**Defined:** 2026-07-15
**Core Value:** A non-programmer can safely find the right open-source capability or project and complete work in one recoverable local CLI.

## v1 Requirements

### Architecture

- [x] **ARCH-01**: Maintainer can build one Cargo workspace containing protocol, core, provider, tools, retrieval, vault, tui, cli, and dev-only compat harness crates on Windows and Linux.
- [x] **ARCH-02**: Automated dependency checks prove core has no dependency on UI, HTTP, Markdown paths, or concrete tool adapters.
- [x] **ARCH-03**: All agent operations and observable state changes use typed commands/events with exactly one legal terminal outcome.
- [x] **ARCH-04**: Deterministic clock, ID, mock Provider, and replay fixtures make runtime behavior reproducible offline.

### Compatibility

- [x] **COMP-01**: Existing public slash commands retain compatible names and user-visible outcomes.
- [x] **COMP-02**: Responses and Chat Completions streams parse into the same typed runtime contract and reject malformed or premature terminal sequences.
- [x] **COMP-03**: Built-in MiniMax official/Hashsight and custom OpenAI-compatible provider profiles remain configurable.
- [x] **COMP-04**: Every migrated behavior has a TypeScript/Rust parity fixture or an explicitly approved difference.

### Runtime

- [x] **RUN-01**: One-shot and interactive runs stream visible output, support cancellation, and persist one terminal result.
- [x] **RUN-02**: Users can create, list, resume, continue, interrupt, retry, and finalize sessions after a process restart.
- [x] **RUN-03**: Local deterministic compaction produces a stable short summary without an extra model call and reports retained context.
- [x] **RUN-04**: A single-writer lease, controlled shutdown, and startup recovery prevent concurrent or half-finalized workspace state.
- [x] **RUN-05**: Folded local trace records safe structured work evidence without credentials or private raw chain of thought.

### CLI and TUI

- [x] **CLI-01**: TUI supports `/interrupt`, `/new`, `/threads`, `/resume`, `/compact`, `/api`, `/provider`, `/continue`, `/agent`, `/chat`, `/models`, `/model`, `/capabilities`, `/permissions`, `/trace`, `/retry`, and `/exit|/quit`.
- [x] **CLI-02**: Headless one-shot mode can emit stable JSONL events and meaningful exit codes without TUI dependencies.
- [x] **CLI-03**: `doctor`, `migrate`, Vault maintenance, and index maintenance commands give actionable diagnostics.
- [x] **CLI-04**: Typed configuration has one precedence chain, and credentials resolve env first then OS keyring with env-only headless fallback.

### Tools and Permissions

- [x] **TOOL-01**: Each model tool request has a stable call ID, normalized arguments, durable result, and correct Provider round-trip ordering.
- [x] **TOOL-02**: `confirm` asks before every external tool invocation and returns a structured rejection when declined.
- [x] **TOOL-03**: `full-access` auto-approves allowed tools only for the current process and resets to `confirm` on restart.
- [x] **TOOL-04**: Rust v1 implements read/list, patch/write, bounded shell, Git status/diff, and npm diagnostics.
- [x] **TOOL-05**: Both modes enforce path, schema, secret, destructive-operation, cancellation, and unknown-side-effect hard gates.

### Subprocess Sandbox

- [x] **SBOX-01**: Approval policy and subprocess isolation are independent; `confirm` selects a restricted sandbox, `full-access` disables it only for the current process, and restart returns to `confirm`.
- [x] **SBOX-02**: Every confirm-mode process tool enters an OS-enforced boundary before target code starts, with child network denied, only the project workspace writable, and host-private paths unavailable.
- [x] **SBOX-03**: A missing, unsupported, or failed sandbox backend returns a stable actionable denial before target execution and never falls back to an unsandboxed process.
- [x] **SBOX-04**: Full access explicitly bypasses only the subprocess sandbox and approval prompt while the fixed tool registry and all hard preflight, timeout, output, and cancellation gates remain active.
- [x] **SBOX-05**: Provider HTTP remains host-owned and separate from subprocess networking; Provider secrets and non-allowlisted host environment never enter child processes.
- [x] **SBOX-06**: Doctor, permission/status text, release documentation, and CI truthfully report backend, enforcement, platform support, and remediation.
- [x] **SBOX-07**: Release-gated adversarial tests execute transitive project code and prove confirm-mode host-file/socket denial, fail-closed backend handling, workspace writes, and explicit full-access bypass.

### Vault

- [x] **VAULT-01**: First run lets the user select a per-project Vault, recommends a sibling path outside Git, and binds it with a stable project ID.
- [x] **VAULT-02**: Human-owned inbox and Agent-owned raw/wiki/internal directories have explicit ownership and fail closed on conflicting external edits.
- [x] **VAULT-03**: Raw sessions append recoverable events, finalize before knowledge work, and become immutable with stable hashes.
- [x] **VAULT-04**: Wiki file transactions use manifests, per-file atomic replace, expected hashes, and idempotent roll-forward recovery.
- [x] **VAULT-05**: Inbox import is content-addressed, provenance-preserving, idempotent, and safe for unsupported binary assets.
- [x] **VAULT-06**: GC only reports by default; referenced/pending/pinned evidence is protected, trash is reversible for 7 days, purge reconfirms, and privacy deletion uses `vault forget`.

### Wiki

- [x] **WIKI-01**: Every terminal session receives a durable local durability evaluation and a no-op or pending receipt.
- [x] **WIKI-02**: A separate `MainModelWikiWorkflow` uses the session's pinned main model, reports separate usage, and produces only a structured KnowledgePatch.
- [x] **WIKI-03**: Core validates source IDs, size, ownership, operation, and expected hashes before the Vault writer applies a patch.
- [x] **WIKI-04**: Wiki exposes one current truth per topic, retains supersession provenance, supports lint/rebuild, and excludes superseded pages from normal retrieval.

### Retrieval and Project Discovery

- [x] **RETR-01**: One shared exact/BM25/embedding/RRF engine serves three schema-isolated indexes: capability, open-source project, and Wiki.
- [x] **RETR-02**: Exact + BM25 works offline without an embedding resource and preserves the TypeScript capability ranking baseline within fixture tolerance.
- [x] **RETR-03**: A non-programmer's natural-language need first yields BM25 keywords/candidate projects, then embedding semantically matches and reranks only those candidates.
- [x] **RETR-04**: A concrete local embedding provider validates model ID, version, hash, license, vectors, fingerprint, and Windows/Linux health before activation.
- [x] **RETR-05**: Retrieval reports `exact+bm25`, verified hybrid, or an explicit degraded reason and never treats a feature flag alone as enabled.
- [x] **RETR-06**: Project results explain the match and include source, license, maintenance signals, actual retrieval mode, and deterministic benchmark coverage.

### Migration

- [x] **MIGR-01**: Migration inventory and dry-run report what can move, what is excluded, collisions, target schema, and expected hashes before writing.
- [x] **MIGR-02**: Migration imports safe configuration, sessions, messages, tool events, and capability metadata without secrets, private reasoning, or derived caches.
- [x] **MIGR-03**: Migration is idempotent, leaves source data unchanged, writes receipts, verifies target hashes, and supports rollback by removing only the new target.

### Release and Cutover

- [x] **REL-01**: Windows and Linux receive versioned base artifacts with checksum, install, upgrade, and rollback instructions; embedding stays separate.
- [x] **REL-02**: Recorded benchmarks enforce cold start <= 500 ms, idle RSS <= 150 MB, base compressed artifact <= 50 MB, and 10k-page BM25 p95 <= 100 ms.
- [x] **REL-03**: Offline unit, contract, parity, recovery, security, migration, and cross-platform CI pass without real credentials or API spend.
- [x] **REL-04**: Rust becomes the default entry only after mandatory acceptance gates pass; TypeScript remains usable until cutover and its source data is never deleted automatically.

## v2.0 Requirements

### Capability Workspace

- [x] **CAPW-01**: A maintainer can curate project, Skill, and MCP metadata in one dedicated source workspace whose three catalogs are physically separate from internal tool adapters and runtime state.
- [x] **CAPW-02**: Every external item uses one strict capability-card contract containing kind, intent, source, platform, license, install, authorization, permission, and maintenance facts without inventing unknown values.
- [x] **CAPW-03**: A user can search project, Skill, and MCP catalogs through three schema-isolated exact/BM25 indexes that share the retrieval kernel but cannot accept one another's document types.
- [x] **CAPW-04**: An optional verified embedding resource can rerank only the bounded BM25 candidates from the selected catalogs, and every semantic failure preserves useful lexical results with a stable degraded reason.
- [x] **CAPW-05**: A non-programmer can search all capability kinds or one kind and see `ready`, `needs_install`, or `needs_authorization` plus a plain-language next action for every result.
- [x] **CAPW-06**: Capability status comes from a separate local inventory overlay; source catalogs never contain credentials, mutable install state, or executable process state.
- [x] **CAPW-07**: Search, status, prompt augmentation, and catalog loading are read-only and cannot download, install, authorize, or execute a discovered project, Skill, or MCP server.
- [x] **CAPW-08**: Deterministic fixtures and evaluations cover kind isolation, mixed Chinese/English intent, candidate-only embedding, readiness precedence, unsafe metadata rejection, and CLI text/JSON parity.

## v3.0 Requirements

### Rust Authority

- [x] **RUST-01**: Rust is the only executable product implementation for CLI/TUI, Provider, sessions, tools, Vault/Wiki, retrieval, capability discovery, migration, and compatibility behavior.
- [x] **RUST-02**: JavaScript is confined to an explicit npm launcher/release allowlist and cannot import product-domain source, implement business behavior, download an unverified runtime, or fall back to TypeScript.
- [x] **RUST-03**: `.minimax` and Rust schemas are the only writable runtime authority; no legacy process can write `.mini-codex` state after cutover.

### Rust Verification Authority

- [x] **RVE-01**: Still-required public CLI, lifecycle, Provider, tool, retrieval, and rendering behaviors covered by TypeScript tests are represented by deterministic Rust tests or an explicit documented retirement decision.
- [x] **RVE-02**: Provider conformance evaluation is Rust-owned, fixture-only, machine-readable, and covers the supported Responses and Chat Completions profiles without credentials or API spend.
- [x] **RVE-03**: Retrieval evaluation is Rust-owned and proves exact/BM25 usefulness, labeled mixed-language cases, BM25-before-embedding ordering, candidate-only semantic reranking, and truthful degradation.

### Compatibility and Migration

- [x] **RCMP-01**: The compatibility harness evaluates the current Rust product against immutable public-contract fixtures and explicit differences without building or executing a TypeScript runtime.
- [x] **RCMP-02**: TypeScript-era migration inventory, dry-run, apply, verify, idempotency, collision, interruption recovery, and narrow rollback remain source-preserving and release-gated for at least two public releases after v3.0.

### npm and Native Release

- [x] **RNPM-01**: `npm install -g minimax-codex` and `npx minimax-codex` launch the packaged Rust binary on supported hosts through one no-fallback command entry.
- [x] **RNPM-02**: The npm package has no `minimax-codex-legacy`, `dist/cli.js`, TypeScript compiler/runtime, React/Ink runtime, or TypeScript-only production/build dependency.
- [x] **RNPM-03**: Packed-package verification rejects missing, wrong-platform, renamed, non-executable, or hash-mismatched Rust binaries with actionable errors before release.

### Final Cutover

- [x] **RCUT-01**: The repository contains no TypeScript/TSX product or test source, and CI blocks reintroduction outside static migration data fixtures.
- [x] **RCUT-02**: Windows x64 MSVC and Linux x64 GNU pass Rust tests, evaluations, packaging, installed smoke, checksum, upgrade/rollback, security, license, and performance gates against one final product fingerprint.
- [x] **RCUT-03**: User and maintainer documentation describes Rust-only architecture, npm/native installation, supported platforms, no-fallback failures, migration/rollback, and the two-release compatibility window.

## v3.1 Requirements

### Full Access Shell

- [ ] **SHELL-01**: A user in `full-access` can offer and invoke exactly the `shell_command` and `shell_session` model tools, while `confirm` neither advertises them nor permits a forged Shell call to reach approval or execution.
- [ ] **SHELL-02**: A user can run an arbitrary one-shot command with an optional working directory through PowerShell on Windows or the configured absolute login shell with safe fallbacks on Linux.
- [ ] **SHELL-03**: A command that outlives its initial yield returns a stable process-scoped session ID and preserves bounded incremental PTY output for later reads.
- [ ] **SHELL-04**: A user can poll a running Shell session, write bounded stdin to it, or explicitly stop it and receive one stable structured receipt for every action.
- [ ] **SHELL-05**: Shell execution enforces the fixed session-count, unread-output, aggregate-output, command, working-directory, input, and per-result limits without recording command text or output in safe traces.
- [ ] **SHELL-06**: Permission downgrade, cancellation, normal application exit, and explicit stop interrupt and then terminate the complete child process tree within bounded cleanup waits.
- [ ] **SHELL-07**: Windows and Linux tests, TUI status, public tool contracts, documentation, and CI prove the full-access Shell behavior without adding Pi, Node/TypeScript, tmux, or an external terminal runtime.

## Future Requirements

### Extensions

- **EXT-01**: Add macOS support after its full platform matrix passes.
- **EXT-02**: Add an explicitly confirmed installer/resolver and sandboxed Skill/MCP runtime without weakening the core/tool boundary.
- **EXT-03**: Add explicit read-only global knowledge above isolated project Vaults.

## Out of Scope

| Feature | Reason |
|---------|--------|
| SQLite or another application database | Conflicts with the chosen transparent file Vault |
| Background daemon | v1 is one foreground process and one writer |
| Application-layer Vault encryption | Obsidian-readable files are protected by OS controls |
| Bundled embedding model | Base distribution must remain small and functional without it |
| Automatic project/Skill/MCP installation or execution | Discovery does not grant authority; a later milestone must design confirmation, provenance, rollback, and sandboxing |
| General plugins, model subagents, or background daemons | Separate execution surfaces that are not required for direct full-access Shell support |

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| ARCH-01 | Phase 1 | Complete |
| ARCH-02 | Phase 1 | Complete |
| ARCH-03 | Phase 1 | Complete |
| ARCH-04 | Phase 1 | Complete |
| COMP-01 | Phase 1 | Complete |
| COMP-02 | Phase 1 | Complete |
| COMP-03 | Phase 1 | Complete |
| COMP-04 | Phase 1 | Complete |
| RUN-01 | Phase 2 | Complete |
| RUN-02 | Phase 2 | Complete |
| RUN-03 | Phase 2 | Complete |
| RUN-04 | Phase 2 | Complete |
| RUN-05 | Phase 2 | Complete |
| CLI-01 | Phase 2 | Complete |
| CLI-02 | Phase 2 | Complete |
| CLI-03 | Phase 2 | Complete |
| CLI-04 | Phase 2 | Complete |
| TOOL-01 | Phase 3 | Complete |
| TOOL-02 | Phase 3 | Complete |
| TOOL-03 | Phase 3 | Complete |
| TOOL-04 | Phase 3 | Complete |
| TOOL-05 | Phase 3 | Complete |
| SBOX-01 | Phase 8 | Complete |
| SBOX-02 | Phase 8 | Complete |
| SBOX-03 | Phase 8 | Complete |
| SBOX-04 | Phase 8 | Complete |
| SBOX-05 | Phase 8 | Complete |
| SBOX-06 | Phase 8 | Complete |
| SBOX-07 | Phase 8 | Complete |
| VAULT-01 | Phase 4 | Complete |
| VAULT-02 | Phase 4 | Complete |
| VAULT-03 | Phase 4 | Complete |
| VAULT-04 | Phase 4 | Complete |
| VAULT-05 | Phase 4 | Complete |
| VAULT-06 | Phase 4 | Complete |
| WIKI-01 | Phase 4 | Complete |
| WIKI-02 | Phase 4 | Complete |
| WIKI-03 | Phase 4 | Complete |
| WIKI-04 | Phase 4 | Complete |
| RETR-01 | Phase 5 | Complete |
| RETR-02 | Phase 5 | Complete |
| RETR-03 | Phase 5 | Complete |
| RETR-04 | Phase 5 | Complete |
| RETR-05 | Phase 5 | Complete |
| RETR-06 | Phase 5 | Complete |
| MIGR-01 | Phase 6 | Complete |
| MIGR-02 | Phase 6 | Complete |
| MIGR-03 | Phase 6 | Complete |
| REL-01 | Phase 6 | Complete |
| REL-02 | Phase 6 | Complete |
| REL-03 | Phase 6 | Complete |
| REL-04 | Phase 6 | Complete |
| CAPW-01 | Phase 9 | Complete |
| CAPW-02 | Phase 9 | Complete |
| CAPW-03 | Phase 9 | Complete |
| CAPW-04 | Phase 9 | Complete |
| CAPW-05 | Phase 9 | Complete |
| CAPW-06 | Phase 9 | Complete |
| CAPW-07 | Phase 9 | Complete |
| CAPW-08 | Phase 9 | Complete |
| RUST-01 | Phase 10 | Complete |
| RUST-02 | Phase 10 | Complete |
| RUST-03 | Phase 10 | Complete |
| RVE-01 | Phase 11 | Complete |
| RVE-02 | Phase 11 | Complete |
| RVE-03 | Phase 11 | Complete |
| RCMP-01 | Phase 12 | Complete |
| RCMP-02 | Phase 12 | Complete |
| RNPM-01 | Phase 13 | Complete |
| RNPM-02 | Phase 13 | Complete |
| RNPM-03 | Phase 13 | Complete |
| RCUT-01 | Phase 14 | Complete |
| RCUT-02 | Phase 14 | Complete |
| RCUT-03 | Phase 14 | Complete |
| SHELL-01 | Phase 15 | Pending |
| SHELL-02 | Phase 15 | Pending |
| SHELL-03 | Phase 15 | Pending |
| SHELL-04 | Phase 15 | Pending |
| SHELL-05 | Phase 15 | Pending |
| SHELL-06 | Phase 15 | Pending |
| SHELL-07 | Phase 15 | Pending |

**Coverage:**

- v1 requirements: 52 complete
- v2.0 requirements: 8 complete
- v3.0 requirements: 14 complete
- v3.1 requirements: 7 pending
- Mapped to phases: 81
- Unmapped: 0

---
*Requirements defined: 2026-07-15*
*Last updated: 2026-07-21 when defining milestone v3.1 Full Access Shell*
