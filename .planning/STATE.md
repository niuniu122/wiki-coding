---
gsd_state_version: 1.0
milestone: v3.1
milestone_name: Full Access Shell
status: planning
last_updated: "2026-07-21T08:37:35.345Z"
last_activity: 2026-07-21
progress:
  total_phases: 0
  completed_phases: 0
  total_plans: 0
  completed_plans: 0
  percent: 0
---

# Project State

## Project Reference

See: `.planning/PROJECT.md` (updated 2026-07-18)

**Core value:** A non-programmer can safely find the right open-source capability or project and complete work in one recoverable local CLI.

## Current Position

Phase: Not started (defining requirements)
Plan: —
Status: Defining requirements
Last activity: 2026-07-21 — Milestone v3.1 started

## Superseded Evidence

- Phase 7 through Phase 13 fingerprints, local candidates, and hosted runs are historical only; their detailed records remain in the corresponding phase summaries and evidence fixtures.
- None of those superseded values is accepted as Phase 14 release authority.

## Final Phase 14 Evidence

- Rust is the sole product implementation; TypeScript/TSX product and test sources are absent and permanent-empty authority gates block reintroduction.
- Final product fingerprint is `513c7565593b3e3088131d2854709be4773f0a81c2445c146f4a5acb597d29b6` across 235 files.
- Candidate run `29638773706` passed Windows job `88065594381` and Linux job `88065594400`.
- Strict run `29639243817` passed Windows job `88066830361` and Linux job `88066830338`.
- Candidate and strict binary, native archive, npm archive, and capability-output hashes are byte-identical per platform; Windows release linking uses `/Brepro`.
- Both hosted paths proved offline execution, zero Provider/credential/model-download activity, and the Linux security canaries.
- Final exact-root closure passed the locked Rust workspace/doc tests, compatibility harness, Rust-contract verification, release verification, and milestone flow.
- Local GNU-LLVM evidence remains `development_only`; it was not represented as hosted Windows MSVC authority.

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
- Publication, tags, PR, merge, and milestone audit/archive remain separate explicitly initiated operations.

## Deferred Items

| Category | Item | Target |
|----------|------|--------|
| Platform | macOS support | v2 |
| Extensions | Explicitly confirmed installer and sandboxed Skill/MCP runtime | post-v2 |
| Retrieval | Optional separately installed embedding resource distribution | post-v2 |

## Authorization Boundaries Preserved

Phase 14-01 performed the planned repository TypeScript/TSX source deletion. No package publication, tag, PR, merge, live Provider request, credential read, embedding model download, SQLite use, or real user-data migration was performed.

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
- [Phase 13]: Keep npm as a dependency-free distribution shell with exact metadata and one Rust launcher. — Exact allowlists make legacy dependencies, scripts, lifecycle hooks, and bins fail closed.
- [Phase 13]: Use stable E_* launcher categories with expected-path or supported-target guidance. — Failures remain actionable while never searching, downloading, or falling back to another runtime.
- [Phase 13]: Keep GNU-LLVM verification development-only and defer hosted fingerprint refresh to Phase 14. — Local artifacts cannot satisfy Windows MSVC and Linux GNU hosted evidence identity.
- [Phase 13]: Select release target and support tier only from the active exact rustc host. — Removing caller-controlled platform labels prevents GNU-LLVM development artifacts from being represented as MSVC hosted evidence.
- [Phase 13]: Emit the strict release manifest beside the two archives. — An external manifest can bind both whole-archive hashes without a self-referential hash cycle.
- [Phase 13]: Classify package contract tests as hash-pinned package-test-only authority. — Fixture and assertion literals remain reviewable without being mistaken for executable production fallback behavior.
- [Phase 13]: Reject all packed-artifact corruption before installation using stable ARTIFACT_* categories. — Package bytes and metadata must fail closed before any command can run or evidence can be emitted.
- [Phase 13]: Require explicit current fingerprint, binary, artifact, and evidence paths for every release command. — A healthy source tree or stale default target output cannot substitute for the packed candidate being verified.
- [Phase 13]: Bind separate native and npm installed identities to one exact Rust binary and capability output. — Both supported distribution paths must independently prove the same product without fallback, network, credentials, or downloads.
- [Phase 13]: Keep Phase 13 CI read-only and defer hosted MSVC/Linux refresh to Phase 14. — Local GNU-LLVM development evidence cannot satisfy hosted release authority.
- [Phase 14]: Transitional TypeScript and legacy fixture authority remains permanently empty after deletion. — Reintroduction must fail closed instead of reopening a migration class.
- [Phase 14]: The 97-source TypeScript responsibility matrix remains immutable sealed historical evidence. — Deleted source provenance remains reviewable while current executable authority stays Rust-only.
- [Phase 14]: Local GNU-LLVM release evidence remains development_only. — Only fresh hosted Windows MSVC and Linux GNU artifacts can close the release record.
- [Phase 14]: Validate package-lock.json as one exact dependency-free Rust distribution object. — TypeScript, React, Ink, lifecycle, and transitive package authority cannot silently re-enter.
- [Phase 14]: Fingerprint v3 hashes current tracked and untracked working-tree bytes while excluding only planning and the hosted evidence record. — Uncommitted product edits cannot reuse an index-snapshot fingerprint.
- [Phase 14]: Classify exact fingerprint/file-count mismatch as stale hosted evidence and keep GNU-LLVM development evidence distinct from hosted MSVC. — Candidate refresh stays local-positive while strict closure fails closed.
- [Phase 14]: Local GNU-LLVM remains development-only; only hosted MSVC and GNU evidence may close RCUT-02. — Prevents a locally available target from being relabeled as supported release authority.
- [Phase 14]: Candidate evidence must be committed as pending before the ordinary strict push run. — Makes run order auditable and prevents strict verification from relying on an unrecorded candidate.
- [Phase 14]: Fingerprint inputs use universal LF checkout policy with one explicit manifest-bound CRLF migration exception. — Keeps Windows and Linux product identity equal without changing immutable historical bytes.
- [Phase 14]: Windows hosted release builds use /Brepro and must match candidate/strict artifact hashes. — Eliminates MSVC linker nondeterminism and turns reproducibility into a release gate.
- [Phase 14]: Remote evidence history advances only through non-force exact-tree commits. — Preserves intentional local/remote history differences while proving byte equality before every ref update.

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
| Phase 13 P01 | 12min | 2 tasks | 9 files |
| Phase 13 P02 | 24min | 2 tasks | 9 files |
| Phase 13 P03 | 49min | 3 tasks | 16 files |
| Phase 14 P01 | 1h | 2 tasks | 209 files |
| Phase 14 P02 | 1h | 2 tasks | 9 files |
| Phase 14 P03 | 5h 11m | 3 tasks | 21 files |

## Session

**Last session:** 2026-07-18T09:56:01.005Z
**Stopped at:** Completed 14-03-PLAN.md
**Resume file:** None
