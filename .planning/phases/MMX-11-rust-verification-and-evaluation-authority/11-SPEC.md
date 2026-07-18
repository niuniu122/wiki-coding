# Phase 11: Rust Verification and Evaluation Authority - Specification

**Created:** 2026-07-17
**Ambiguity score:** 0.13 (gate: <= 0.20)
**Requirements:** RVE-01, RVE-02, RVE-03

## Goal

Replace TypeScript test and evaluation authority with deterministic Rust coverage before the TypeScript source and test trees are deleted.

## Requirements

1. Produce a reviewable coverage matrix classifying every TS test/evaluation responsibility as already covered in Rust, requiring a Rust port, package-only smoke, or intentionally retired.
2. Add missing public-behavior Rust tests without recreating undocumented TS internals.
3. Implement Rust-owned Provider conformance for Responses and Chat Completions fixtures with strict machine-readable output.
4. Implement Rust-owned retrieval evaluation for exact/BM25, mixed Chinese/English intents, candidate-only embedding, outsider rejection, and degraded modes.
5. CI/release decisions use Rust evaluation reports; transitional TS results are comparison evidence only until removed.

## Boundaries

**In scope:** coverage audit, Rust tests, Provider/retrieval evaluators, fixture ownership, machine-readable reports, CI authority switch.

**Out of scope:** compatibility-report naming, migration support window, npm archive layout, TS source deletion.

## Acceptance Criteria

- [ ] All TS test/eval responsibilities have a disposition and owner.
- [ ] Required public behavior has deterministic Rust coverage or an explicit retirement record.
- [ ] Rust Provider evaluation passes supported offline fixtures and rejects malformed streams.
- [ ] Rust retrieval evaluation covers the existing labeled corpus and proves BM25-before-embedding/candidate isolation.
- [ ] Rust report failure blocks release even if Node/package smoke passes.

## Edge Coverage

| Edge | Resolution | Coverage |
|------|------------|----------|
| TS test asserts undocumented internal behavior | retire explicitly rather than port blindly | covered |
| fixture semantics differ between evaluators | define one fixture schema and fail on unknown fields | covered |
| ranking ties create platform drift | stable IDs and deterministic tie-breaking | covered |
| semantic helper fails or returns outsider | preserve lexical results and report degradation | covered |

## Prohibitions

| Must-NOT statement | Status | Verification |
|--------------------|--------|--------------|
| MUST NOT lower thresholds silently to match Rust | resolved | checked fixture metadata and reviewable changes |
| MUST NOT call real Providers or use credentials | resolved | offline transport and environment-negative tests |
| MUST NOT treat Node smoke as behavioral authority | resolved | CI dependency/order assertions |

## Verification Strategy

Run focused Rust crate tests, golden machine-readable report tests, full workspace tests, and CI command-order/source assertions. Keep all evaluation inputs deterministic and offline.
