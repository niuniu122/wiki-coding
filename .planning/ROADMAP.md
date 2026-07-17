# Roadmap: MiniMax Codex Rust Rewrite

## Overview

The product proceeds through fourteen vertical, verifiable boundaries. Eight completed v1 phases established the Rust runtime, safe tools, Vault/Wiki, hybrid retrieval, release cutover, and subprocess sandbox; the completed v2 phase added a read-only capability workspace. v3.0 Rust Convergence now moves authority in five ordered slices: establish Rust-only execution and state boundaries, replace TypeScript verification, preserve fixture-based compatibility and migration, reduce npm to a no-fallback native launcher, and only then delete the inert TypeScript tree before refreshing final hosted evidence.

## Phases

- [x] **Phase 1: Contract Foundation** - Compile the Rust workspace and prove typed protocol/parity contracts offline. (completed 2026-07-15)
- [x] **Phase 2: Usable Rust Agent Shell** - Run, stream, resume, compact, diagnose, and operate through TUI or headless CLI. (completed 2026-07-15)
- [x] **Phase 3: Safe Tool Completion** - Execute the v1 tool set under exactly two permission modes. (completed 2026-07-15)
- [x] **Phase 4: Vault and Main-Model Wiki** - Persist raw evidence and maintain recoverable Obsidian knowledge through the pinned main model. (completed 2026-07-16)
- [x] **Phase 5: Retrieval and Project Discovery** - Serve three isolated indexes and preserve BM25-first project finding with truthful embedding. (completed 2026-07-16)
- [x] **Phase 6: Migration, Release, and Cutover** - Import safely, meet release gates, and switch the default entry only after parity. (completed 2026-07-16)
- [x] **Phase 7: Close Milestone Integration Gaps** - Wire the product flows, prove the exact final artifacts, and pass the repeated milestone audit. (completed 2026-07-16)
- [x] **Phase 8: Codex-style subprocess sandbox hardening** - Separate approval from isolation and enforce or fail closed for every confirm-mode process tool. (completed 2026-07-17)
- [x] **Phase 9: Capability Workspace and Non-Programmer Harness** - Isolate project/Skill/MCP catalogs, retrieve them BM25-first, and explain safe readiness without automatic execution.
- [ ] **Phase 10: Rust Authority and Source Boundaries** - Make Rust the sole executable and writable product authority while constraining JavaScript to an explicit distribution allowlist.
- [ ] **Phase 11: Rust Verification and Evaluation Authority** - Replace still-required TypeScript behavioral, Provider, and retrieval verification with deterministic Rust-owned gates.
- [ ] **Phase 12: Fixture Compatibility and Rust Migration** - Verify compatibility and TypeScript-era upgrades from immutable fixtures without building or executing the legacy runtime.
- [ ] **Phase 13: Thin npm and Native Release** - Ship one no-fallback npm command that launches a verified platform Rust binary and rejects invalid packages before release.
- [ ] **Phase 14: TypeScript Removal and Hosted Closure** - Delete the inert TypeScript implementation, converge CI and documentation, and bind final Windows/Linux evidence to one product fingerprint.

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
| 7. Close Milestone Integration Gaps | 4/4 | Complete | 2026-07-16 |
| 8. Codex-style Subprocess Sandbox Hardening | 3/3 | Complete | 2026-07-17 |
| 9. Capability Workspace and Non-Programmer Harness | 3/3 | Complete | 2026-07-17 |
| 10. Rust Authority and Source Boundaries | 2/3 | In Progress|  |
| 11. Rust Verification and Evaluation Authority | 0/3 | Not started | - |
| 12. Fixture Compatibility and Rust Migration | 0/2 | Not started | - |
| 13. Thin npm and Native Release | 0/3 | Not started | - |
| 14. TypeScript Removal and Hosted Closure | 0/3 | Not started | - |

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

**Plans:** 4/4 plans complete

Plans:

- [x] 07-01: Wire runtime finalization, Vault binding, and the production main-model Wiki workflow
- [x] 07-02: Restore automatic BM25-first project discovery and executable command contracts
- [x] 07-03: Build one complete Rust-plus-legacy distribution and bind hosted cutover evidence
- [x] 07-04: Run cross-phase chain tests, hosted native gates, and the final milestone audit

### Phase 8: Codex-style subprocess sandbox hardening

**Goal:** Replace ordinary host child processes with an operating-system-enforced, fail-closed confirm-mode boundary while retaining an explicit full-access bypass and every existing hard gate.
**Requirements**: SBOX-01, SBOX-02, SBOX-03, SBOX-04, SBOX-05, SBOX-06, SBOX-07
**Depends on:** Phase 7
**Success Criteria** (what must be TRUE):

  1. Approval and sandbox policy are separate; confirm snapshots a restricted policy, full access snapshots disabled policy, and restart returns to confirm.
  2. Linux Bubblewrap canaries prove transitive build code cannot read a host marker or reach a host-local socket while workspace writes still work.
  3. Missing, unsupported, or failed sandbox backends reject before target start with no unsandboxed fallback.
  4. Full access bypasses only the subprocess sandbox and user prompt; all fixed-tool and hard preflight gates stay active.
  5. Doctor, permission text, release docs, and CI truthfully report the platform/backend boundary, and unrelated Provider/retrieval/Vault/Wiki regressions stay green.

**Plans:** 3/3 plans complete

Plans:

- [x] 08-01: Introduce the independent sandbox policy and fail-closed launch contract
- [x] 08-02: Implement Linux Bubblewrap enforcement and adversarial canaries
- [x] 08-03: Wire truthful diagnostics, documentation, CI, and full regression gates

### Phase 9: Capability Workspace and Non-Programmer Harness

**Goal:** Let a non-programmer search a dedicated external capability workspace for open-source projects, Skills, and MCP servers, understand what is ready or still required, and remain protected from implicit installation, authorization, or execution.
**Requirements**: CAPW-01, CAPW-02, CAPW-03, CAPW-04, CAPW-05, CAPW-06, CAPW-07, CAPW-08
**Depends on:** Phase 8
**Success Criteria** (what must be TRUE):

  1. Source-controlled project, Skill, and MCP catalogs live under one dedicated `capabilities/` root and never share mutable runtime state or internal tool-adapter code.
  2. Three typed indexes share exact/BM25 algorithms but reject cross-kind documents; optional embedding observes and reranks only a bounded lexical candidate union.
  3. CLI text and JSON expose kind, readiness, reason, permissions, source facts, actual retrieval mode, and a safe next action in language a non-programmer can follow.
  4. Missing, corrupt, unsafe, incompatible, or slow external metadata/resources fail closed or degrade to BM25 without triggering network, credentials, installation, or process launch.
  5. Deterministic catalog, retrieval, readiness, prompt-augmentation, and compatibility tests pass without a Provider call or downloaded model.

**Plans:** 3/3 plans complete

Plans:

- [x] 09-01: Create the dedicated workspace, strict capability-card schema, and three typed retrieval indexes
- [x] 09-02: Add inventory-derived readiness, unified CLI search, safe prompt context, and authority guardrails
- [x] 09-03: Add evaluation fixtures, non-programmer documentation, compatibility gates, and full verification

### Phase 10: Rust Authority and Source Boundaries

**Goal:** Users and maintainers have one executable product and writable runtime authority in Rust, while any JavaScript that remains is visibly limited to distribution orchestration.
**Requirements**: RUST-01, RUST-02, RUST-03
**Depends on:** Phase 9
**Success Criteria** (what must be TRUE):

  1. Every supported CLI/TUI, Provider, session, tool, Vault/Wiki, retrieval, capability, migration, and compatibility product path executes the Rust implementation; a legacy product entry cannot start a second implementation.
  2. A repository gate reports the complete JavaScript allowlist and fails when JavaScript imports product-domain source, implements domain behavior, downloads an unverified runtime, or introduces a fallback path.
  3. Runtime commands write only the Rust-owned `.minimax` schemas; no supported or legacy command can create or mutate `.mini-codex` state after the authority cutover.
  4. The Rust CLI and the current npm-installed command remain usable after the authority boundary is enforced, before TypeScript source is deleted.

**Plans:** 2/3 plans executed

Plans:

- [x] 10-01-PLAN.md
- [x] 10-02-PLAN.md
- [ ] 10-03-PLAN.md

**Wave 1**

- [x] 10-01: Inventory product ownership and lock the Rust/JavaScript source allowlists

**Wave 2** *(blocked on Wave 1 completion)*

- [x] 10-02: Disable legacy execution and enforce Rust-only writable state

**Wave 3** *(blocked on Wave 2 completion)*

- [ ] 10-03: Gate the sole-authority product surface and verify both direct and npm entry paths

### Phase 11: Rust Verification and Evaluation Authority

**Goal:** Maintainers can decide parity and release readiness from deterministic Rust tests and evaluations before any TypeScript-covered source is removed.
**Requirements**: RVE-01, RVE-02, RVE-03
**Depends on:** Phase 10
**Success Criteria** (what must be TRUE):

  1. Every still-required public CLI, lifecycle, Provider, tool, retrieval, and rendering behavior formerly covered by TypeScript has a deterministic Rust test, while each intentionally retired behavior has an explicit reviewable decision.
  2. The Rust Provider evaluation runs offline against Responses and Chat Completions fixtures, needs no credentials or API spend, and emits a machine-readable pass/fail report.
  3. The Rust retrieval evaluation reports deterministic exact/BM25 and mixed-language ranking results, proves BM25 runs before embedding, rejects semantic outsiders, and records truthful degraded modes.
  4. Rust verification failures block parity and release decisions even if any transitional Node/package smoke passes.

**Plans:** 4 plans planned

Plans:

- [ ] 11-01: Classify TypeScript coverage and close required Rust behavioral gaps
- [ ] 11-02: Port Provider conformance fixtures and machine-readable evaluation to Rust
- [ ] 11-03: Implement and verify the deterministic Rust retrieval evaluator
- [ ] 11-04: Make Rust Provider/retrieval reports blocking package and CI authority

### Phase 12: Fixture Compatibility and Rust Migration

**Goal:** Existing users can verify compatibility and migrate TypeScript-era durable data through Rust without keeping the TypeScript runtime executable.
**Requirements**: RCMP-01, RCMP-02
**Depends on:** Phase 11
**Success Criteria** (what must be TRUE):

  1. The compatibility harness compares the current Rust product with immutable public-contract fixtures and approved differences without building, importing, or executing TypeScript.
  2. Compatibility reports are deterministic and machine-readable, contain no live `typescript.*` product rows, and fail when fixture intent and Rust behavior disagree.
  3. Rust migration tests cover inventory, dry-run, apply, verify, idempotency, collision handling, interruption recovery, and narrow rollback while leaving all source data unchanged.
  4. Static TypeScript v1 migration fixtures and release metadata make the two-public-release support window observable and prevent accidental early removal.

**Plans:** 2 plans planned

Plans:

- [ ] 12-01: Rebase compatibility reports on immutable contract fixtures and explicit differences
- [ ] 12-02: Harden Rust migration fixtures, recovery gates, and support-window evidence

### Phase 13: Thin npm and Native Release

**Goal:** Users can install through npm or native archives and always run the verified Rust binary through one clear, no-fallback command path.
**Requirements**: RNPM-01, RNPM-02, RNPM-03
**Depends on:** Phase 12
**Success Criteria** (what must be TRUE):

  1. On supported Windows x64 and Linux x64 hosts, both `npm install -g minimax-codex` and `npx minimax-codex` launch the Rust binary packaged for that host through the single `minimax-codex` command.
  2. The packed npm artifact exposes no legacy command or `dist/cli.js` path and contains no TypeScript compiler/runtime, React/Ink runtime, or TypeScript-only production/build dependency.
  3. Package verification rejects missing, wrong-platform, renamed, non-executable, or hash-mismatched binaries with a stable actionable non-zero error and never tries another runtime.
  4. Offline installed-package smoke proves the npm and native release paths use the expected checksummed Rust artifact without a runtime download.

**Plans:** 3 plans planned

Plans:

- [ ] 13-01: Reduce package metadata and the launcher to one Rust-only command path
- [ ] 13-02: Assemble checksummed Windows/Linux native and npm artifacts
- [ ] 13-03: Add offline installed smoke and fail-closed package corruption tests

### Phase 14: TypeScript Removal and Hosted Closure

**Goal:** The repository and release evidence describe one Rust-only product after the replaced TypeScript implementation is deleted.
**Requirements**: RCUT-01, RCUT-02, RCUT-03
**Depends on:** Phase 13
**Success Criteria** (what must be TRUE):

  1. Repository scans find no TypeScript/TSX product or test source, compiler configuration, legacy build path, or executable fallback; CI rejects reintroduction outside immutable migration fixture data.
  2. The Rust workspace, evaluations, compatibility and migration gates, packaging, and installed command remain green after the TypeScript tree and all stale references are removed.
  3. Hosted Windows x64 MSVC and Linux x64 GNU jobs pass tests, evaluations, checksums, install, upgrade/rollback, security, license, and performance gates against one final product fingerprint; stale v2/Phase 9 or local GNU-LLVM evidence cannot satisfy the gate.
  4. User and maintainer documentation explains the Rust-only architecture, npm/native installation, supported platforms, actionable no-fallback failures, migration/rollback, and the two-release compatibility window.

**Plans:** 3 plans planned

Plans:

- [ ] 14-01: Delete replaced TypeScript source, tests, configuration, and legacy references
- [ ] 14-02: Make permanent source, CI, release, and product-fingerprint gates enforce the Rust-only tree
- [ ] 14-03: Finalize Rust-only docs, freeze the final fingerprint/intake, collect hosted Windows/Linux evidence, and close v3.0
