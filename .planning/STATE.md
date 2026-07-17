---
gsd_state_version: 1.0
milestone: v1.0
milestone_name: Rust rewrite
current_phase: 8
current_phase_name: Codex-style subprocess sandbox hardening
status: in_progress
stopped_at: Local implementation and audits committed; hosted Ubuntu canary and native matrix require fresh push authorization
last_updated: "2026-07-17T10:43:53+08:00"
last_activity: 2026-07-17
last_activity_desc: Completed local sandbox implementation, audit, and release checks; waiting for hosted Linux/native evidence
progress:
  total_phases: 8
  completed_phases: 7
  total_plans: 25
  completed_plans: 23
  percent: 92
---

# Project State

## Project Reference

See: `.planning/PROJECT.md` (updated 2026-07-15)

**Core value:** A non-programmer can safely find the right open-source capability or project and complete work in one recoverable local CLI.

## Current Position

Phase 7 remains complete. Phase 8 reopens the milestone to replace advisory subprocess safety with a real enforced/fail-closed boundary.

- Requirements: 49/52 complete; SBOX-02, SBOX-06, and SBOX-07 await hosted proof
- Cross-phase integrations: 38/38
- End-to-end flows: 7/7
- Plans: 23/25
- Phase 8 plans: 1/3
- Final audit blockers: 0

## Previous Phase 7 Hosted Baseline (invalidated by Phase 8 product changes)

- Local TypeScript suite: 438 passed.
- Rust workspace tests and doc tests: passed.
- Rust formatting and workspace Clippy with warnings denied: passed.
- Compatibility, retrieval, Provider, migration, release-package, and milestone-flow gates: passed offline.
- Hosted CI run `29485975135`: Windows x64 MSVC and Linux x64 GNU passed.
- Hosted tree: `54b780d09d1a461495120b9987869a073eec5ecb`.
- Product fingerprint: `ff805ee8d73168b968e0b5834b2e7582bf9cc598b4cb3f35835c004aec577172` across 402 product files.

## Current Phase 8 Local Evidence

- Rust format and all-target workspace Clippy with warnings denied: passed.
- Core/tools/CLI tests and doc tests: passed.
- TypeScript: 439/439 passed; retrieval and Provider evaluations passed offline.
- Linux x86_64 sandbox/adversarial targets compile cleanly with Clippy warnings denied.
- Development release verification: 3,217,017-byte base archive, 19.289 ms cold-start p95, 4,796,416-byte maximum idle RSS, no Provider calls/credential reads/model downloads.
- Full workspace and milestone-flow gates correctly reject the now-stale Phase 7 hosted fingerprint. A new Ubuntu Bubblewrap/seccomp canary plus Windows MSVC/Linux GNU matrix is required before Phase 8 closes.

## Locked Decisions

- Rust is the default product entry; `minimax-codex-legacy` keeps the explicit TypeScript path for the support window.
- Permissions are exactly `confirm` and process-scoped `full-access`; all hard safety gates remain active.
- The product uses one project-bound Obsidian-compatible Vault and no SQLite.
- Raw terminal sessions finalize before the separate pinned-main-model Wiki workflow.
- Open-source project discovery is BM25-first; optional verified embedding reranks only BM25 candidates.
- No embedding weights are bundled or downloaded automatically.
- Migration is explicit, source-preserving, receipt-bound, verifiable, and narrowly reversible.

## Non-Blocking Follow-up

- GitHub reports that `actions/checkout@v4` and `actions/setup-node@v4` still target a deprecated Node 20 action runtime. The hosted runner forced Node 24 for those actions and all gates passed; upgrade the action versions when the upstream replacements are adopted.
- Installed-package smoke uses a read-only Rust capability command. The complete Provider/Vault/Wiki chain is verified compositionally by Rust integration tests rather than replayed from the extracted package without credentials.

## Deferred Items

| Category | Item | Target |
|----------|------|--------|
| Platform | macOS support | v2 |
| Extensions | MCP/plugins/subagents | v2 |
| Retrieval | Optional separately installed embedding resource distribution | post-v1 |

## Authorization Boundaries Preserved

No package publication, tag, PR, merge, live Provider request, credential read, embedding model download, SQLite use, source deletion, or real user-data migration was performed.

## Accumulated Context

### Roadmap Evolution

- Phase 8 added: Codex-style subprocess sandbox hardening.
- Approval and sandboxing are independent axes: confirm maps to restricted execution, while process-scoped full access explicitly disables the subprocess sandbox.
- Confirm-mode process execution fails closed on platforms or installations without a proven backend; no partial Windows imitation is presented as safe.
- Local Phase 8 code, docs, CI contracts, regressions, and development packaging pass. The exact Linux Bubblewrap/seccomp canary and refreshed Windows/Linux product-fingerprint evidence remain mandatory hosted gates.
