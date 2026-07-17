# Phase 14: TypeScript Removal and Hosted Closure - Context

**Gathered:** 2026-07-17
**Status:** Ready for planning

<domain>
## Phase Boundary

Perform the irreversible source cleanup only after Phases 10-13 prove Rust replacements and package cutover, then close against final hosted evidence.
</domain>

<spec_lock>
## Requirements (locked via SPEC.md)

**5 requirements are locked.** See `14-SPEC.md`.
</spec_lock>

<decisions>
## Implementation Decisions

### Cleanup
- **D-14-01:** Remove all TypeScript/TSX product and test source after replacement gates pass.
- **D-14-02:** Keep only thin CJS/MJS packaging/launcher code and static TypeScript-era data fixtures.

### Release closure
- **D-14-03:** Windows x64 MSVC and Linux x64 GNU hosted evidence must match the final tree fingerprint.
- **D-14-04:** Local GNU-LLVM evidence cannot stand in for hosted MSVC.
- **D-14-05:** Push, PR, publication, and hosted trigger actions need fresh user authorization.

### the agent's Discretion
Order mechanical file deletions and documentation edits to keep reviewable commits, provided every deletion follows replacement verification.
</decisions>

<canonical_refs>
## Canonical References

- `.planning/SPEC.md` — final v3 acceptance and prohibitions.
- Phase 10-13 SPEC/CONTEXT/PLAN/SUMMARY/VERIFICATION artifacts — mandatory predecessor evidence.
- `package.json`, `tsconfig.json`, `src/`, and `test/` — deletion targets after gates pass.
- `.github/workflows/ci.yml` — final Rust-only hosted matrix.
- `scripts/release/product-fingerprint.mjs` — final tree binding.
- `fixtures/compat/release/hosted-gates.v1.json` — hosted evidence record.
- `docs/release/` and `README.md` — user/maintainer cutover documentation.
</canonical_refs>

<code_context>
## Existing Code Insights

The current tree has about 25k lines of TS source/tests and four release MJS scripts. The former is deleted; the latter may remain only when it orchestrates Rust artifacts and contains no domain logic.
</code_context>

<specifics>
## Specific Ideas

Use a final source-boundary assertion with zero transitional TS allowance, plus an explicit allowlist for immutable fixture data and thin release JavaScript.
</specifics>

<deferred>
## Deferred Ideas

Removing Node/npm entirely and ending migration support are future milestone decisions.
</deferred>

---
*Phase: 14-typescript-removal-and-hosted-closure*
*Context gathered: 2026-07-17*
