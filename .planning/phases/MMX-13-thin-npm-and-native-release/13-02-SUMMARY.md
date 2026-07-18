---
phase: 13-thin-npm-and-native-release
plan: "02"
subsystem: distribution
tags: [rust, npm, tar, gzip, sha256, reproducible-builds, release-manifest]

requires:
  - phase: 13-01-thin-npm-metadata-and-launcher
    provides: Dependency-free npm metadata and one fixed fail-closed Rust launcher
provides:
  - Strict target contract separating two hosted identities from GNU-LLVM development evidence
  - Schema-validated release manifest binding product, source, entry, archive, and npm hashes
  - Byte-identical native and npm candidate assembly with independent extraction verification
affects: [13-03-offline-corruption-guards, 14-typescript-removal-and-hosted-closure, release, packaging]

tech-stack:
  added: []
  patterns:
    - External release manifests avoid self-referential archive hashes while binding every contained entry
    - Target identity is selected from the active rustc host; callers cannot relabel GNU-LLVM as MSVC

key-files:
  created:
    - fixtures/compat/release/targets.v1.json
    - scripts/release/package-contract.mjs
    - scripts/release/package-contract.test.mjs
  modified:
    - scripts/release/package-rust.mjs
    - scripts/release/verify-rust-release.mjs
    - fixtures/compat/release/thresholds.v1.json
    - fixtures/compat/source-authority.v1.json
    - crates/compat-harness/src/source_authority.rs
    - crates/compat-harness/tests/source_authority.rs

key-decisions:
  - "Select release target and support tier only from the exact active rustc host; remove the caller-controlled platform override."
  - "Emit the strict release manifest beside the artifacts so it can bind both complete archive hashes without self-reference."
  - "Keep package-contract tests hash-pinned but outside executable production scanning so assertion literals cannot create fallback/download false positives."

patterns-established:
  - "Canonical artifact set: one native archive, one npm archive, exact SHA-256 sidecars, and one external schema-v2 manifest."
  - "Independent verification: parse gzip/tar bytes, validate checksums/types/modes/paths/content hashes, then run installed identity smoke."

requirements-completed: [RNPM-01, RNPM-03]

coverage:
  - id: D1
    description: Exactly two hosted targets and one GNU-LLVM development target are schema locked, and malformed, duplicate, unsafe, tier-confused, hash-drifted, platform-drifted, or embedding-bearing manifests fail by category.
    requirement: RNPM-03
    verification:
      - kind: unit
        ref: scripts/release/package-contract.test.mjs#healthy target and release manifest controls pass
        status: pass
      - kind: unit
        ref: scripts/release/package-contract.test.mjs#target contract rejects malformed and tier-confused identities by category
        status: pass
      - kind: unit
        ref: scripts/release/package-contract.test.mjs#release manifest rejects schema path duplicate tier hash platform and embedding drift
        status: pass
    human_judgment: false
  - id: D2
    description: Identical inputs produce byte-identical native/npm archives, sidecars, and strict manifests whose canonical entries share exact launcher, binary, docs, and license bytes.
    requirement: RNPM-03
    verification:
      - kind: integration
        ref: scripts/release/package-contract.test.mjs#package assembly is byte-identical and emits one strict external manifest
        status: pass
      - kind: other
        ref: two real GNU-LLVM package:rust runs with five SHA-256-identical outputs
        status: pass
    human_judgment: false
  - id: D3
    description: The current candidate target, source binary, npm launcher, archive entries, checksums, product fingerprint, installed identity, capability smoke, licenses, security, and budgets are independently verified offline.
    requirement: RNPM-01
    verification:
      - kind: integration
        ref: npm run verify:rust-release -- --binary target/phase13-02-gnullvm/release/minimax-cli.exe --artifacts target/phase13-02-candidate-a
        status: pass
      - kind: integration
        ref: crates/cli/tests/product_identity.rs
        status: pass
      - kind: integration
        ref: cargo run -p minimax-compat-harness --locked -- verify-candidate
        status: pass
    human_judgment: false

duration: 24min
completed: 2026-07-18
status: complete
---

# Phase 13 Plan 02: Checksummed Native and npm Artifacts Summary

**Native and npm candidates are now byte-reproducible views of one Rust binary, governed by an exact target contract and independently verified schema-v2 manifest.**

## Performance

- **Duration:** 24 min
- **Started:** 2026-07-18T09:36:44+08:00
- **Completed:** 2026-07-18T10:00:34+08:00
- **Tasks:** 2
- **Files modified:** 9

## Accomplishments

- Defined exact Linux GNU, Windows MSVC, and Windows GNU-LLVM development target identities with locked host, OS/arch, filename, mode, suffix, and support tier.
- Added a strict manifest schema and table-driven negative matrix for unknown fields, unsafe names, duplicates, tier confusion, malformed hashes, unsupported identities, canonical entries, and embedding exclusion.
- Rebuilt native/npm assembly around canonical deterministic tar/gzip entries and an external manifest that binds every source/contained/archive hash, size, mode, target, and product fingerprint.
- Independently parsed and verified the actual archives before installed npm identity/capability smoke, producing five byte-identical files across two real local package runs.

## Task Commits

Each TDD task was committed atomically:

1. **Task 1 RED: Lock target and release-manifest contract** - `9d37bcc` (test)
2. **Task 1 GREEN: Define strict release artifact contract** - `6e64e84` (feat)
3. **Task 2 RED: Require deterministic strict release candidates** - `d1aaa12` (test)
4. **Task 2 GREEN: Assemble deterministic native and npm candidates** - `93506a6` (feat)

## Files Created/Modified

- `fixtures/compat/release/targets.v1.json` - Exact hosted and development target identities.
- `scripts/release/package-contract.mjs` - Target, manifest, entry-set, hash, and cross-channel validation helpers.
- `scripts/release/package-contract.test.mjs` - Healthy control, negative matrix, threshold lock, and two-run packaging integration test.
- `scripts/release/package-rust.mjs` - Rustc-host-selected deterministic native/npm candidate assembly.
- `scripts/release/verify-rust-release.mjs` - Independent manifest, sidecar, gzip/tar, entry, hash, identity, license, security, and performance verification.
- `fixtures/compat/release/thresholds.v1.json` - Target-contract schema link with unchanged numeric budgets.
- `fixtures/compat/source-authority.v1.json` - Hash-pinned package contract, test, packager, and verifier authority.
- `crates/compat-harness/src/source_authority.rs` - Distribution-orchestration and package-test-only authority classes.
- `crates/compat-harness/tests/source_authority.rs` - Classification, assertion-literal, and executable-download negative coverage.

## Decisions Made

- A caller can choose binary/output/version paths but cannot choose a platform label; the active exact rustc host selects the only valid target and tier.
- The release manifest is a fifth external candidate file. Keeping it outside both archives lets it bind both whole-archive hashes without a recursive hash paradox.
- Package contract tests remain immutable hash-pinned evidence, but their fixture/assertion strings are not interpreted as executable production behavior.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing Critical] Extended source-authority implementation for new package classes**
- **Found during:** Task 1 (source classification)
- **Issue:** The plan named the fixture/tests but the Rust authority enum and exact allowlist could not represent `releaseOrchestration` or `packageTestOnly`.
- **Fix:** Added both non-product classes, expanded the exact allowlist, and skipped executable-token scanning only for hash-pinned package-test code.
- **Files modified:** `crates/compat-harness/src/source_authority.rs`
- **Verification:** All 14 source-authority tests and `verify-candidate` pass.
- **Committed in:** `6e64e84`

**2. [Rule 1 - Bug] Distinguished allowed embedding documentation from model payloads**
- **Found during:** Task 2 real candidate verification
- **Issue:** The old broad path regex treated `docs/release/embedding-package.md` as bundled model content.
- **Fix:** Restricted rejection to real model/embedding resource directories and weight extensions while keeping the documentation in the canonical set.
- **Files modified:** `scripts/release/verify-rust-release.mjs`
- **Verification:** The real candidate passes embedding exclusion and the complete release verifier.
- **Committed in:** `93506a6`

---

**Total deviations:** 2 auto-fixed (1 missing critical contract, 1 verifier bug).
**Impact on plan:** Both fixes enforce the planned distribution boundary without adding dependencies, product behavior, platforms, publication, or network access.

## Issues Encountered

- The first real verification run stopped on the embedding-document false positive before any evidence was accepted. Candidates were regenerated after the fix so their product fingerprint binds the final source.
- This workstation has no MSVC linker. The real local build used the existing GNU-LLVM/rust-lld toolchain and is correctly labeled `windows-x86_64-gnullvm-dev` / `development_only`.

## User Setup Required

None - no external service configuration required.

## Verification

- `node --test scripts/release/package-contract.test.mjs` - 5 passed.
- Offline GNU-LLVM `cargo test -p minimax-compat-harness --test source_authority --locked` - 14 passed.
- Offline GNU-LLVM `cargo test -p minimax-cli --test product_identity --locked` - 4 passed.
- Offline GNU-LLVM `cargo fmt --all -- --check` - passed.
- Offline GNU-LLVM `cargo clippy --workspace --all-targets --locked -- -D warnings` - passed.
- Offline GNU-LLVM `npm run build:rust:release` - passed.
- Two `npm run package:rust` runs - five output files each; every corresponding SHA-256 matched.
- `npm run verify:rust-release` - passed for exact GNU-LLVM development target, manifest, archives, installed Rust identity, capability smoke, licenses, security, and budgets.
- Offline GNU-LLVM `cargo run -p minimax-compat-harness --locked -- verify-candidate` - passed.
- No network request, credential read, dependency download, runtime download, publish, push, PR, tag, merge, or hosted trigger occurred.

## Next Phase Readiness

- Plan 13-03 can add exhaustive local corruption matrices and explicit fingerprint/evidence paths on top of the canonical five-file artifact set.
- Hosted MSVC/Linux GNU evidence remains Phase 14 work and cannot be satisfied by the local GNU-LLVM candidate.

## Self-Check: PASSED

- All nine created/modified contract, packaging, verification, fixture, and test files exist and are committed.
- RED/GREEN commits `9d37bcc`, `6e64e84`, `d1aaa12`, and `93506a6` exist on `codex/rust-convergence-v3`.
- Contract, negative matrix, reproducibility, real packaging, independent verification, identity smoke, Clippy, and candidate gates pass offline.
- Every deliverable has passing automated evidence and requires no subjective UAT.

---
*Phase: 13-thin-npm-and-native-release*
*Completed: 2026-07-18*
