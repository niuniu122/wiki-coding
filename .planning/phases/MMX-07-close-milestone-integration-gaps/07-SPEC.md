# Phase 7: Close Milestone Integration Gaps - Specification

**Created:** 2026-07-16
**Source:** `.planning/v1.0-MILESTONE-AUDIT.md`
**Ambiguity score:** 0.08 (gate: <= 0.20)

## Goal

Turn the six locally complete phases into one end-to-end product by wiring runtime finalization to the Vault/Wiki workflow, restoring automatic non-programmer discovery, making compatibility claims behavioral, and shipping one verified Rust-default-plus-legacy artifact.

## Requirements

1. Runtime session lifecycle deliberately finalizes on one-shot completion, interactive exit/new, and process termination paths; reopening a finalized active session creates a new session rather than mutating immutable evidence.
2. The project binds exactly one Obsidian-compatible Vault, defaulting to the warned sibling recommendation while accepting explicit user selection and stable project identity.
3. Every finalized session receives a durable Wiki receipt. A deterministic local gate may produce no-op; durable work calls the session's pinned main model through a separately visible generation request with separate usage.
4. Production Wiki inputs contain only bounded finalized visible evidence and current pages. Core validates the model's structured KnowledgePatch before the Vault transaction.
5. A bundled, source-backed project catalog makes BM25-first project discovery available without expert paths. Optional embedding remains separately installed, verified, and restricted to BM25 candidates.
6. Natural-language agent requests for open-source tooling receive discovery evidence automatically; read-only discovery never installs or runs a project.
7. Locked slash outcomes have executable tests. Unsupported same-process behavior is an explicit machine-readable difference, not an unqualified matched claim.
8. The official npm artifact contains the launcher, exactly one native platform binary, `dist/cli.js`, package metadata, licenses, release docs, and the explicit legacy bin; an extracted-artifact smoke test runs the Rust default.
9. Final hosted evidence identifies both native jobs and a deterministic product fingerprint that changes when cutover-critical inputs change.

## Acceptance Criteria

- [ ] A real runtime receipt reaches raw immutable Vault evidence and a no-op/pending/synthesized Wiki receipt in an offline end-to-end test.
- [ ] A scripted pinned-main-model response commits a validated Wiki page that normal Wiki search returns; restart is idempotent.
- [ ] No production Wiki workflow can use a model binding different from the finalized session without explicit rebind.
- [ ] Default/bound Vault selection is stable, outside Git by recommendation, visible, and overrideable.
- [ ] Project status/search work with no `--catalog`; BM25 candidate order is captured before any semantic request.
- [ ] An ordinary agent prompt requesting an open-source tool receives bounded discovery context without adding a ninth external tool.
- [ ] Command contract tests cover every canonical command and `/quit`, including product retry and lifecycle behavior or an approved-difference record.
- [ ] The official staged npm tarball includes and runs the native binary and retains `minimax-codex-legacy`.
- [ ] Release inspection remains embedding-free, secret-free, Provider-free, database-free, and below all four budgets.
- [ ] Hosted Windows MSVC and Linux GNU pass the final tree, and the checked fixture matches its product fingerprint.
- [ ] A repeated milestone integration audit reports no blocker and no broken required flow.

## Must Not

- MUST NOT call a second or different model for Wiki synthesis; the finalized session binding is authoritative unless the user explicitly rebinds a pending job.
- MUST NOT put raw/private reasoning, credentials, arbitrary tool output, or unbounded transcripts into the Wiki prompt or pages.
- MUST NOT download or bundle an embedding model, install a discovered project, use SQLite, or perform remote catalog lookup.
- MUST NOT silently continue a session after its evidence is finalized.
- MUST NOT publish, tag, merge, delete TypeScript source, or migrate real user data.

## Verification Strategy

Use deterministic scripted Provider fixtures for the complete runtime/Vault/Wiki/retrieval chain, package extraction and installed-launcher smoke tests for distribution, exhaustive command-contract assertions, product-fingerprint negative tests, the existing full local gates, and final hosted Windows/Linux CI.
