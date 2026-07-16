# Phase 4: Vault and Main-Model Wiki - Context

**Gathered:** 2026-07-16
**Status:** Ready for planning
**Mode:** Auto-resolved from the user's completed architecture discussion and locked Phase 4 SPEC/AI-SPEC

<domain>
## Phase Boundary

Extend the existing Rust session journal into one per-project Obsidian-compatible Vault with immutable raw evidence, recoverable Wiki transactions, a separately visible pinned-main-model synthesis workflow, and safe maintenance/retention commands. This phase establishes Wiki truth and provenance but leaves its search engine to Phase 5 and product cutover to Phase 6.

</domain>

<spec_lock>
## Requirements (locked via 04-SPEC.md)

Ten requirements are locked: project binding, ownership, raw finalization, Wiki transactions, inbox import, durability receipts, pinned main-model synthesis, provenance/current truth, lint/rebuild, and GC/forget.

Downstream work MUST read `04-SPEC.md` and `04-AI-SPEC.md`. It may choose internal helpers, but it may not weaken their positive, crash-recovery, edge, or must-NOT acceptance criteria.

</spec_lock>

<decisions>
## Implementation Decisions

### Vault layout and binding

- **D-401:** Use the fixed root tree `AGENTS.md`, `inbox/`, `raw/{sessions,imports,assets}`, `wiki/{sessions,projects,decisions,concepts,providers,lessons}`, `wiki/index.md`, `log.md`, and `.minimax/{manifest,locks,pending,transactions,recovery,indexes,trash}`. Do not create a third session truth area.
- **D-402:** `.minimax/manifest.json` is the strict machine contract with schema version, stable project ID, canonical project fingerprint, and creation metadata. `AGENTS.md` is human guidance and never parsed as schema.
- **D-403:** Bootstrap recommends a sibling Vault path, permits an explicit path, warns about readable local files and in-Git risk, and never edits `.gitignore` automatically.
- **D-404:** Reuse the existing `fs4` non-blocking OS lease pattern for one writer. Obsidian and ordinary editors may read concurrently; a second MiniMax writer fails busy.

### Raw evidence and inbox

- **D-405:** Preserve the Phase 2 append-sync runtime journal and promote finalized sessions into `raw/sessions/<session-id>/session.json` plus `events.jsonl` without a second event body copy. Terminal metadata records stable hashes and makes later appends illegal.
- **D-406:** Recovery quarantines only an incomplete final fragment in `.minimax/recovery/`; middle corruption, future schema, missing sequence, or checksum mismatch stops maintenance and reports repair-required.
- **D-407:** Inbox identity is SHA-256 over exact bytes. Copy to a sibling temp, sync, rename into `raw/imports/` or `raw/assets/`, re-read the hash, then create a stable receipt. The human original is removed only after the knowledge transaction; failure leaves it visibly `imported_source_retained`.
- **D-408:** Binary assets may be immutable evidence but are never converted into a claim without an explicitly supported extractor and source link; no extractor is added in this phase.

### Wiki protocol and transaction

- **D-409:** Define strict schema-v1 `KnowledgePatch`, `KnowledgeOperation`, page/frontmatter, evaluation job, workflow event, separate usage, and receipt types in `minimax-protocol`; reject unknown fields and bound IDs/strings/collections manually with the existing Serde pattern.
- **D-410:** Keep production dependencies small: use Serde plus deterministic in-tree semantic validators and fixture-tested Markdown/frontmatter rendering. Do not add a general RAG framework, database, or Markdown database abstraction.
- **D-411:** Stable page ID lives in frontmatter; filenames are normalized slugs only. A page is `current` or a lightweight `superseded` tombstone; exactly one current page per topic is enforced before commit.
- **D-412:** A transaction directory contains staged target bytes and a strict manifest of path, old hash/absence, expected hash, order, and state. Apply knowledge pages, then `wiki/index.md`, then `log.md`; each already-matching hash is skipped during roll-forward.
- **D-413:** External edits to Agent-owned pages are detected through expected hashes and preserved by failing closed. Recovery rolls forward only a previously prepared validated transaction; it never performs a blind multi-file rollback.

### Main-model synthesis workflow

- **D-414:** Local `DurabilityGate` examines typed outcomes only: new decisions/constraints/preferences/architecture, durable code/config/API/data behavior changes, diagnosed cause/fix/lesson, or explicit todo/risk. Ordinary chat, simple lookup, repeats, and inconclusive failure write no-op receipts with zero Provider work.
- **D-415:** `minimax-core` owns a synchronous effect/state reducer for `evaluation_pending -> no_op | synthesis_pending -> generating -> validating -> committing -> synthesized`, plus typed pending/failure outcomes. CLI async composition executes Provider and knowledge ports without holding the Vault lease across network I/O.
- **D-416:** Each job binds the source session's Provider/model/protocol/settings and uses at most one normal scripted generation plus one schema-only repair attempt. A binding mismatch remains pending until an explicit rebind operation; it never silently picks another model.
- **D-417:** The separate workflow emits typed status/model/usage events and a receipt that cannot be merged into ordinary session usage. The model sees bounded source-labeled raw evidence and relevant current Wiki excerpts as untrusted data, has no tool access, and returns only `KnowledgePatch`.
- **D-418:** Core validates parse/schema, sources, paths, ownership, operation count/type, bytes, secrets, current truth, supersession, and expected hashes before calling `KnowledgePort`; semantic/secret failures do not broaden context or auto-repair.

### Maintenance, GC, and forget

- **D-419:** `vault lint` is read-only and returns stable issue codes/order for manifest, ownership, page ID/frontmatter, source reachability, one-current-truth, pending jobs, and incomplete transactions. `repair` performs only deterministic journal/transaction recovery.
- **D-420:** `rebuild` replaces compiled Wiki/index outputs from raw in a recoverable transaction while leaving raw bytes and evidence hashes unchanged. It is explicit and does not run automatically on ordinary startup.
- **D-421:** GC follows reference first, age second, size last. Normal scans classify permanent/referenced/rebuildable/collectable and never delete. Apply recomputes reachability from current/supersession sources, pending jobs, migration receipts, and pins before moving eligible objects to `.minimax/trash/<gc-id>/`.
- **D-422:** Raw user input/final answer/commands/key tool facts/migration receipts, referenced/supersession evidence, pending/recovery/transaction data, and pins are never ordinary GC candidates. Only derived caches, completed staging, exact duplicate attachments, and unreferenced large transient attachments older than 30 days may be candidates.
- **D-423:** Trash is undoable until its exact recorded 7-day expiry. Purge is a distinct action requiring a second exact plan-bound confirmation. `gc` has no force switch.
- **D-424:** `vault forget` is independent: inventory affected claims, remove or re-crystallize them, commit Wiki changes, delete the sensitive raw object, then write a non-secret tombstone. It never routes through ordinary GC authority.

### the agent's Discretion

- Exact private Rust module names and helper types inside protocol/core/vault/cli/tui.
- Conservative numeric limits for patch operations, source count, page bytes, evidence bytes, and GC report size, provided they are finite, documented in tests, and do not change product behavior.
- Exact plain-text/JSONL wording for maintenance status, provided typed codes, workflow identity, pinned model, and separate usage remain visible.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets

- `crates/vault/src/runtime/{journal,lease,recovery,index}.rs` already supplies append-sync JSONL, non-blocking lease, final-fragment recovery, and atomic content-addressed publication.
- `crates/core/src/{runtime,session,tool}.rs` demonstrates synchronous reducers whose effects persist before publication/external work.
- `crates/protocol/src/{runtime,session,tool}.rs` supplies strict schema-v1 Serde records, validated IDs, model binding inputs, usage, and deny-unknown-fields behavior.
- `crates/cli/src/driver.rs` is the existing async composition root with scripted Provider ports and durable-before-next-step ordering.
- `crates/compat-harness/src/architecture.rs` can mechanically forbid Vault-to-Provider and database dependencies.

### Established Patterns

- Core imports no filesystem, HTTP, terminal, Provider adapter, or concrete Vault code.
- Restart converges by appending typed facts or rolling forward atomically published expected bytes; it never guesses a side effect succeeded.
- Default acceptance uses fixtures and loopback/scripted providers, never a real credential or paid request.
- Headless JSONL and interactive text render the same typed events; rendering stays outside core.

### Integration Points

- Extend `minimax-protocol` with knowledge/vault maintenance records and export them from `lib.rs`.
- Extend `minimax-core` with durability/patch validation/workflow reducers and knowledge/provider-facing ports.
- Extend `minimax-vault` from runtime storage into bootstrap, raw/import, page, transaction, maintenance, and retention modules.
- Compose scripted Wiki generation and Vault commits in `minimax-cli`; expose maintenance subcommands without letting CLI parse Markdown.
- Add compatibility/architecture evidence only after Rust behavior is executable; keep npm product entry unchanged.

</code_context>

<specifics>
## Specific Ideas

- The primary end-to-end fixture is: finalize raw session -> durable evaluation -> scripted pinned main-model patch -> core validation -> crash-injected Vault transaction -> one current Wiki page and separate usage receipt.
- The no-value fixture proves a terminal session writes a no-op receipt and invokes zero Provider operations.
- The cleanup proof previews a mixed graph, rejects referenced/pending/pinned objects, trashes one eligible attachment, undoes it, then reconfirms a separate purge; a referenced privacy request succeeds only through Wiki-first forget.

</specifics>

<deferred>
## Deferred Ideas

- Exact/BM25/embedding Wiki search and project discovery - Phase 5.
- TypeScript data migration, artifact packaging, default entry cutover, and support-window policy - Phase 6.
- Cross-project writable knowledge, global personal knowledge, application-layer encryption, model-provided tools, MCP/plugins/subagents, and background daemon - v2 or later.

</deferred>

---

*Phase: MMX-04-vault-and-main-model-wiki*
*Context gathered: 2026-07-16 from completed discussion and auto spec/edge probes*

