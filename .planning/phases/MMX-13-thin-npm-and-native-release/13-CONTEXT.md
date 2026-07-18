# Phase 13: Thin npm and Native Release - Context

**Gathered:** 2026-07-17
**Status:** Ready for planning

<domain>
## Phase Boundary

Keep npm convenient while reducing it to a distribution shell for supported native Rust artifacts.
</domain>

<spec_lock>
## Requirements (locked via SPEC.md)

**5 requirements are locked.** See `13-SPEC.md`.
</spec_lock>

<decisions>
## Implementation Decisions

### Installation
- **D-13-01:** Preserve npm global install and `npx` for developer/agent users.
- **D-13-02:** Also preserve native Windows/Linux archive installation.

### Launcher behavior
- **D-13-03:** The JavaScript launcher only locates and starts the packaged Rust binary.
- **D-13-04:** Missing/wrong/corrupt binary errors are explicit and never trigger fallback or runtime download.

### Platforms
- **D-13-05:** Support Windows x64 MSVC and Linux x64 GNU only in v3; GNU-LLVM remains development-only.

### the agent's Discretion
Choose whether native binaries are bundled directly or represented as platform packages, provided offline packaging and exact-host verification pass.
</decisions>

<canonical_refs>
## Canonical References

- `.planning/SPEC.md` — thin npm and supported-platform contract.
- `package.json` and `bin/minimax-codex.cjs` — current npm surface.
- `scripts/release/package-rust.mjs` — current archive/npm assembly.
- `scripts/release/verify-rust-release.mjs` — current installed smoke and evidence checks.
- `scripts/release/verify-milestone-flow.mjs` — current release composition gate.
- `fixtures/compat/release/thresholds.v1.json` and `fixtures/compat/release/hosted-gates.v1.json` — fixed release thresholds/evidence schema.
</canonical_refs>

<code_context>
## Existing Code Insights

The launcher already selects a sibling Rust binary and does not silently fall back. Release scripts already validate host triples, archives, checksums, installed smoke, and support tiers, but still package and require `dist/cli.js`.
</code_context>

<specifics>
## Specific Ideas

npm convenience is retained without maintaining Node as a second application runtime.
</specifics>

<deferred>
## Deferred Ideas

GUI installers, package-manager-specific installers, macOS, ARM, and runtime binary download.
</deferred>

---
*Phase: 13-thin-npm-and-native-release*
*Context gathered: 2026-07-17*
