# Phase 10: Rust Authority and Source Boundaries - Context

**Gathered:** 2026-07-17
**Status:** Ready for planning

<domain>
## Phase Boundary

Cut executable and writable authority over to Rust, while leaving transitional TypeScript source inert until later Rust verification phases make deletion safe.
</domain>

<spec_lock>
## Requirements (locked via SPEC.md)

**5 requirements are locked.** See `10-SPEC.md` for requirements, boundaries, acceptance criteria, edges, and prohibitions.
</spec_lock>

<decisions>
## Implementation Decisions

### Product authority
- **D-10-01:** Rust is the only executable product; there is no supported legacy command or fallback.
- **D-10-02:** Current Rust behavior and documented public contracts are authoritative, not dormant TS internals.

### Distribution boundary
- **D-10-03:** npm remains supported, but JavaScript is restricted to locating/launching and packaging the Rust binary.
- **D-10-04:** Transitional TS files may remain temporarily only as non-executable inputs to the verification migration.

### State ownership
- **D-10-05:** `.minimax` is the sole writable state root; `.mini-codex` is source-only migration input.

### the agent's Discretion
Choose the manifest format and scan implementation, provided failures are deterministic and the allowlist is reviewable.
</decisions>

<canonical_refs>
## Canonical References

- `.planning/SPEC.md` — v3 product, npm, compatibility, and cutover contract.
- `.planning/REQUIREMENTS.md` — RUST-01 through RUST-03.
- `package.json` — current dual bin and TypeScript script/dependency surface.
- `bin/minimax-codex.cjs` — current fixed Rust launcher.
- `crates/cli/src/migration.rs` — read-only TypeScript-era migration boundary.
- `crates/vault/src/runtime/mod.rs` — Rust runtime state root.
- `.github/workflows/ci.yml` — current product/release gates.
</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- The current CJS launcher already refuses silent fallback and can become the allowed distribution shell.
- Rust migration has collision, receipt, idempotency, interruption, and rollback tests.

### Integration Points
- Package metadata, Rust CLI headless tests, compatibility architecture checks, and CI source scans.
</code_context>

<specifics>
## Specific Ideas

Use a shrinking transitional-source baseline: Phase 10 prevents new TS authority, Phase 14 drives the allowed count to zero.
</specifics>

<deferred>
## Deferred Ideas

Full source deletion, TypeScript dependency removal, and hosted evidence refresh belong to Phase 14.
</deferred>

---
*Phase: 10-rust-authority-and-source-boundaries*
*Context gathered: 2026-07-17*
