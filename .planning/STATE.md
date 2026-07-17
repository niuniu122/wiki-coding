---
gsd_state_version: 1.0
milestone: v3.0
milestone_name: Rust Convergence
current_phase_name: defining requirements
status: executing
last_updated: "2026-07-17T11:23:36.978Z"
last_activity: 2026-07-17
last_activity_desc: Milestone v3.0 started
progress:
  total_phases: 0
  completed_phases: 0
  total_plans: 0
  completed_plans: 0
  percent: 0
---

# Project State

## Project Reference

See: `.planning/PROJECT.md` (updated 2026-07-15)

**Core value:** A non-programmer can safely find the right open-source capability or project and complete work in one recoverable local CLI.

## Current Position

Phase: Not started (defining requirements)
Plan: —
Status: Ready to execute
Last activity: 2026-07-17 — Milestone v3.0 started

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

## Phase 9 Local Evidence

- Dedicated source-only project, Skill, and MCP catalogs and three typed exact/BM25 indexes are implemented.
- Candidate-only embedding, outsider rejection, lexical fallback, readiness precedence, strict JSON, rendering parity, and prompt no-action boundaries are covered by Rust tests.
- Rust candidate workspace tests and doc tests passed; strict workspace Clippy passed with warnings denied.
- TypeScript check, 440 tests, build, 175-case retrieval evaluation, and Provider conformance evaluation passed.
- The previous hosted evidence is intentionally stale after product changes. A manual candidate CI run and subsequent strict push are required before release; no local evidence fixture was forged.
- The current pre-hosted product fingerprint is `f599aa324e135d30db744d86c497d67196d5d170d469aaa03941aed64d0a74f7` across 414 product files.
- A fallback GNU-LLVM release archive was built, but its installed smoke exited `0xC0000135`; it was not represented as Windows MSVC release evidence.

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
- Refresh the hosted release record through the documented manual candidate CI flow before merging or releasing this product change.

## Deferred Items

| Category | Item | Target |
|----------|------|--------|
| Platform | macOS support | v2 |
| Extensions | Explicitly confirmed installer and sandboxed Skill/MCP runtime | post-v2 |
| Retrieval | Optional separately installed embedding resource distribution | post-v2 |

## Authorization Boundaries Preserved

No package publication, tag, PR, merge, live Provider request, credential read, embedding model download, SQLite use, source deletion, or real user-data migration was performed.

## Accumulated Context

### Roadmap Evolution

- Phase 8 added: Codex-style subprocess sandbox hardening.
- Approval and sandboxing are independent axes: confirm maps to restricted execution, while process-scoped full access explicitly disables the subprocess sandbox.
- Confirm-mode process execution fails closed on platforms or installations without a proven backend; no partial Windows imitation is presented as safe.
- Phase 8 code, docs, CI contracts, adversarial canaries, native release artifacts, and refreshed product-fingerprint evidence all pass. The milestone has no remaining mandatory gate.
- Phase 9 separates external capability metadata from executable tools, keeps BM25 authoritative, and exposes readiness without granting action authority.
- Phase 9 local gates pass. Hosted release evidence is a separate pre-release follow-up because this branch changes the deterministic product fingerprint.

## Decisions

- [Phase 9]: External project, Skill, and MCP metadata lives under capabilities/; crates/tools remains the fixed internal adapter set. — Separates discovery metadata from executable authority.
- [Phase 9]: BM25 is authoritative recall; verified embedding may rerank only the bounded lexical candidate union. — Preserves offline usefulness and prevents semantic expansion.
- [Phase 9]: Discovery exposes ready, needs_install, or needs_authorization but never performs the next action. — Makes prerequisites understandable without granting install or execution authority.
