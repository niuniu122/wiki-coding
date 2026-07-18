---
gsd_state_version: 1.0
milestone: v3.0
milestone_name: Rust Convergence
current_phase: 12
current_phase_name: fixture-compatibility-and-rust-migration
status: executing
stopped_at: Completed 12-04-PLAN.md
last_updated: "2026-07-18T01:11:59.123Z"
last_activity: 2026-07-17
last_activity_desc: Phase 12 execution started
progress:
  total_phases: 14
  completed_phases: 12
  total_plans: 48
  completed_plans: 42
  percent: 86
---

# Project State

## Project Reference

See: `.planning/PROJECT.md` (updated 2026-07-15)

**Core value:** A non-programmer can safely find the right open-source capability or project and complete work in one recoverable local CLI.

## Current Position

Phase: 12 (fixture-compatibility-and-rust-migration) — EXECUTING
Plan: 4 of 4
Status: Ready to execute
Last activity: 2026-07-17 — Phase 12 execution started

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

- Rust is the sole supported product entry; `minimax-codex` reaches only the fixed native launcher and no legacy command is exposed.
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
- [Phase 10]: Hash-pin every tracked TS/TSX path as inert transitional evidence until Phase 11 retirement. — Any addition or content edit must become an explicit source-authority review.
- [Phase 10]: Keep the three diagnostic JavaScript fixtures in a separate lifecycle class outside executable authority. — Phase 11 must disposition them and Phase 14 must delete them and zero the class.
- [Phase 10]: Exclude generated dist contents from source authority while validating committed sources and package executable links. — The offline gate must be independent of generated build outputs.
- [Phase 10]: The supported npm surface exposes only minimax-codex through the fixed Rust launcher; TypeScript remains inert source evidence until retirement. — Removes duplicate executable authority without deleting Phase 10 transition evidence.
- [Phase 10]: .minimax is the only project writable runtime root; .mini-codex is read-only migration input and rollback is receipt-scoped. — Keeps migration source-preserving and makes every project mutation attributable to Rust state.
- [Phase 10]: Candidate archives expose only the fixed launcher and one platform Rust binary; generated TypeScript output is not packaged.
- [Phase 10]: Installed identity requires matching direct and launcher versions plus the exact packaged binary SHA-256 under a controlled environment.
- [Phase 10]: CI runs Rust contracts before transitional Node checks and every package or installed smoke step.
- [Phase 10]: Milestone evidence is selected by the actual rustc host so GNU-LLVM development evidence cannot satisfy a hosted tier.
- [Phase 10]: npm development uses the exact locked Cargo CLI route; installed startup remains on the fixed native launcher. — This removes direct TypeScript product execution while preserving argv forwarding and the cross-platform npm shim.
- [Phase 10]: Both offline authority preflights share a structural package-script policy that preserves only transitional test, evaluation, and smoke TypeScript routes. — One fail-closed classifier prevents compatibility and source-authority checks from drifting apart without executing package scripts.
- [Phase 11]: Freeze every Phase 10 verification source by exact path and SHA-256 while allowing multiple responsibility rows per source. — Preserves reviewable history without collapsing distinct public outcomes or requiring one Rust filename per TypeScript file.
- [Phase 11]: Rust-cover documented public and safety behavior, and explicitly retire only dormant, internal, or unshipped TypeScript responsibilities. — Avoids porting undocumented internals merely for file-count parity while making every non-port reviewable.
- [Phase 11]: Treat the responsibility matrix as evidence rather than runtime authority and require every Rust-covered row to name an existing Rust file and exact test function. — Keeps product behavior in Rust while making stale or fabricated coverage bindings fail closed.
- [Phase 11]: Provider evaluation uses fingerprinted immutable fixtures and production Rust replay/config/profile APIs only. — Keeps evaluation deterministic, offline, credential-free, and free of a second Provider implementation.
- [Phase 11]: Both Provider protocols publish the same ordered ten-check contract and verification requires its byte-stable golden. — Makes fixture/check/report drift reviewable and fail-closed.
- [Phase 11]: Transitional package smoke cannot override any failed Rust Provider check. — Preserves D-11-03 and D-11-05 Rust evaluation authority.
- [Phase 11]: The 175-case retrieval corpus moves to immutable compatibility ownership with stable IDs and locked thresholds. — Lets Plan 11-03 consume one fingerprinted corpus while the original remains hash-pinned until 14-01.
- [Phase 11]: The immutable 175-case corpus is the sole lexical evaluation input; its stable IDs, fingerprint, count, and thresholds fail closed in Rust.
- [Phase 11]: BM25 is authoritative candidate recall; deterministic semantic evaluation may rerank only the observed bounded lexical set and outsiders preserve BM25.
- [Phase 11]: Retrieval degradation is fixture-only and model-free; stable reasons preserve BM25 without network, Provider, credentials, downloads, or model loads.
- [Phase 11]: Package evaluation aliases are exact Rust compatibility-harness commands, and verify:agent composes coverage, Provider, then retrieval. — One shared aggregate prevents release-order drift and removes TypeScript evaluator authority.
- [Phase 11]: CI requires coverage, Provider evaluation, and retrieval evaluation before every build, package, or evidence step. — A package smoke success cannot compensate for a failed Rust evaluator report.
- [Phase 11]: Transitional TypeScript tests, static checks, build, and smoke remain allowed, but TypeScript evaluator commands and src/eval paths are denied. — Phase 11 cuts authority without deleting hash-pinned evidence reserved for Phase 14.
- [Phase 11]: Bind every responsibility through one closed semantic evidence contract registry. — A strict row-to-contract bijection preserves per-source auditability while preventing unrelated test-owner reuse.
- [Phase 11]: Keep parser recognition separate from retry and continue outcome authority. — Only executed CLI runtime behavior proves distinct durable requests, turns, terminals, immutable source, and replay.
- [Phase 11]: Validate the complete discovered TypeScript test dependency graph before any test import. — Pre-import closure prevents an earlier test module from producing side effects before a later forbidden evaluator edge is detected.
- [Phase 11]: Keep independent TypeScript AST and Rust lexical graph validators. — Shared fail-closed semantics with separate implementations prevent one parser defect from silently authorizing both gates.
- [Phase 11]: Retain TypeScript evaluator sources as hash-pinned Phase 14 inputs while Rust owns executable evaluation. — Static evidence preserves migration provenance without leaving a Node evaluator route in npm test.
- [Phase 12]: Use immutable contract.* IDs as the fixture-owned public compatibility surface — Separates public product contracts from legacy TypeScript or Rust implementation identities.
- [Phase 12]: Require approved command differences to match explicit fixture IDs, commands, and outcomes — Prevents unknown or loosely matched migration differences from passing verification.
- [Phase 12]: Run candidate compatibility verification through Rust-only fixture consumers — Allows hermetic verification without top-level TypeScript source, tests, dist artifacts, or Node execution.
- [Phase 12]: Exclude migration metadata from its own recursive fingerprint while binding every evidence row. — Prevents self-referential hashes and makes source or policy drift review-visible.
- [Phase 12]: Compute fixture removal eligibility from two distinct ordered public releases after 3.0.0. — Prevents elapsed-time guesses duplicate evidence and premature fixture deletion.
- [Phase 12]: Bind recovery and rollback ownership to exact Rust migration targets. — Forged operation manifests or recomputed receipts must not claim arbitrary project files.
- [Phase 12]: Derive compatibility authority from lib.rs/main.rs and exact recursive source-inventory equality; classify legacy references only when they flow through executable read, include, process, or shell contexts. — This prevents omitted or orphaned Rust modules from escaping the gate without turning inert historical fixture and authority literals into false positives.

## Performance Metrics

| Phase | Plan | Duration | Notes |
|-------|------|----------|-------|
| Phase 10 P01 | 29min | 2 tasks | 5 files |
| Phase 10 P02 | 31min | 2 tasks | 9 files |
| Phase 10 P03 | 31min | 3 tasks | 8 files |
| Phase 10 P04 | 20min | 2 tasks | 8 files |
| Phase 11 P01 | 30min | 2 tasks | 9 files |
| Phase 11 P02 | 45min | 2 tasks | 9 files |
| Phase 11 P03 | 17min | 2 tasks | 9 files |
| Phase 11 P04 | 26min | 1 task | 9 files |
| Phase 11 P05 | 65min | 2 tasks | 7 files |
| Phase 11 P06 | 42min | 2 tasks | 10 files |
| Phase 12 P01 | 22min | 2 tasks | 14 files |
| Phase 12 P02 | 23min | 2 tasks | 8 files |
| Phase 12 P03 | 29min | 2 tasks | 2 files |
| Phase 12 P04 | 2h 46m | 2 tasks | 2 files |

## Session

**Last session:** 2026-07-18T01:11:59.101Z
**Stopped at:** Completed 12-04-PLAN.md
**Resume file:** None
