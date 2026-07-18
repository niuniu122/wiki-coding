# Phase 12: Fixture Compatibility and Rust Migration - Specification

**Created:** 2026-07-17
**Ambiguity score:** 0.09 (gate: <= 0.20)
**Requirements:** RCMP-01, RCMP-02

## Goal

Verify compatibility and protect upgrades through Rust and immutable fixtures, without retaining or executing the TypeScript product.

## Requirements

1. Replace live `typescript.*` compatibility-product rows with versioned public-contract fixture identities and explicit approved differences.
2. Compatibility verification must not import, build, or execute `src/` or `dist/cli.js`.
3. Preserve deterministic report/golden behavior and architecture checks.
4. Keep Rust migration inventory/dry-run/apply/verify/idempotency/collision/interruption/rollback coverage source-preserving and secret-free.
5. Record a support-window marker that makes TypeScript v1 fixtures ineligible for removal before two public releases after v3.0.

## Boundaries

**In scope:** compat harness baseline model, report/golden fixtures, migration fixtures/tests, support-window metadata/docs.

**Out of scope:** npm packaging mechanics, TS source deletion, schema expansion, real user migration.

## Acceptance Criteria

- [ ] Compatibility output has no executable TypeScript product dependency or live `typescript.*` rows.
- [ ] Fixture intent, approved differences, and Rust evidence are complete and deterministic.
- [ ] Migration tests prove source immutability, bounded input, secret exclusion, idempotency, collision safety, recovery, verification, and rollback.
- [ ] The support-window rule is machine-checkable and documented.

## Edge Coverage

| Edge | Resolution | Coverage |
|------|------------|----------|
| historical fixture is missing or edited | fingerprint/version validation fails | covered |
| report loses a public contract during renaming | completeness and golden tests fail | covered |
| migration is interrupted after partial target writes | operation manifest recovery owns exact targets | covered |
| future cleanup removes fixtures too early | support-window gate blocks | backstop |

## Prohibitions

| Must-NOT statement | Status | Verification |
|--------------------|--------|--------------|
| MUST NOT execute TypeScript for compatibility | resolved | dependency/process/source scan |
| MUST NOT alter or delete migration source data | resolved | hash and rollback tests |
| MUST NOT import secrets/private reasoning/derived caches | resolved | hostile fixtures and allowlist tests |

## Verification Strategy

Run compat-harness golden/completeness/architecture tests and Rust migration integration tests, followed by workspace verification. No real home directory or user data.
