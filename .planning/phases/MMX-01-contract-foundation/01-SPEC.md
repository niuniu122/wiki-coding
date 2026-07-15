# Phase 1: Contract Foundation — Specification

**Created:** 2026-07-15
**Ambiguity score:** 0.07 (gate: <= 0.20)
**Requirements:** 8 locked

## Goal

A pinned Rust 1.97.0 workspace compiles offline after dependencies are cached and proves the protocol, dependency, provider, command, and parity contracts required by all later phases.

## Background

The baseline repository contains TypeScript + Ink only; `cargo`, `rustc`, and `rustup` are not currently installed on this Windows environment. TypeScript at `84784f5` is runnable and already contains provider conformance, capability retrieval evaluation, slash-command routing, and fixed tool-call identity behavior. Phase 1 adds the Rust skeleton beside it without switching the product entry.

## Requirements

1. **Workspace shape (ARCH-01)**
   - Current: No `Cargo.toml`, Rust crate, or pinned Rust toolchain exists.
   - Target: Root Cargo workspace with protocol, core, provider, tools, retrieval, vault, tui, cli, and dev-only compat-harness crates, pinned to Rust 1.97.0.
   - Acceptance: `cargo metadata --locked` lists exactly the intended members and `cargo test --workspace --locked` compiles on the installed Windows toolchain.

2. **One-way dependencies (ARCH-02)**
   - Current: TypeScript responsibilities cross module boundaries and no Rust dependency rule exists.
   - Target: Core depends only on protocol and abstract ports; adapter crates point inward; an automated check rejects forbidden edges.
   - Acceptance: Dependency gate passes on the workspace and a fixture/negative test detects a synthetic forbidden core-to-adapter edge.

3. **Typed operation/event protocol (ARCH-03)**
   - Current: TypeScript runtime events are the only executable contract.
   - Target: Versioned Rust commands/events/errors/tool-call identities and terminal outcomes with unknown-field-safe serialization policy.
   - Acceptance: Round-trip fixtures pass and illegal sequences (early EOF, duplicate terminal, data after terminal) return typed protocol errors.

4. **Determinism (ARCH-04)**
   - Current: Existing TypeScript tests provide partial deterministic clocks/providers but no cross-language harness.
   - Target: Rust ports for clock/ID/mock Provider plus normalized fixture output independent of wall time and random IDs.
   - Acceptance: Two identical replay runs produce byte-identical normalized reports.

5. **Slash-command inventory (COMP-01)**
   - Current: Commands are implemented across TypeScript UI/runtime code without one machine-readable compatibility manifest.
   - Target: A checked-in manifest enumerates every locked public slash command, aliases, arguments, and expected high-level outcome.
   - Acceptance: A test compares the manifest with the locked command list and fails on missing/duplicate names.

6. **Provider protocol fixtures (COMP-02)**
   - Current: Responses and Chat Completions conformance exists only in TypeScript.
   - Target: Language-neutral JSONL fixtures and expected typed events consumed by both TypeScript and Rust harnesses.
   - Acceptance: Valid streams converge; malformed JSON, premature EOF, duplicate terminal, and terminal-after-data cases fail as specified.

7. **Provider profile inventory (COMP-03)**
   - Current: MiniMax official/Hashsight and custom OpenAI-compatible configuration is TypeScript-specific.
   - Target: A provider capability/configuration manifest records built-in profiles, protocols, required fields, and secret references without secret values.
   - Acceptance: Manifest schema validation passes and includes all three profile classes.

8. **Parity reporting (COMP-04)**
   - Current: No single report states which behaviors Rust already matches.
   - Target: Dev-only compat harness produces matched/pending/approved-difference results for commands, providers, and protocol fixtures.
   - Acceptance: Initial report is deterministic, marks unimplemented product behaviors pending rather than failed/matched, and exits nonzero only for regression of implemented mandatory contracts.

## Boundaries

**In scope:**

- Pinned Rust toolchain declaration and root Cargo workspace.
- Minimal protocol/core types and ports needed to express fixtures.
- Machine-readable command/provider manifests and protocol fixtures.
- Dev-only compatibility runner and architecture/dependency checks.
- CI-ready format/lint/test scripts for the Rust workspace.

**Out of scope:**

- Real HTTP provider calls and user conversations — Phase 2.
- TUI rendering and session persistence — Phase 2.
- Executing filesystem/shell tools — Phase 3.
- Creating a real Vault, Wiki, retrieval index, or migration — Phases 4-6.
- Replacing the npm binary entry — only after Phase 6 acceptance.

## Constraints

- Root `package.json` and TypeScript source remain runnable and unmodified except for additive verification scripts when needed.
- Rust edition is 2024 and toolchain is pinned to 1.97.0.
- Phase tests do not call real Providers, download embedding weights, or require credentials.
- Production crates must not depend on `compat-harness`.
- No SQLite or database crate may enter the dependency graph.

## Acceptance Criteria

- [ ] Official Rust 1.97.0 toolchain is available and pinned by `rust-toolchain.toml`.
- [ ] `cargo fmt --all -- --check` passes.
- [ ] `cargo clippy --workspace --all-targets --locked -- -D warnings` passes.
- [ ] `cargo test --workspace --locked` passes.
- [ ] `cargo metadata --locked` confirms all intended workspace members.
- [ ] Protocol valid/invalid fixtures produce the expected typed outcomes.
- [ ] Command/provider manifests validate and contain all locked compatibility entries.
- [ ] Compat report is deterministic and contains no false `matched` result.
- [ ] Dependency and forbidden-dependency checks pass.
- [ ] Existing `npm run check` and `npm test` still pass.

## Edge Coverage

**Coverage:** 8/8 applicable edges resolved; 0 unresolved

| Category | Requirement | Status | Resolution / Reason |
|----------|-------------|--------|---------------------|
| Missing toolchain | ARCH-01 | covered | Pin/install official Rust 1.97.0 before compile verification |
| Offline rerun | ARCH-01 | covered | Lockfile and `--locked` verification after initial dependency fetch |
| Dependency cycle | ARCH-02 | covered | Automated metadata graph check |
| Unknown event field/type | ARCH-03 | covered | Explicit versioning and unknown-event policy fixtures |
| Duplicate terminal | ARCH-03 | covered | Negative protocol sequence fixture |
| Nondeterministic time/IDs | ARCH-04 | covered | Injected clock/ID and byte-identical replay check |
| Command alias collision | COMP-01 | covered | Manifest uniqueness validation |
| Unimplemented parity item | COMP-04 | covered | Explicit `pending`, never false `matched` |

## Prohibitions (must-NOT)

**Coverage:** 6/6 applicable prohibitions resolved; 0 unresolved

| Prohibition | Requirement | Status | Verification / Reason |
|-------------|-------------|--------|-----------------------|
| MUST NOT change the npm product entry | COMP-04 | resolved | package/bin snapshot and npm tests |
| MUST NOT add core-to-adapter dependencies | ARCH-02 | resolved | metadata graph test |
| MUST NOT add SQLite/database dependencies | ARCH-02 | resolved | dependency-name/license scan |
| MUST NOT use live Provider credentials or network in tests | ARCH-04 | resolved | fixture-only harness and CI env |
| MUST NOT mark pending behavior matched | COMP-04 | resolved | report schema invariant |
| MUST NOT place runtime state in the compat harness | ARCH-04 | resolved | production crate dependency check |

## Ambiguity Report

| Dimension | Score | Min | Status | Notes |
|-----------|-------|-----|--------|-------|
| Goal Clarity | 0.97 | 0.75 | met | Exact compile/contract outcome |
| Boundary Clarity | 0.96 | 0.70 | met | No product cutover or real I/O |
| Constraint Clarity | 0.91 | 0.65 | met | Toolchain, offline, dependency rules fixed |
| Acceptance Criteria | 0.92 | 0.70 | met | Command-level checks listed |
| **Ambiguity** | **0.07** | **<= 0.20** | **pass** | Remaining library patch versions resolve in lockfile |

## Interview Log

| Round | Perspective | Question summary | Decision locked |
|-------|-------------|------------------|-----------------|
| 1 | Researcher | Which baseline is authoritative? | TypeScript `84784f5` |
| 2 | Architect | Translate files or establish ports? | New Cargo workspace with one-way ports |
| 3 | Compatibility | What must Phase 1 prove? | Commands, providers, protocol, deterministic parity |
| 4 | Boundary Keeper | When does Rust become default? | Only Phase 6, not this phase |
| 5 | Environment | Which toolchain? | Official Rust 1.97.0, edition 2024 |

---
*Phase: MMX-01-contract-foundation*
*Spec created: 2026-07-15*
*Next step: execute 01-01 through 01-03 plans*
