# Phase 13: Thin npm and Native Release - Specification

**Created:** 2026-07-17
**Ambiguity score:** 0.11 (gate: <= 0.20)
**Requirements:** RNPM-01, RNPM-02, RNPM-03

## Goal

Preserve convenient npm installation and native archives while guaranteeing that every installed command launches one verified Rust binary and fails closed when that binary is invalid.

## Requirements

1. Expose only the `minimax-codex` command for global npm install and `npx`.
2. Package only the supported host Rust binary, launcher, release docs, licenses, and necessary metadata; omit `dist/` and TypeScript dependencies.
3. Verify platform identity, filename, executable mode, checksum, archive manifest, and product fingerprint before release.
4. Installed npm/native smoke runs offline and proves the packaged binary identity.
5. Missing, corrupted, renamed, wrong-platform, or non-executable binaries produce stable non-zero actionable failures with no fallback/download.

## Boundaries

**In scope:** package.json/bin/files, thin launcher, release archive scripts, manifests/checksums, npm/native installed smoke.

**Out of scope:** npm registry publication, runtime download installer, macOS/ARM packages, GUI installer, product behavior changes.

## Acceptance Criteria

- [ ] Packed npm metadata exposes one command and no legacy/dist/TypeScript/React/Ink runtime.
- [ ] Windows x64 MSVC and Linux x64 GNU artifacts have strict target identity and checksums.
- [ ] Offline installed smoke starts the expected Rust binary through npm and native paths.
- [ ] Negative package corruption/platform tests fail before release without another runtime.

## Edge Coverage

| Edge | Resolution | Coverage |
|------|------------|----------|
| binary absent after npm extraction | actionable launcher error, no fallback | covered |
| archive says Linux but contains Windows filename | manifest/host verification rejects | covered |
| executable bit lost on Linux | package verifier rejects before smoke | covered |
| checksum or fingerprint drift | release evidence cannot be emitted | covered |

## Prohibitions

| Must-NOT statement | Status | Verification |
|--------------------|--------|--------------|
| MUST NOT publish or download at runtime | resolved | offline tests and network/process scan |
| MUST NOT include `dist/cli.js` or legacy bin | resolved | tarball manifest assertions |
| MUST NOT treat GNU-LLVM dev evidence as hosted MSVC | resolved | support-tier/host assertions |

## Verification Strategy

Build/package only local candidates, inspect tar/archive entries, run extracted-command smoke, exercise negative fixtures, and validate release evidence schemas. Publication and hosted refresh remain separate authorization gates.
