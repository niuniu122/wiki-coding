---
phase: 14-typescript-removal-and-hosted-closure
plan: "02"
subsystem: permanent-cutover-gates
tags: [rust, source-authority, ci, fingerprint, release-evidence, offline]

requires:
  - phase: 14-01-verified-typescript-removal
    provides: Rust-only post-deletion tree and zero transitional authority
provides:
  - Permanent zero-TypeScript, dependency-free package, no-fallback, retained-fixture, and single-state-root enforcement
  - Rust-only read-only Windows/Linux CI composition with the Linux sandbox canary
  - Matching Rust/MJS v3 product fingerprint over current tracked and untracked working-tree bytes
  - Stable stale-hosted-evidence rejection and exact hosted/development target separation
  - Candidate milestone evidence containing installed identities, hashes, evaluations, compatibility, migration, package, license, security, performance, and target facts
affects: [14-03-documentation-and-hosted-closure, release, ci, compatibility]

tech-stack:
  added: []
  patterns:
    - Hash current working-tree bytes rather than Git index blobs while retaining index mode identity
    - Exclude only planning files and the hosted evidence record from the v3 product fingerprint
    - Keep candidate verification positive while strict verification rejects a stale hosted record with a stable error

key-files:
  created: []
  modified:
    - .github/workflows/ci.yml
    - crates/compat-harness/src/source_authority.rs
    - crates/compat-harness/tests/source_authority.rs
    - crates/compat-harness/src/baseline.rs
    - crates/compat-harness/src/lib.rs
    - crates/compat-harness/tests/compat_report.rs
    - scripts/release/product-fingerprint.mjs
    - scripts/release/verify-milestone-flow.mjs
    - fixtures/compat/source-authority.v1.json

key-decisions:
  - "The package lock is validated as one exact dependency-free Rust distribution object, so TypeScript, React, Ink, lifecycle, and transitive package authority cannot re-enter."
  - "The v3 fingerprint uses index mode plus SHA-256 of current working-tree bytes for tracked inputs and SHA-256 for untracked inputs."
  - "A fingerprint/file-count mismatch has a dedicated stale-evidence error; all other hosted schema, job, platform, tier, and performance failures remain fail-closed cutover errors."
  - "Local x86_64-pc-windows-gnullvm evidence remains development_only and is explicitly rejected as Windows MSVC hosted authority."

patterns-established:
  - "Permanent source authority: compiler configs, dist, TS/TSX, unexpected JS, package dependencies, missing retained fixtures, and extra writable roots fail before product verification."
  - "Hosted closure: only exact Windows MSVC and Linux GNU jobs bound to the final v3 fingerprint can satisfy strict evidence."

requirements-completed: []

coverage:
  - id: D1
    description: Permanent source, dependency, CI-order, fallback, fixture, and state-root gates reject Rust-only cutover regressions.
    requirement: RCUT-01
    verification:
      - kind: integration
        ref: crates/compat-harness/tests/source_authority.rs
        status: pass
      - kind: integration
        ref: cargo run -p minimax-compat-harness --locked -- verify-candidate
        status: pass
    human_judgment: false
  - id: D2
    description: Rust and MJS v3 fingerprints match, change for tracked/untracked product edits, and ignore only planning and hosted-record edits.
    requirement: RCUT-02
    verification:
      - kind: integration
        ref: product_fingerprint_v3_tracks_working_tree_and_excludes_only_planning_and_hosted_record
        status: pass
      - kind: integration
        ref: target/phase14-gates-final-v3/fingerprint-final.json
        status: pass
    human_judgment: false
  - id: D3
    description: Stale hosted evidence and GNU-LLVM development evidence cannot satisfy final hosted Windows MSVC/Linux closure.
    requirement: RCUT-02
    verification:
      - kind: integration
        ref: stale_hosted_evidence_is_rejected_for_freshness_or_fingerprint
        status: pass
      - kind: integration
        ref: gnullvm_development_evidence_cannot_satisfy_hosted_msvc
        status: pass
    human_judgment: false

duration: 1h
completed: 2026-07-18
status: complete
---

# Phase 14 Plan 02: Permanent Rust-Only Gates and v3 Fingerprint Summary

**The post-deletion tree is permanently Rust-authoritative, and release candidates are bound to current working-tree bytes through a cross-language v3 fingerprint while stale or tier-confused hosted evidence fails closed.**

## Performance

- **Duration:** 1 hour
- **Completed:** 2026-07-18
- **Tasks:** 2
- **Product inputs:** 235

## Accomplishments

- Enforced zero TypeScript/TSX, zero compiler configuration, zero generated `dist`, exact dependency-free package/lock metadata, exact retained compatibility fixtures, no JavaScript product/fallback authority, and one Rust-owned writable state root.
- Moved all Phase 14 CI artifact paths to `target/phase14-ci`, retained read-only permissions, exact Ubuntu/Windows x64 matrix order, and the Linux Bubblewrap canary.
- Replaced index-blob fingerprint v2 with working-tree-byte fingerprint v3 in both Rust and MJS; tracked and untracked product edits change it, while `.planning/**` and `fixtures/compat/release/hosted-gates.v1.json` do not.
- Added the stable `HostedEvidenceFingerprintStale` result, a passing dedicated stale-evidence negative test, and a separate GNU-LLVM-versus-MSVC tier-confusion test.
- Expanded milestone candidate evidence to include exact target facts, artifact hashes, native/npm installed identities, earlier ordered Rust gates, migration, license, security, performance, and offline counters.

## Task Commits

1. **Task 1 RED: Define permanent Rust-only source regressions** - `5d9940c` (test)
2. **Task 1 GREEN: Enforce permanent source/package/CI authority** - `165ad32` (feat)
3. **Task 2 RED: Define v3 fingerprint and stale evidence behavior** - `5ffc59a` (test)
4. **Task 2 GREEN: Bind release evidence to fingerprint v3** - `1ce3a10` (feat)
5. **Verification fix: Validate structured performance evidence** - `234a99c` (fix)

## Final Local Evidence

- Provisional v3 fingerprint: `5e4b27b23a28627b6f4aeae3d8e8d566bc49fe128e5a66057983d049da401f52` across 235 product files.
- Binary SHA-256: `28a6c98262837519c43bd68f9eaa019dd587b5c99ae6d86193b07659f02d9c81`.
- Native archive SHA-256: `cf6e87ab317b73dd6a67cdcfa26a21d23d61fa5496691fdc2b96afe2222c14d2`.
- npm archive SHA-256: `d47717f1430eea6986b71741be4c59f5415e2b459e750057ca26e6173959a41a`.
- Native/npm identity: `minimax-codex-rust 0.1.0`, identical binary and capability-output hashes.
- Performance: 20.354 ms cold-start p95, 4,968,448-byte max idle RSS, and 2.996 ms Wiki BM25 p95.
- Offline/provider/credential/model-download counters: `true / 0 / 0 / 0`.
- Target: `windows-x86_64-gnullvm-dev`, support tier `development_only`.

The fingerprint is provisional because 14-03 documentation is intentionally included as product input. This local evidence is not hosted Windows MSVC or Linux GNU release authority.

## Deviations from Plan

### Auto-fixed Issues

**1. Release threshold parsing omitted its target-contract schema link**
- **Found during:** dedicated stale-evidence GREEN test
- **Issue:** strict hosted validation treated the existing `targetContractSchemaVersion` field as an unknown field before reaching the fingerprint comparison.
- **Fix:** Added and validated the exact schema link, allowing stale records to reach the stable fingerprint-freshness classification.
- **Committed in:** `1ce3a10`

**2. Milestone performance validation initially assumed scalar values**
- **Found during:** actual release/milestone verification
- **Issue:** release evidence records cold start and RSS as sample objects with `p95` and `maximum`, not scalar values.
- **Fix:** Validate exact sample counts, positive values, computed extrema, thresholds, and package-size binding.
- **Committed in:** `234a99c`

**3. PowerShell added a UTF-8 BOM to one generated fingerprint file**
- **Found during:** local package verification
- **Issue:** the strict JSON loader rejected the BOM-bearing generated file.
- **Fix:** Regenerated the target-only file directly with Node UTF-8 output and used fresh artifact/evidence directories.
- **Committed in:** N/A; no source change.

**4. GNU-LLVM direct-binary smoke required its installed runtime search path**
- **Found during:** local release verification
- **Issue:** the development-only binary could not locate the installed `libunwind` runtime when launched directly.
- **Fix:** Added only the existing GNU-LLVM toolchain runtime directory to the verification process `PATH`; target identity remained `development_only`.
- **Committed in:** N/A; no source or evidence-tier change.

No Provider/API call, credential read, dependency/model download, push, PR, publication, tag, merge, or hosted workflow trigger occurred.

## Verification

- Offline format and full workspace Clippy (`--all-targets`, `-D warnings`) - passed.
- Full offline candidate workspace and doc tests - passed; only the intentionally stale hosted-positive test was skipped.
- Source-authority suite - 19/19 passed.
- Dedicated stale-evidence and GNU-LLVM tier-confusion negative tests - passed.
- Provider evaluation - 20/20 passed.
- Retrieval evaluation - 175 cases; every locked metric 1.0 and all external-path counters zero.
- Package corruption suite - 19/19 passed.
- Release build, deterministic package assembly, native/npm installed smoke, hashes, licenses, security, performance, and schema-v3 milestone flow - passed.
- Strict verification - rejected the old hosted record with `hosted release evidence is stale for the current product fingerprint` as required.

## Next Phase Readiness

- 14-03 Task 1 can finalize Rust-only user/maintainer documentation and then freeze the true final v3 fingerprint.
- 14-03 Task 2 remains an external authorization checkpoint for commit/push/hosted CI/intake; no such action has occurred.

## Self-Check: PASSED

- Commits `5d9940c`, `165ad32`, `5ffc59a`, `1ce3a10`, and `234a99c` exist on `codex/rust-convergence-v3`.
- Rust and MJS produce the same 235-file v3 fingerprint on the clean working tree.
- Candidate verification and the complete local package evidence chain pass offline.
- Strict closure rejects the current stale hosted record, and local GNU-LLVM remains development-only.

---
*Phase: 14-typescript-removal-and-hosted-closure*
*Completed: 2026-07-18*
