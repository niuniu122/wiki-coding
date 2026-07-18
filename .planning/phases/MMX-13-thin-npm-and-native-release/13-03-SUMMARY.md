---
phase: 13-thin-npm-and-native-release
plan: "03"
subsystem: distribution
tags: [rust, npm, offline, corruption, fingerprint, installed-smoke, ci]

requires:
  - phase: 13-02-checksummed-native-and-npm-artifacts
    provides: Deterministic native/npm candidates and strict schema-v2 release manifests
provides:
  - Eleven-category fail-closed package corruption matrix with stable actionable errors
  - Explicit fingerprint, binary, artifact, and evidence path ownership across every release command
  - Separate offline native and npm installed identities bound to one Rust binary and product fingerprint
  - CI ordering that blocks candidate upload on every Rust, evaluator, compatibility, package, and installed gate
affects: [14-typescript-removal-and-hosted-closure, release, packaging, ci]

tech-stack:
  added: []
  patterns:
    - Validate package bytes and metadata before any installed command can run
    - Extract native and npm candidates independently and bind both identities to the same exact Rust binary
    - Require caller-supplied current fingerprint and task-contained artifact/evidence roots; never select stale defaults

key-files:
  created: []
  modified:
    - scripts/release/package-contract.mjs
    - scripts/release/package-contract.test.mjs
    - scripts/release/package-rust.mjs
    - scripts/release/verify-rust-release.mjs
    - scripts/release/verify-milestone-flow.mjs
    - package.json
    - .github/workflows/ci.yml
    - fixtures/compat/source-authority.v1.json
    - crates/compat-harness/tests/source_authority.rs

key-decisions:
  - "Expose stable ARTIFACT_* corruption categories and reject all invalid candidates before installation or evidence generation."
  - "Require explicit binary, artifact, fingerprint, and evidence paths so a healthy source tree or stale target output cannot substitute for packed evidence."
  - "Treat native and npm extraction as separate installed paths whose exact binary hash, version, read-only capability output, and no-I/O counters must agree."
  - "Keep CI read-only and candidate-only in Phase 13; hosted MSVC/Linux evidence remains Phase 14 authority."

patterns-established:
  - "Package negative matrix: missing, wrong target, renamed, non-executable, unsafe type, checksum drift, product drift, launcher drift, extra executable, corrupt archive, and invalid sidecar."
  - "Installed evidence schema v2: both installed identities plus archive/binary hashes, current product fingerprint, offline counters, licenses, security, and performance."

requirements-completed: [RNPM-01, RNPM-03]

coverage:
  - id: D1
    description: Every locked package corruption class fails before installed smoke with one stable actionable category and no alternate child, fallback, or download.
    requirement: RNPM-03
    verification:
      - kind: unit
        ref: scripts/release/package-contract.test.mjs#eleven corruption subtests
        status: pass
      - kind: integration
        ref: npm run test:package
        status: pass
    human_judgment: false
  - id: D2
    description: Package and milestone commands require explicit current fingerprint and task-contained binary, artifact, and evidence paths and reject missing, malformed, stale, or mismatched inputs.
    requirement: RNPM-03
    verification:
      - kind: unit
        ref: scripts/release/package-contract.test.mjs#explicit fingerprint command contract
        status: pass
      - kind: integration
        ref: target/phase13-final-d4fa386/fingerprint.json
        status: pass
    human_judgment: false
  - id: D3
    description: Independently extracted native and npm paths run the same checksummed Rust identity and read-only capability command entirely offline.
    requirement: RNPM-01
    verification:
      - kind: integration
        ref: target/phase13-final-d4fa386/evidence/windows-x86_64-gnullvm-dev.json
        status: pass
      - kind: integration
        ref: target/phase13-final-d4fa386/evidence/milestone-flow-windows-x86_64-gnullvm-dev.json
        status: pass
    human_judgment: false
  - id: D4
    description: CI blocks artifact upload until Rust, evaluator, compatibility, migration, corruption, package, installed, and milestone gates pass in strict order.
    requirement: RNPM-01
    verification:
      - kind: integration
        ref: crates/compat-harness/tests/source_authority.rs#CI release gate order
        status: pass
      - kind: integration
        ref: cargo run -p minimax-compat-harness --locked -- verify-candidate
        status: pass
    human_judgment: false

duration: 49min
completed: 2026-07-18
status: complete
---

# Phase 13 Plan 03: Offline Installed and Corruption Gates Summary

**Native and npm packages now fail closed across every locked corruption class, then prove two independent offline installed paths execute the same exact Rust binary before release evidence can progress.**

## Performance

- **Duration:** 49 min
- **Started:** 2026-07-18T10:08:23+08:00
- **Completed:** 2026-07-18T10:57:23+08:00
- **Tasks:** 3
- **Files modified:** 16

## Accomplishments

- Added eleven named corruption categories covering missing, incompatible, renamed, non-executable, unsafe, hash-drifted, fingerprint-drifted, launcher-drifted, overbroad, truncated, and invalid-sidecar candidates.
- Made package assembly and milestone verification consume explicit current fingerprint and task-contained binary/artifact/evidence paths with stable rejection for absent, malformed, stale, and mismatched inputs.
- Extracted native and npm artifacts into separate isolated roots and proved both report `minimax-codex-rust 0.1.0`, the same binary SHA-256, and the same read-only capability output with zero network, Provider, credential, or model-download activity.
- Ordered read-only CI so format/Clippy/tests, Rust evaluators, compatibility/migration, corruption tests, packaging, installed verification, and milestone flow all block candidate upload.
- Reconciled stale full-suite assertions with the final Phase 13 npm boundary, dual installed identities, package script count, compatibility gate, Provider fixture, and retrieval CI order.

## Task Commits

Each planned task used atomic RED/GREEN or gate commits:

1. **Task 1 RED: Expose corrupt package candidates** - `cb29bec` (test)
2. **Task 1 GREEN: Reject corrupt package candidates** - `05d07c1` (feat)
3. **Task 2 RED: Require explicit release fingerprints** - `08377a6` (test)
4. **Task 2 GREEN: Verify explicit offline installed artifacts** - `a69fe4d` (feat)
5. **Task 3: Gate installed release candidates in CI** - `a683da7` (ci)

Full-regression contract alignment was committed separately as `eac9583`, `f4c88da`, `e0b2592`, `2f35c96`, `9b14f7c`, and `d4fa386`.

## Final Candidate Evidence

- Product fingerprint: `39bbe83352d1be482ecdca59482855732683505112fa23a1cdcc5337f5a1eaee` across 438 files.
- Binary SHA-256: `942adad08597801f037b8dfc1be49937963c12d7c5337055cf669c6ed0c5ffee`.
- Native archive SHA-256: `bdffdf0edd413045af04308f4cd5bd5191e059b2ad8d395f50e8970ee7ad78c2`.
- npm archive SHA-256: `da7fa03d5fa55031ee4ea45884eb64a8a48df2873d477ec04a373cffa821b9f4`.
- Native and npm installed identities: `minimax-codex-rust 0.1.0`; identical capability output hash `7aba6c...b3b0`.
- Offline/provider/credential/model-download counters: `true / 0 / 0 / 0` for both paths and aggregate evidence.
- Performance: 82.616 ms cold-start p95, 4,943,872-byte max idle RSS, 3,374,761-byte native archive, and 2.381 ms Wiki BM25 p95; all below locked limits.
- Security/license: 234 packages checked, 0 invalid licenses, 0 unsafe files, and 0 database packages.

This evidence is `windows-x86_64-gnullvm-dev` / `development_only`. It does not claim or replace hosted Windows MSVC or Linux GNU evidence.

## Deviations from Plan

### Auto-fixed Issues

**1. Full-suite package authority assertions still described the pre-Phase-13 npm boundary**
- **Found during:** Final workspace regression
- **Issue:** Headless, product identity, script-count, and coverage tests still expected one installed identity or older npm scripts.
- **Fix:** Updated those assertions to the exact one-command npm contract, both installed identities, fourteen allowed scripts, and the current compatibility gate.
- **Committed in:** `eac9583`, `f4c88da`, `e0b2592`, `2f35c96`

**2. Provider and retrieval CI fixtures had stale repository/order assumptions**
- **Found during:** Final workspace regression
- **Issue:** The Provider temporary repository omitted the immutable public-contract evidence, and retrieval expected the older CI step order.
- **Fix:** Copied the required immutable fixture into the temporary repository and asserted the final evaluator/package gate order.
- **Committed in:** `9b14f7c`, `d4fa386`

No product behavior, threshold, authorization boundary, supported platform, or external action was added beyond the plan.

## User Setup Required

None - no external service configuration required.

## Verification

- Offline GNU-LLVM `cargo fmt --all -- --check` - passed.
- Offline GNU-LLVM workspace Clippy with all targets and warnings denied - passed.
- Offline GNU-LLVM full workspace tests and doc tests - passed; only the intentional Phase 14 hosted-freshness test was skipped.
- Rust Provider evaluation - 20/20 passed.
- Rust retrieval evaluation - all locked metrics 1.0 over 175 cases; zero disabled-path network/Provider/download activity.
- `npm run test:package` - 19 passed, including eleven corruption subtests.
- Explicit final fingerprint, package, release verification, and milestone-flow chain under `target/phase13-final-d4fa386` - passed.
- Offline GNU-LLVM `cargo run -p minimax-compat-harness --locked -- verify-candidate` - passed.
- No network request, dependency download, credential read, live Provider request, model download, publication, push, PR, tag, merge, deletion, or hosted trigger occurred.

## Next Phase Readiness

- Phase 13 is complete locally: one no-fallback npm command, deterministic native/npm artifacts, exhaustive corruption rejection, and both installed package paths are release-gated.
- Phase 14 still owns TypeScript removal and fresh hosted Windows MSVC/Linux GNU evidence. Both require the existing authorization boundary; this run does not begin either action.

## Self-Check: PASSED

- All eleven implementation/test commits exist on `codex/rust-convergence-v3`.
- The final current-fingerprint artifact chain, full workspace regression, Rust evaluations, package matrix, source authority, CI order, and candidate gate pass offline.
- Every planned deliverable has automated evidence and requires no subjective UAT.

---
*Phase: 13-thin-npm-and-native-release*
*Completed: 2026-07-18*
