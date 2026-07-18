# Phase 12: Fixture Compatibility and Rust Migration - Context

**Gathered:** 2026-07-17
**Status:** Ready for planning

<domain>
## Phase Boundary

Decouple compatibility from a live legacy runtime while retaining safe upgrade evidence.
</domain>

<spec_lock>
## Requirements (locked via SPEC.md)

**5 requirements are locked.** See `12-SPEC.md`.
</spec_lock>

<decisions>
## Implementation Decisions

### Legacy support
- **D-12-01:** Remove the executable legacy product; keep Rust-owned TypeScript-era data migration.
- **D-12-02:** Retain static migration fixtures for at least two public releases after v3.0.

### Compatibility authority
- **D-12-03:** Compare current Rust behavior with immutable public-contract fixtures and explicit differences, not a running TS baseline.
- **D-12-04:** Source data is never deleted or mutated automatically.

### the agent's Discretion
Choose fixture naming/version structure as long as report completeness and historical provenance remain explicit.
</decisions>

<canonical_refs>
## Canonical References

- `.planning/SPEC.md` — compatibility and migration product contract.
- `crates/compat-harness/src/baseline.rs` — current live TypeScript baseline model.
- `crates/compat-harness/src/report.rs` — deterministic report composition.
- `fixtures/compat/baseline-status.v1.json` and `fixtures/compat/report.expected.json` — current golden inputs/output.
- `crates/cli/src/migration.rs` and `crates/cli/tests/migration.rs` — Rust migration implementation and safety suite.
- `fixtures/compat/migration/typescript-v1/` — immutable TypeScript-era migration evidence.
</canonical_refs>

<code_context>
## Existing Code Insights

Migration is already Rust-native and extensively fail-closed. The compat harness is also Rust, but its identity model and release assertions still assume an executable TypeScript baseline.
</code_context>

<specifics>
## Specific Ideas

Treat historical TypeScript as data provenance, never as runtime authority.
</specifics>

<deferred>
## Deferred Ideas

Removing migration support after the window requires a future explicit milestone.
</deferred>

---
*Phase: 12-fixture-compatibility-and-rust-migration*
*Context gathered: 2026-07-17*
