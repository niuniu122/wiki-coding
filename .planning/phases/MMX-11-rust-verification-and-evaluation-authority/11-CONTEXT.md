# Phase 11: Rust Verification and Evaluation Authority - Context

**Gathered:** 2026-07-17
**Status:** Ready for planning

<domain>
## Phase Boundary

Move behavioral and evaluation confidence from TypeScript into Rust without broadening product scope.
</domain>

<spec_lock>
## Requirements (locked via SPEC.md)

**5 requirements are locked.** See `11-SPEC.md`.
</spec_lock>

<decisions>
## Implementation Decisions

### Coverage policy
- **D-11-01:** Port public behavior and safety contracts; do not preserve dormant TS-only internals.
- **D-11-02:** Every removed TS test needs a written disposition, not necessarily a one-to-one Rust test file.

### Evaluation authority
- **D-11-03:** Provider and retrieval evaluations run in Rust, offline, with deterministic machine-readable reports.
- **D-11-04:** BM25 remains authoritative recall and embedding remains candidate-only.
- **D-11-05:** Rust failures block release regardless of transitional package smoke.

### the agent's Discretion
Choose whether evaluators are dedicated binaries or test-support modules, provided they share production parsing/ranking code without creating a second implementation.
</decisions>

<canonical_refs>
## Canonical References

- `.planning/SPEC.md` — v3 verification authority contract.
- `src/eval/capability-retrieval-report.ts` — transitional retrieval evaluation responsibility.
- `src/eval/provider-conformance.ts` — transitional Provider evaluation responsibility.
- `test/run-tests.ts` and `test/support/provider-conformance-suite.ts` — current TS harness roles.
- `fixtures/compat/provider-streams/` — existing Provider contract fixtures.
- `fixtures/compat/retrieval/` — existing retrieval contract fixtures.
- `crates/provider/`, `crates/retrieval/`, and `crates/cli/tests/` — production Rust owners and integration coverage.
</canonical_refs>

<code_context>
## Existing Code Insights

Rust already normalizes both Provider protocols and contains exact/BM25/embedding/RRF components plus deterministic fixtures. The main gap is evaluation ownership and explicit TS-coverage disposition, not a new runtime.
</code_context>

<specifics>
## Specific Ideas

Keep report schemas stable and diffable so Phase 14 can prove deletion did not reduce coverage.
</specifics>

<deferred>
## Deferred Ideas

LLM-as-judge scoring and live Provider smoke remain outside this milestone.
</deferred>

---
*Phase: 11-rust-verification-and-evaluation-authority*
*Context gathered: 2026-07-17*
