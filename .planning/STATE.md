---
gsd_state_version: 1.0
milestone: v1.0
milestone_name: Rust rewrite
current_phase: 8
current_phase_name: Codex-style subprocess sandbox hardening
status: complete
stopped_at: Phase 8 and the v1.0 Rust rewrite milestone completed with fresh candidate and strict hosted evidence
last_updated: "2026-07-17T12:10:00+08:00"
last_activity: 2026-07-17
last_activity_desc: Completed Phase 8 after candidate and strict Windows/Linux sandbox, release, and milestone gates passed
progress:
  total_phases: 8
  completed_phases: 8
  total_plans: 25
  completed_plans: 25
  percent: 100
---

# Project State

## Project Reference

See: `.planning/PROJECT.md` (updated 2026-07-15)

**Core value:** A non-programmer can safely find the right open-source capability or project and complete work in one recoverable local CLI.

## Current Position

The v1.0 milestone is complete. Phase 8 replaced advisory subprocess safety with an enforced/fail-closed Linux boundary and truthful unsupported-platform behavior.

- Requirements: 52/52 complete
- Cross-phase integrations: 38/38
- End-to-end flows: 7/7
- Plans: 25/25
- Phase 8 plans: 3/3
- Final audit blockers: 0

## Previous Phase 7 Hosted Baseline (superseded)

- Local TypeScript suite: 438 passed.
- Rust workspace tests and doc tests: passed.
- Rust formatting and workspace Clippy with warnings denied: passed.
- Compatibility, retrieval, Provider, migration, release-package, and milestone-flow gates: passed offline.
- Hosted CI run `29485975135`: Windows x64 MSVC and Linux x64 GNU passed.
- Hosted tree: `54b780d09d1a461495120b9987869a073eec5ecb`.
- Product fingerprint: `ff805ee8d73168b968e0b5834b2e7582bf9cc598b4cb3f35835c004aec577172` across 402 product files.

## Final Phase 8 Evidence

- Local Rust formatting, TypeScript checks, 440 tests, build, and fingerprint comparison passed.
- Hosted candidate run `29553147648` passed on Windows job `87799771241` and Ubuntu job `87799771311` and produced fingerprint `12e41e7384a4474e8e1ed53ccb8942fd7992a6b7b0585a1ab537406b9c74cce4` across 406 product files.
- Hosted strict push run `29553650069` passed on Windows job `87801243529` and Ubuntu job `87801243532` without candidate mode.
- Ubuntu executed the real Bubblewrap namespace preflight and malicious transitive Cargo canary; strict Rust, release, retrieval, Provider, package, and milestone-flow gates passed.
- Windows and Linux archives, binaries, environment facts, and performance measurements are bound to the refreshed machine-readable evidence.

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
- Phase 8 code, docs, CI contracts, adversarial canaries, native release artifacts, and refreshed product-fingerprint evidence all pass. The milestone has no remaining mandatory gate.
