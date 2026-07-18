# MiniMax Codex v3.0 Rust Convergence — Master Specification

**Created:** 2026-07-17
**Predecessor:** v2.0 Capability Workspace at `8691999`
**Ambiguity score:** 0.12 (gate: <= 0.20)
**Requirements:** 14 locked

## Goal

Make Rust the only executable product, state, compatibility, test, and evaluation authority. Keep npm as a convenient but deliberately thin distribution shell that can only locate and launch a supported Rust binary. Remove the live TypeScript product without losing TypeScript-era user data or weakening Windows/Linux release evidence.

## Current State

- Rust is already the default `minimax-codex` entry, but `minimax-codex-legacy` still executes `dist/cli.js`.
- `src/` contains 100 TypeScript/TSX files (about 12,949 lines) and `test/` contains 91 TypeScript test files (about 12,391 lines).
- TypeScript still owns check/build/test, retrieval evaluation, Provider conformance, and compatibility-baseline duties in CI.
- Release scripts and the compatibility harness hard-code the legacy path and expect a two-implementation package.
- Rust already owns source-preserving migration from TypeScript-era data and keeps deterministic fixtures under `fixtures/compat/migration/typescript-v1/`.
- Supported hosted release targets remain `windows-x86_64-msvc` and `linux-x86_64-gnu`; `windows-x86_64-gnullvm-dev` remains development-only evidence.

## Locked Product Contracts

### 1. One Executable Authority

Rust owns CLI/TUI behavior, Provider normalization, sessions, tools, Vault/Wiki, retrieval, capability discovery, migration, compatibility reports, and user-visible output. No TypeScript or JavaScript module may provide a second implementation or runtime fallback.

The parity baseline is the current Rust product plus documented public commands and locked fixtures. Dormant or unshipped TypeScript-only behavior is not automatically ported.

### 2. Thin npm Distribution

The npm experience remains supported through `npm install -g minimax-codex` and `npx`. JavaScript/CJS/MJS is allowed only for package metadata, platform-binary selection, archive assembly, checksums, and installed-package smoke orchestration.

The launcher must fail closed with a stable, actionable error when the binary is missing, incompatible, or not executable. It must never import `src/`, execute `dist/cli.js`, download an unverified binary at runtime, or silently fall back to another implementation.

### 3. Compatibility Without a Live Legacy Runtime

Compatibility is verified as current Rust behavior against immutable public-contract fixtures, golden records, migration fixtures, and explicitly documented differences. The compatibility harness must not build or execute the TypeScript CLI.

`minimax-codex-legacy`, `dist/cli.js`, `src/`, `test/`, TypeScript compiler configuration, and TypeScript-only dependencies are removed only after the Rust replacement gates pass.

### 4. Source-Preserving Upgrade

The Rust importer remains the only TypeScript-era data migration path. It continues to inventory, dry-run, apply, verify, and narrowly roll back without modifying source data or importing secrets/private reasoning. Static TypeScript v1 migration fixtures remain for at least two public releases after v3.0 cutover.

### 5. Rust-Owned Verification

Rust tests/evaluations become authoritative for:

- public CLI and JSONL behavior;
- Responses and Chat Completions Provider conformance;
- exact/BM25/hybrid retrieval, candidate isolation, and labeled ranking cases;
- release packaging, installed-command smoke, migration, and compatibility reporting;
- the absence of TypeScript business source, dependencies, and runtime fallback.

Node may run package-level smoke scripts, but a passing Node script cannot substitute for a failing Rust test.

### 6. Safe Incremental Cutover

Every phase leaves the Rust CLI buildable and the npm-installed command usable. Responsibilities move first, legacy dependencies are removed second, and source deletion occurs last. Hosted evidence is refreshed only after the final product fingerprint is stable.

## Boundaries

### In Scope

- Remove the executable TypeScript CLI, TypeScript source/tests/evaluations, legacy bin, compiler configuration, and TypeScript-only dependencies.
- Port still-required behavioral and evaluation coverage into Rust.
- Rebase compatibility reports on immutable fixtures and the current Rust/public contract.
- Preserve Rust-owned import of TypeScript-era durable data.
- Keep and simplify npm packaging for Windows x64 and Linux x64 Rust binaries.
- Update CI, release verification, documentation, and source-boundary checks.

### Out of Scope

- Removing npm/Node entirely or building a GUI installer.
- Adding macOS, ARM, new Providers, new tools, or new capability installation/execution features.
- Recreating dormant, internal, or unshipped TypeScript-only behavior.
- Changing the Vault format, permission model, BM25-first contract, embedding authority boundary, or capability-workspace safety model.
- Deleting real user data, publishing packages, pushing branches, or spending Provider/API quota.

## Milestone Acceptance Criteria

- [ ] The repository has no `.ts` or `.tsx` product/test files and no `typescript`, `tsx`, `tsc`, `ink`, or React runtime/build dependency.
- [ ] `package.json` exposes `minimax-codex` only; `minimax-codex-legacy`, `dist/cli.js`, and legacy start/build scripts are absent.
- [ ] Allowed JavaScript is confined to the npm launcher and release/package orchestration and has no imports from product-domain source.
- [ ] `cargo test --workspace --locked`, doc tests, formatting, and Clippy with warnings denied pass.
- [ ] Rust-owned Provider and retrieval evaluations cover the existing deterministic fixture cases and publish machine-readable results.
- [ ] Compatibility reports compare Rust against immutable contract fixtures and contain no live `typescript.*` product rows.
- [ ] TypeScript-era migration inventory/dry-run/apply/verify/rollback tests remain source-preserving and pass.
- [ ] Packed npm artifacts install offline in the release job and the installed `minimax-codex` command launches the packaged Rust binary.
- [ ] Missing or wrong-platform native binaries fail with an actionable non-zero error and never invoke another runtime.
- [ ] Windows x64 MSVC and Linux x64 GNU release, checksum, install, upgrade, rollback, security, license, and performance gates pass.
- [ ] CI contains an explicit source-boundary gate preventing reintroduction of TypeScript business logic or a legacy fallback.
- [ ] Documentation explains npm and native installation, the Rust-only architecture, and the two-release migration-support window.
- [ ] Hosted evidence is regenerated against the final v3 product fingerprint; stale v2 evidence cannot satisfy the gate.

## Edge Coverage

| ID | Edge | Resolution | Coverage |
|----|------|------------|----------|
| EDGE-01 | npm launcher cannot find its native binary | Exit non-zero with platform/path guidance; never fall back | covered |
| EDGE-02 | npm package contains a binary for the wrong platform | Package verification rejects the artifact before publication | covered |
| EDGE-03 | old TypeScript data is malformed, oversized, secret-bearing, or collides | Rust migration fails closed before target mutation and preserves source | covered |
| EDGE-04 | a TypeScript-only test describes undocumented behavior | Classify against the Rust/public contract; port only required behavior and record explicit retirement | covered |
| EDGE-05 | Rust and historical evaluation results disagree | Block deletion/cutover until fixture intent is reconciled; never weaken thresholds silently | covered |
| EDGE-06 | deletion removes a still-referenced legacy path | Repository-wide source/package/CI scan and installed-package smoke fail | covered |
| EDGE-07 | Windows local GNU-LLVM artifact is mistaken for hosted MSVC evidence | Keep development-only tier and require hosted target identity | covered |
| EDGE-08 | migration support window expires | Removal requires a later explicit milestone decision; v3 only records the earliest eligible release | backstop |

## Prohibitions

| Must-NOT statement | Status | Verification |
|--------------------|--------|--------------|
| MUST NOT keep two executable product implementations | resolved | package-bin scan, source-boundary test, installed smoke |
| MUST NOT put Provider, retrieval, session, Vault, tool, or migration behavior in JavaScript | resolved | JS allowlist plus import/content architecture gate |
| MUST NOT let npm fall back to `dist/cli.js` or download an unverified runtime | resolved | launcher negative tests and packed-artifact inspection |
| MUST NOT delete TypeScript source before replacement Rust gates pass | resolved | phase dependencies and cutover preflight |
| MUST NOT modify or delete TypeScript-era source data during migration | resolved | migration collision/idempotency/rollback tests |
| MUST NOT expand the supported platform or product feature scope in this milestone | resolved | roadmap scope and release-target assertions |
| MUST NOT treat local GNU-LLVM smoke as hosted Windows MSVC release evidence | resolved | support-tier and host-target validation |
| MUST NOT push, publish, open a PR, spend API quota, or download model weights without fresh approval | resolved | execution log and git/network review |

## Ambiguity Report

| Dimension | Clarity | Weight | Notes |
|-----------|---------|--------|-------|
| Functional clarity | 0.92 | 35% | Rust authority and npm distribution outcome are explicit |
| Behavioral clarity | 0.84 | 25% | current Rust/public contract is the baseline; dormant TS behavior excluded |
| Boundary clarity | 0.91 | 25% | npm, migration, platforms, feature scope, and deletion order are fixed |
| Quality clarity | 0.82 | 15% | deterministic, package, CI, and hosted gates are defined |
| **Weighted ambiguity** | **0.12** | | **Passes the <= 0.20 specification gate** |

## Interview Log

| Round | Decision locked |
|-------|-----------------|
| Product direction | Rust leads nearly all implementation to reduce drift and bugs |
| Distribution | Keep npm for convenience, but only as a thin Rust binary shell |
| Legacy runtime | Remove `minimax-codex-legacy`; do not keep a second executable implementation |
| Existing users | Preserve source-safe Rust migration for at least two public releases |
| Compatibility | Current Rust CLI and documented public contract are authoritative |
| Platforms | Keep Windows x64 and Linux x64; defer macOS/ARM |
| Delivery | Use a v3.0 milestone and small, independently verified phases |

---
*Next: define v3.0 requirements, roadmap phases, and executable phase plans.*
