# Phase 4: Vault and Main-Model Wiki - Specification

**Created:** 2026-07-16
**Ambiguity score:** 0.024 (gate: <= 0.20)
**Requirements:** 10 locked
**Mode:** Auto-generated from the completed user interview, master specification, and Phase 4 AI-SPEC

## Goal

Turn every completed Rust session into recoverable project-local evidence and, only when locally judged durable, one source-grounded current Wiki view proposed by that session's pinned main model and committed through a Provider-free Vault writer.

## Background

Phase 2 already stores append-synced project-local session JSONL behind a non-blocking writer lease, and Phase 3 durably records tool boundaries. The Rust Vault crate does not yet bind an Obsidian directory, freeze finalized evidence, import a human inbox, transact Markdown, run a Wiki synthesis job, maintain current truth, or clean data safely. This phase extends the existing durability boundary; it does not add SQLite or a second persistence authority.

## Requirements

1. **Project Vault binding**: First-run bootstrap binds exactly one selected Obsidian-compatible Vault to one stable project ID, recommends a sibling path outside Git, warns that project material is locally readable, and fails closed on project mismatch or a competing writer.
   - Current: Runtime session files live under a caller-selected root, but no project Vault manifest or bootstrap contract exists.
   - Target: The fixed Vault tree and schema-v1 manifest are created idempotently; reopening validates the same project ID and schema before mutation.
   - Acceptance: Repeating bootstrap is byte-stable, a second project is rejected without mutation, a second writer is busy, and an in-Git path produces guidance without editing `.gitignore`.

2. **Ownership and truth regions**: `inbox/` is human-owned; `raw/`, `wiki/`, `log.md`, and `.minimax/` are written only through the Vault adapter with explicit expected-state checks.
   - Current: Only the runtime session namespace has concrete Rust ownership rules.
   - Target: The complete fixed tree is present and each mutation is classified by owner and authority.
   - Acceptance: Empty and populated Vaults validate; writes outside the owned region and externally changed Agent-owned targets fail before replacement.

3. **Finalized raw evidence**: Active sessions remain append-recoverable, but a flushed terminal `session.json` and `events.jsonl` become immutable evidence with stable hashes before any knowledge evaluation.
   - Current: Session journals recover final fragments but are not promoted into the full raw evidence contract.
   - Target: Raw envelopes have validated IDs, monotonic sequences, safe payloads, checksums/hashes, immutable terminal metadata, and secret/private-reasoning exclusion.
   - Acceptance: Tail interruption repairs idempotently; middle corruption, future schema, sequence gaps, post-finalize append, secret content, and hash mismatch all fail closed.

4. **Recoverable Wiki transactions**: Every multi-file Wiki change uses a durable prepared manifest with ordered targets, old/expected hashes, staging files, per-file atomic replacement, and idempotent roll-forward.
   - Current: Atomic derived-index publication exists, but no Markdown transaction manifest exists.
   - Target: Crashes at every prepared/apply/receipt boundary converge to the expected target hashes without duplicate log entries or blind rollback.
   - Acceptance: Fault-injection tests across every boundary converge after restart; stale expected hashes preserve the external edit and reject the transaction.

5. **Content-addressed inbox import**: Inbox ingestion derives a stable import ID from bytes, copies and re-reads immutable raw evidence before compilation, preserves origin metadata, and never overwrites the human original before a successful receipt.
   - Current: No inbox importer exists.
   - Target: Text imports may feed synthesis; unsupported binary assets are retained as evidence but cannot create automatic factual claims.
   - Acceptance: Empty, Unicode, repeated, interrupted, binary, and original-removal-failure fixtures produce one import identity, truthful status, and no duplicate compilation.

6. **Durability evaluation and receipt**: Every finalized session has one stable local evaluation job and exactly one durable `no_op`, `pending`, `synthesized`, or `failed` receipt; startup reconstructs the raw-finalized/evaluation-missing window.
   - Current: No post-finalization knowledge evaluation exists.
   - Target: A typed local `DurabilityGate` uses durable signals only and performs zero model calls for ordinary chat, simple lookups, or repeated information.
   - Acceptance: Durable and no-op fixtures classify deterministically; repeating evaluation or recovery does not create a second job or Provider call.

7. **Pinned main-model Wiki workflow**: A separately visible `MainModelWikiWorkflow` uses the source session's immutable Provider/model binding, reports its own status and usage, and accepts only one bounded structured `KnowledgePatch` result.
   - Current: The main agent runtime exists, but no separate Wiki workflow or patch protocol exists.
   - Target: Core coordinates evidence selection, Provider generation, validation, and a knowledge port; Vault has no Provider dependency or call path.
   - Acceptance: Scripted Provider tests prove the pinned identity, distinct usage, no-op zero cost, unavailable-model pending state, explicit rebind requirement, bounded schema repair, and no Vault-to-Provider edge.

8. **Provenance and current truth**: Core rejects a patch unless every durable claim resolves to allowed raw source IDs, every operation stays in the Wiki namespace with expected hashes, and supersession leaves exactly one current truth per topic.
   - Current: No Wiki page/frontmatter or `KnowledgePatch` validator exists.
   - Target: Stable page IDs live in strict frontmatter; filenames remain slugs; superseded pages leave ordinary retrieval while provenance remains auditable.
   - Acceptance: Missing/fabricated sources, duplicate IDs, unsupported operations, oversize text, secret/injection payloads, conflicting current pages, and stale hashes are rejected before any Vault write.

9. **Lint and rebuild**: Maintenance can diagnose schema, ownership, ID, source, frontmatter, current-truth, and incomplete-transaction defects and can rebuild compiled Wiki/derived indexes from raw evidence without deleting raw.
   - Current: `doctor` routes later maintenance commands as unavailable.
   - Target: `vault lint`, `vault repair`, and rebuild paths return stable human and machine diagnostics through CLI-owned rendering.
   - Acceptance: Empty, clean, damaged, and externally edited fixtures produce deterministic issue codes; rebuild is idempotent and raw hashes remain unchanged.

10. **GC, undo, purge, and forget**: Ordinary GC reports only by default, recomputes reachability before apply, protects permanent/referenced/pending/pinned evidence, and uses 7-day trash plus reconfirmed purge; referenced privacy deletion is a separate Wiki-first forget workflow.
   - Current: No Phase 4 retention workflow exists.
   - Target: Weekly/low-disk/manual triggers only produce a classified report until explicit application; GC has no force bypass.
   - Acceptance: At exactly 7 days trash is still undoable until the recorded expiry instant; concurrent state drift invalidates a plan; purge requires a second matching confirmation; forget removes or re-crystallizes affected claims before raw deletion and leaves a non-secret tombstone.

## Boundaries

**In scope:**

- Per-project Vault bootstrap, manifest, fixed directory tree, ownership checks, and writer lease.
- Raw session promotion/finalization, inbox imports, immutable evidence hashes, and recovery metadata.
- Strict Wiki page/frontmatter and `KnowledgePatch` protocol types.
- `DurabilityGate` and the separate pinned `MainModelWikiWorkflow` with scripted Provider evidence.
- Wiki transactions, current-truth/supersession rules, lint, repair, and rebuild.
- GC report/apply, 7-day trash/undo, reconfirmed purge, and separate forget.
- Rust CLI/TUI/headless status and maintenance routing needed to expose these behaviors.

**Out of scope:**

- SQLite or another database - ordinary files are the chosen transparent truth model.
- Phase 5 retrieval implementation - this phase only exposes current/superseded data needed by that index.
- Real Provider spend or a second summarizer model - fixtures prove the pinned main-model workflow.
- Embedding resources or downloads - owned by Phase 5 and separately authorized.
- TypeScript data import, packaging, npm entry cutover, PR, or merge - owned by Phase 6 or separate authorization.
- Automatic raw deletion, destructive migration, cross-project writable knowledge, or an application-layer encryption claim.

## Constraints

- Core depends on ports only; CLI/TUI do not parse Markdown; Vault never invokes Provider; tools do not own Wiki orchestration.
- Raw finalization is durable before evaluation; every model or file boundary is journaled before the next external action.
- One project has one writable Vault and one writer. Ordinary retrieval later sees only `status: current` pages.
- No credential, environment secret, request header, raw private reasoning, or unrestricted tool output may enter the Vault.
- All automated evaluation uses deterministic/scripted Providers and local fixtures.
- Existing npm product entry stays `dist/cli.js` through Phase 5.

## Acceptance Criteria

- [ ] Bootstrap is idempotent, warns about plaintext/Git placement, and rejects project/schema/writer conflicts without mutation.
- [ ] Raw recovery repairs only an interrupted final fragment; finalization is immutable and precedes evaluation.
- [ ] Inbox text/binary/retry/crash fixtures converge to one content-addressed raw import with preserved provenance.
- [ ] Every injected Wiki transaction crash converges by roll-forward to one expected hash set and one receipt.
- [ ] Every terminal session has one durable evaluation outcome; no-op sessions make zero Provider calls.
- [ ] The scripted Wiki job uses exactly the pinned model, exposes separate usage, and leaves unavailable original models pending until explicit rebind.
- [ ] Invalid source, schema, secret, ownership, operation, supersession, or expected-hash input causes zero Wiki target replacements.
- [ ] Lint and rebuild diagnose deterministic issue codes, restore one current truth, and leave all raw hashes unchanged.
- [ ] GC never auto-deletes raw, never plans protected evidence, revalidates before apply, and retains trash for 7 days.
- [ ] Purge requires a second exact confirmation and forget repairs affected Wiki claims before removing referenced evidence.
- [ ] Repeating bootstrap/import/evaluation/transaction/rebuild/GC recovery on identical state is a safe no-op.
- [ ] A second process, simultaneous plan application, or external target edit fails closed without partial replacement.
- [ ] Empty Vault, empty inbox, empty patch, Unicode names/content, equal hashes, and duplicate input identities have deterministic outcomes.
- [ ] File ordering, transaction ordering, lint issue ordering, and report ordering are stable across runs.
- [ ] MUST NOT silently rebind a Vault, edit project Git policy, change the pinned summarizer, or hide Wiki usage.
- [ ] MUST NOT persist credentials/private raw reasoning, manufacture a claim from unsupported binary data, or let Vault call Provider.
- [ ] MUST NOT provide a GC force switch that deletes referenced/pending/pinned/permanent evidence or bypasses Wiki-first forget.

## Edge Coverage

**Coverage:** 43/43 applicable edges resolved - 0 unresolved

| Category | Requirement | Status | Resolution / Reason |
|----------|-------------|--------|---------------------|
| idempotency, concurrency | R1 | covered | Repeat bootstrap and second-project/writer acceptance criteria. |
| adjacency, empty, ordering, idempotency, concurrency | R2 | covered | Empty/populated ownership validation, stable classification, and conflicting-edit criteria. |
| idempotency, concurrency | R3 | covered | Finalization/recovery replay and post-finalize concurrency criteria. |
| adjacency, empty, ordering, idempotency, concurrency | R4 | covered | Empty patch, equal/stale hashes, ordered apply, restart replay, and writer-conflict criteria. |
| empty, encoding, idempotency, concurrency | R5 | covered | Empty/Unicode/byte-identity/retry/removal-race fixtures. |
| adjacency, empty, ordering, idempotency, concurrency | R6 | covered | One stable job/receipt and raw-finalized marker recovery criteria. |
| idempotency, concurrency | R7 | covered | Stable model-bound job, bounded retry, and unavailable-model pending criteria. |
| adjacency, empty, encoding, ordering, idempotency, concurrency | R8 | covered | Empty patch, Unicode/source identity, stable operation order, one-current-truth, and stale-hash criteria. |
| adjacency, empty, ordering, idempotency, concurrency | R9 | covered | Empty/clean/damaged fixtures, stable issue order, repeat rebuild, and external-edit handling. |
| boundary, adjacency, empty, ordering, precision, idempotency, concurrency | R10 | covered | Exact expiry instant, reachability recompute, stable report order, repeat apply, and concurrent drift criteria. |

## Prohibitions (must-NOT)

**Coverage:** 7/7 applicable prohibitions resolved - 0 unresolved

| Prohibition (must-NOT statement) | Requirement | Status | Verification / Reason |
|----------------------------------|-------------|--------|------------------------|
| MUST NOT silently bind a Vault to a different project or edit `.gitignore`/Git policy. | R1 | resolved | test - negative bootstrap fixtures. |
| MUST NOT persist credentials, environment secrets, request headers, or private raw reasoning. | R3, R5, R8 | resolved | test - adversarial secret fixtures scan all durable outputs. |
| MUST NOT invent Wiki facts from an unsupported binary asset or from Wiki context without raw provenance. | R5, R8 | resolved | test - unsupported-source and self-reference fixtures. |
| MUST NOT silently change the session-pinned Provider/model for synthesis. | R7 | resolved | test - binding mismatch remains pending until explicit rebind. |
| MUST NOT hide or merge Wiki synthesis usage into the original session usage. | R7 | resolved | test - distinct typed workflow events and receipt fields. |
| MUST NOT let the Vault crate depend on or invoke a Provider. | R7 | resolved | test - Cargo architecture/source negative gate. |
| MUST NOT expose a GC force path that removes protected evidence or bypasses Wiki-first forget. | R10 | resolved | test - command/schema and reachability negative fixtures. |

Canon security concerns such as path traversal, prompt injection, and general secret scanning remain additionally owned by the existing security/architecture gates; the rows above capture product-specific non-negotiables.

## Ambiguity Report

| Dimension | Score | Min | Status | Notes |
|-----------|-------|-----|--------|-------|
| Goal Clarity | 0.98 | 0.75 | met | Ten mapped VAULT/WIKI outcomes. |
| Boundary Clarity | 0.98 | 0.70 | met | Phase 5/6 and authorization boundaries explicit. |
| Constraint Clarity | 0.97 | 0.65 | met | Ownership, ordering, privacy, model pinning, and no-database rules locked. |
| Acceptance Criteria | 0.97 | 0.70 | met | Positive, failure, crash, repeat, and destructive-negative cases specified. |
| **Ambiguity** | **0.024** | **<= 0.20** | **pass** | Weighted clarity 0.9765. |

## Interview Log

| Round | Perspective | Question summary | Decision locked |
|-------|-------------|------------------|-----------------|
| Prior discussion | Researcher | Database or transparent local knowledge? | Per-project Obsidian-compatible file Vault; no SQLite. |
| Prior discussion | Simplifier | What is the irreducible truth model? | Immutable raw evidence plus one compiled current Wiki view. |
| Prior discussion | Boundary Keeper | Which model summarizes and who writes? | Separate pinned main-model workflow proposes; core validates; Vault writes. |
| Prior discussion | Failure Analyst | When and how can data be cleaned? | Report first, reachability protection, 7-day trash, reconfirmed purge, separate forget. |
| Auto edge probe | Seed Closer | What happens on empty/repeat/concurrent/crash/boundary cases? | All 43 applicable edge candidates became explicit acceptance criteria or fixtures. |

---

*Phase: MMX-04-vault-and-main-model-wiki*
*Spec created: 2026-07-16*
*Next step: Phase 4 implementation planning*

