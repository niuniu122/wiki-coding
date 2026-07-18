# Phase 10: Rust Authority and Source Boundaries - Specification

**Created:** 2026-07-17
**Ambiguity score:** 0.10 (gate: <= 0.20)
**Requirements:** RUST-01, RUST-02, RUST-03

## Goal

Make Rust the sole executable product and writable runtime authority before deleting the transitional TypeScript source, and encode an enforceable boundary for the small JavaScript distribution layer that remains.

## Requirements

1. Every supported command path starts the Rust product; the package exposes no runnable TypeScript legacy product.
2. A checked-in ownership manifest classifies Rust product code, allowed JavaScript packaging code, immutable compatibility data, and transitional TypeScript source awaiting deletion.
3. A deterministic architecture gate rejects new TypeScript/TSX files, JavaScript product imports/behavior/fallbacks, and unsupported writable state roots.
4. Supported commands write `.minimax` only. `.mini-codex` is accepted solely as read-only migration input.
5. Direct Rust and npm-installed entry smoke tests prove the same binary/product identity.

## Boundaries

**In scope:** source ownership inventory, package-bin cutover, state-authority guard, architecture/source scan, direct/npm identity smoke.

**Out of scope:** deleting the full TypeScript tree, porting evaluations, rewriting compatibility reports, final package minimization, hosted evidence refresh.

## Acceptance Criteria

- [ ] `minimax-codex-legacy` is not a supported or executable package command.
- [ ] The Rust/JavaScript ownership manifest covers every runtime/build entry and fails closed on unknown executable paths.
- [ ] JavaScript cannot import `src/`, implement domain behavior, or invoke a fallback executable.
- [ ] No supported command creates or mutates `.mini-codex`; migration tests treat it as source-only.
- [ ] Direct and npm entry smoke report the same Rust version/product identity.

## Edge Coverage

| Edge | Resolution | Coverage |
|------|------------|----------|
| transitional TS source still exists | permit only files recorded in a shrinking baseline; forbid new files and execution | covered |
| JavaScript spawns another interpreter | reject non-Rust runtime/fallback tokens in the allowlisted launcher boundary | covered |
| migration needs `.mini-codex` | permit read-only input in migration code/tests only | covered |
| package points at stale `dist/cli.js` | package contract and installed smoke fail | covered |

## Prohibitions

| Must-NOT statement | Status | Verification |
|--------------------|--------|--------------|
| MUST NOT delete the transitional TS tree in this phase | resolved | phase file/delete allowlist |
| MUST NOT keep a runnable legacy CLI | resolved | package bin and launcher tests |
| MUST NOT allow JavaScript business behavior or runtime download | resolved | source-boundary gate |
| MUST NOT write `.mini-codex` from supported commands | resolved | state-root tests |

## Verification Strategy

Use Rust architecture tests, package metadata assertions, negative launcher fixtures, repository inventory scans, and direct/installed command smoke. No live Provider calls or package publication.
