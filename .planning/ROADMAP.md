# Roadmap: MiniMax Codex Rust Rewrite

## Overview

The milestone proceeds as six vertical, verifiable boundaries: freeze compatibility and core contracts, deliver a usable Rust conversation shell, add safe tools, add the per-project Vault and main-model Wiki workflow, restore and complete three-domain hybrid retrieval, then migrate/package/cut over. TypeScript remains the runnable reference until Phase 6 gates pass.

## Phases

- [x] **Phase 1: Contract Foundation** - Compile the Rust workspace and prove typed protocol/parity contracts offline. (completed 2026-07-15)
- [x] **Phase 2: Usable Rust Agent Shell** - Run, stream, resume, compact, diagnose, and operate through TUI or headless CLI. (completed 2026-07-15)
- [x] **Phase 3: Safe Tool Completion** - Execute the v1 tool set under exactly two permission modes. (completed 2026-07-15)
- [x] **Phase 4: Vault and Main-Model Wiki** - Persist raw evidence and maintain recoverable Obsidian knowledge through the pinned main model. (completed 2026-07-16)
- [x] **Phase 5: Retrieval and Project Discovery** - Serve three isolated indexes and preserve BM25-first project finding with truthful embedding. (completed 2026-07-16)
- [x] **Phase 6: Migration, Release, and Cutover** - Import safely, meet release gates, and switch the default entry only after parity. (completed 2026-07-16)

## Phase Details

### Phase 1: Contract Foundation

**Goal**: Maintainers can build and test a one-way Rust workspace whose protocol and compatibility fixtures make later slices independently verifiable.
**Depends on**: Nothing
**Requirements**: ARCH-01, ARCH-02, ARCH-03, ARCH-04, COMP-01, COMP-02, COMP-03, COMP-04
**Success Criteria** (what must be TRUE):

  1. A clean Windows/Linux Rust toolchain can compile and test the workspace without modifying the TypeScript entry.
  2. Mock Responses and Chat Completions fixtures converge on one typed event contract and reject illegal terminal sequences.
  3. A parity report lists every public command/provider behavior as matched, pending, or explicitly different.
  4. Dependency checks fail if core imports an adapter crate.

**Plans**: 4/4 plans executed

Plans:

- [x] 01-01-PLAN.md
- [x] 01-02-PLAN.md
- [x] 01-03-PLAN.md
- [x] 01-04-PLAN.md

**Wave 1**

- [x] 01-01: Freeze TypeScript compatibility manifests and offline fixtures
- [x] 01-02: Scaffold Cargo workspace and typed protocol/core boundaries

**Wave 2** *(blocked on Wave 1 completion)*

- [x] 01-03: Implement typed protocol/core sequence and provider fixture normalization

**Wave 3** *(blocked on Wave 2 completion)*

- [x] 01-04: Build deterministic compat harness, architecture gates, and CI integration

Cross-cutting constraints:

- TypeScript remains the product entry and all verification stays fixture-only.
- Rust uses pinned 1.97.0/edition 2024 with no database dependency.

### Phase 2: Usable Rust Agent Shell

**Goal**: Users can complete and recover a model conversation through either the interactive shell or stable headless output.
**Depends on**: Phase 1
**Requirements**: RUN-01, RUN-02, RUN-03, RUN-04, RUN-05, CLI-01, CLI-02, CLI-03, CLI-04
**Success Criteria** (what must be TRUE):

  1. A user can run one prompt, see streaming output, interrupt it, and receive one durable terminal status.
  2. A user can create, list, resume, continue, retry, and compact sessions after restart.
  3. TUI slash commands and headless JSONL share the same core events without rendering logic in core.
  4. Configuration and credentials resolve predictably without writing plaintext secrets.
  5. Startup recovery, single-writer lease, controlled shutdown, and safe folded trace are observable through doctor/tests.

**Plans**: 3/3 plans executed

Plans:

- [x] 02-01-PLAN.md
- [x] 02-02-PLAN.md
- [x] 02-03-PLAN.md

**Wave 1**

- [x] 02-01: Implement provider adapters and the minimal streaming runtime

**Wave 2** *(blocked on Wave 1 completion)*

- [x] 02-02: Add durable sessions, recovery, deterministic compaction, and trace

**Wave 3** *(blocked on Wave 2 completion)*

- [x] 02-03: Add compatible TUI/headless surfaces, diagnostics, configuration, and credentials

### Phase 3: Safe Tool Completion

**Goal**: Users can let the model perform the complete Rust v1 tool set with understandable approval behavior and recoverable call identity.
**Depends on**: Phase 2
**Requirements**: TOOL-01, TOOL-02, TOOL-03, TOOL-04, TOOL-05
**Success Criteria** (what must be TRUE):

  1. `/permissions` exposes only `confirm` and `full-access`, and restart always returns to `confirm`.
  2. Confirm mode asks before every external tool; full access skips prompts only for the current process.
  3. Read/list, patch/write, bounded shell, Git status/diff, and npm diagnostics complete the Provider tool loop with stable call IDs.
  4. Rejection, cancellation, schema failure, path escape, secret detection, and unknown side effects remain hard-gated in both modes.

**Plans**: 2/2 plans executed

Plans:
**Wave 1**

- [x] 03-01: Implement tool state machine, approval protocol, and two-mode policy

**Wave 2** *(blocked on Wave 1 completion)*

- [x] 03-02: Implement and verify the bounded v1 tool adapters

### Phase 4: Vault and Main-Model Wiki

**Goal**: Every project has recoverable raw evidence and current Wiki knowledge produced by a separately visible workflow using that session's pinned main model.
**Depends on**: Phase 3
**Requirements**: VAULT-01, VAULT-02, VAULT-03, VAULT-04, VAULT-05, VAULT-06, WIKI-01, WIKI-02, WIKI-03, WIKI-04
**Success Criteria** (what must be TRUE):

  1. A user can bind one project Vault, recover finalized raw sessions, and inspect its ordinary Markdown in Obsidian.
  2. Inbox import, Wiki transactions, and crash recovery are idempotent and provenance-preserving.
  3. The pinned main model runs a separate Wiki synthesis workflow with separate usage; Vault never invokes a Provider.
  4. Normal retrieval sees one current truth while raw sources and supersession remain auditable.
  5. GC never auto-deletes raw, protects reachable evidence, supports 7-day undo, and keeps privacy deletion separate.

**Plans**: 3/3 plans complete

Plans:

- [x] 04-01-PLAN.md
- [x] 04-02-PLAN.md
- [x] 04-03-PLAN.md

**Wave 1**

- [x] 04-01: Implement Vault bootstrap, lease, raw journal, and recoverable file transactions

**Wave 2** *(blocked on Wave 1 completion)*

- [x] 04-02: Implement inbox, durability gate, MainModelWikiWorkflow, provenance, and supersession

**Wave 3** *(blocked on Wave 2 completion)*

- [x] 04-03: Implement lint/rebuild plus GC, trash/undo/purge, and forget workflows

### Phase 5: Retrieval and Project Discovery

**Goal**: Non-programmers can describe a need and receive explainable open-source project matches from BM25 candidates with optional verified semantic reranking.
**Depends on**: Phase 4
**Requirements**: RETR-01, RETR-02, RETR-03, RETR-04, RETR-05, RETR-06
**Success Criteria** (what must be TRUE):

  1. Capability, open-source project, and Wiki indexes share one engine but cannot return one another's document schema.
  2. Exact + BM25 remains useful with no model resource and matches current capability fixtures.
  3. Project discovery visibly performs BM25 keyword/candidate recall before embedding rerank and explains every result.
  4. Missing, damaged, incompatible, or slow embedding resources degrade truthfully without crashing.
  5. A verified local model/vector fingerprint is required before hybrid mode appears.

**Plans**: 3/3 plans executed

Plans:

- [x] 05-01-PLAN.md
- [x] 05-02-PLAN.md
- [x] 05-03-PLAN.md

- [x] 05-01: Port exact/BM25/RRF core and build three isolated lexical indexes
- [x] 05-02: Restore project catalog workflow and wire a concrete optional embedding provider
- [x] 05-03: Add truthful status, source/license/maintenance explanations, and retrieval benchmarks

### Phase 6: Migration, Release, and Cutover

**Goal**: Users can migrate without losing or exposing data and install a release-gated Rust binary as the default product.
**Depends on**: Phase 5
**Requirements**: MIGR-01, MIGR-02, MIGR-03, REL-01, REL-02, REL-03, REL-04
**Success Criteria** (what must be TRUE):

  1. Dry-run and applied migration are idempotent, auditable, secret-safe, and leave TypeScript source data untouched.
  2. Windows/Linux artifacts include checksums and reproducible install, upgrade, and rollback instructions.
  3. Offline CI and recorded performance gates pass without real API calls or bundled embedding weight.
  4. Rust becomes default only when the acceptance matrix is green; rollback keeps evidence and receipts.

**Plans**: 3/3 plans executed

Plans:

- [x] 06-01: Implement inventory, dry-run, import, receipt verification, and rollback
- [x] 06-02: Build cross-platform packaging, CI, security, license, and performance gates
- [x] 06-03: Complete parity, cutover, upgrade, rollback, and support-window documentation

## Progress

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Contract Foundation | 4/4 | Complete    | 2026-07-15 |
| 2. Usable Rust Agent Shell | 3/3 | Complete   | 2026-07-15 |
| 3. Safe Tool Completion | 2/2 | Complete | 2026-07-15 |
| 4. Vault and Main-Model Wiki | 3/3 | Complete   | 2026-07-16 |
| 5. Retrieval and Project Discovery | 3/3 | Complete   | 2026-07-16 |
| 6. Migration, Release, and Cutover | 3/3 | Complete | 2026-07-16 |
| 7. Close Milestone Integration Gaps | 3/4 | In progress | - |

### Phase 7: Close milestone integration gaps

**Goal:** Make the already-built runtime, Vault/Wiki, retrieval, command, distribution, and release-evidence components one complete installable product flow.
**Requirements**: COMP-01, COMP-04, RUN-02, CLI-01, VAULT-01, VAULT-03, WIKI-01, WIKI-02, WIKI-03, WIKI-04, RETR-03, REL-01, REL-03, REL-04
**Depends on:** Phase 6
**Success Criteria** (what must be TRUE):

  1. A terminal runtime session finalizes into its bound Vault, receives a durable Wiki receipt, and uses the same pinned main-model Provider only in the separately visible Wiki workflow.
  2. A natural-language request can reach the bundled BM25-first project catalog without expert paths; optional embeddings remain verified and candidate-only.
  3. Locked command outcomes are executable or recorded as explicit tested differences, including functional retry and lifecycle finalization.
  4. One official npm artifact contains the fixed Rust launcher, native binary, and explicit TypeScript legacy command; its installed default is smoke-tested.
  5. Machine-readable hosted evidence is bound to the cutover product fingerprint and final Windows/Linux jobs.

**Plans:** 3/4 plans complete

Plans:

- [x] 07-01: Wire runtime finalization, Vault binding, and the production main-model Wiki workflow
- [x] 07-02: Restore automatic BM25-first project discovery and executable command contracts
- [x] 07-03: Build one complete Rust-plus-legacy distribution and bind hosted cutover evidence
- [ ] 07-04: Run cross-phase chain tests, hosted native gates, and the final milestone audit
