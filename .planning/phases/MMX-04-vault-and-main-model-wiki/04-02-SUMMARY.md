---
phase: MMX-04-vault-and-main-model-wiki
plan: "02"
subsystem: main-model-wiki
tags: [rust, inbox, knowledge, main-model, provenance, workflow, recovery, obsidian]
requires:
  - phase: MMX-04-vault-and-main-model-wiki
    provides: project-bound Vault, immutable raw evidence, strict pages, and recoverable transactions
provides:
  - exact-byte content-addressed inbox import with human-original protection
  - deterministic local durability gate and source/current-truth patch validator
  - separate pinned-main-model Wiki reducer, ports, events, usage, and receipts
  - durable generation-attempt journal, explicit rebind, and crash-idempotent CLI composition
affects: [vault-maintenance, wiki-retrieval, project-discovery, migration, product-cutover]
tech-stack:
  added: []
  patterns: [local no-op gate, model-bound attempt journal, no-lock-across-network, proposal-validate-commit]
key-files:
  created:
    - crates/vault/src/inbox.rs
    - crates/core/src/knowledge.rs
    - crates/vault/src/workflow.rs
    - crates/cli/src/wiki.rs
    - crates/cli/tests/wiki_workflow.rs
    - fixtures/compat/wiki/main-model-workflow.v1.json
  modified:
    - crates/protocol/src/knowledge.rs
    - crates/protocol/src/vault.rs
    - crates/core/src/ports.rs
    - crates/cli/src/headless.rs
key-decisions:
  - "The local gate, not a second model, decides whether a finalized session merits synthesis; no-op costs zero Provider calls."
  - "Generation started/result, pinned identity, schema repair, explicit rebind, separate usage, and final receipt are durable independent workflow facts."
  - "CLI drops the Vault lease before every model await and reopens/revalidates only for durable records or expected-hash commit."
patterns-established:
  - "Unsupported binary inbox objects remain evidence-only; a changed human original is never removed after compilation."
  - "A model proposes one strict patch, core validates provenance/current truth/secrets, and Vault alone renders and commits pages."
requirements-completed: [VAULT-05, WIKI-01, WIKI-02, WIKI-03]
requirements-progressed: [WIKI-04]
coverage:
  - id: D1
    description: Exact text, Unicode, empty, repeat, changed-original, and binary inbox inputs converge to truthful content-addressed evidence.
    requirement: VAULT-05
    verification:
      - kind: integration
        ref: "crates/vault/tests/inbox_import.rs"
        status: pass
    human_judgment: false
  - id: D2
    description: Durable/no-op classification, bounded schema repair, explicit rebind, state transitions, and source/current validation are pure core behavior.
    requirement: WIKI-01
    verification:
      - kind: unit
        ref: "crates/core/tests/knowledge_workflow.rs"
        status: pass
    human_judgment: false
  - id: D3
    description: Scripted pinned-model synthesis exposes separate usage and resumes generation/commit crashes without double-call or duplicate log writes.
    requirement: WIKI-02
    verification:
      - kind: integration
        ref: "crates/cli/tests/wiki_workflow.rs"
        status: pass
    human_judgment: false
duration: 56min
completed: 2026-07-16
status: complete
---

# Phase 4 Plan 2: Main-Model Wiki Workflow Summary

**The source session's pinned main model can now participate in a separately visible Wiki workflow, but it only proposes: local policy decides whether to call it, core validates the result, and Vault performs the write.**

## Accomplishments

- Imported exact inbox bytes into content-addressed text or evidence-only binary storage, re-hashed after publication, deduplicated repeats, and removed the human original only after a committed knowledge transaction.
- Added a deterministic `DurabilityGate`, strict evaluation jobs, current-page/evidence inventories, source/hash/path/secret/injection/supersession validation, and one-current-truth projection.
- Added the pure `MainModelWikiWorkflow` reducer with no-op, generation, validation, commit, pending, failed, synthesized, bounded one-repair, explicit-rebind, and separate-usage behavior.
- Added durable job/generation/event/receipt storage and recovery of the raw-finalized/evaluation-missing window.
- Added CLI composition and a Vault knowledge port that release the writer lease across model work, persist attempt boundaries, reuse durable results after crashes, and recover existing transactions without a second model call or log entry.

## Task Commits

1. **Content-addressed inbox import** - `4b5d382`
2. **Durability gate, validator, and workflow core** - `a8a8261`
3. **Durable pinned-model synthesis composition** - `f0843d6`

## Decisions and Deviations

- Stored only typed accepted patches or safe rejection codes; malformed/secret model text is never retained as raw untrusted output.
- Keyed generation attempts by both attempt number and model binding, so an explicit rebind cannot reuse an unavailable result from the original model.
- Allowed a pending receipt to transition to a later pending/synthesized/failed current receipt only through a durable explicit rebind record; terminal no-op/synthesized/failed receipts remain immutable.
- No live Provider request, credential access, model download, database, migration, deletion outside temporary fixtures, product-entry change, PR, or merge occurred.

## Verification

- Inbox, core workflow, durable workflow-store, CLI synthesis, rebind, schema-repair, provenance rejection, and two crash-boundary suites passed.
- Rust workspace formatting, workspace Clippy with `-D warnings`, all workspace tests/doc tests, architecture metadata, Rust compatibility verifier, and `git diff --check` passed.
- The npm product entry remains `dist/cli.js`, and every Provider behavior is scripted.

## Next Plan Readiness

- Plan 04-03 can lint these manifests/pages/jobs/transactions, rebuild compiled output from raw, and add conservative report-first GC, trash/undo/purge, and Wiki-first forget.

## Self-Check: PASSED

---
*Phase: MMX-04-vault-and-main-model-wiki*
*Plan: 04-02 completed 2026-07-16*
