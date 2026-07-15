# Ingested Decisions

All decisions below are user-confirmed and locked for the v1 rewrite.

- Rust replaces the main product path; TypeScript remains an executable parity reference until cutover.
- Storage is an Obsidian-compatible, per-project file Vault; SQLite and application-layer Vault encryption are excluded.
- `raw/` is immutable evidence, `wiki/` is current compiled knowledge, and only the Vault adapter writes Agent-owned files.
- Public permission modes are exactly `confirm` and `full-access`; full access is session-scoped and cannot bypass hard safety gates.
- The non-programmer project finder remains a first-class workflow: BM25 performs keyword/candidate recall, then embedding matches and reranks projects.
- The current pinned main model participates directly in a separate `MainModelWikiWorkflow`; core validates its structured patch before Vault commit.
- Rust v1 includes read/list, patch/write, bounded shell, Git status/diff, and npm diagnostics; MCP, plugins, and subagents are deferred.
- Windows and Linux ship first; macOS follows only after its platform matrix passes.
- source: docs/superpowers/specs/2026-07-15-rust-vault-rewrite-design.md
