---
phase: 14-typescript-removal-and-hosted-closure
plan: "03"
subsystem: hosted-release-closure
tags: [rust, github-actions, msvc, reproducibility, release-evidence, documentation]

requires:
  - phase: 14-02-permanent-rust-only-gates
    provides: Rust-only source authority, working-tree fingerprint v3, and fail-closed hosted-evidence validation
provides:
  - Rust-only user and maintainer documentation for npm/native installation, migration, rollback, supported targets, and the two-release support window
  - Frozen 235-file v3 product fingerprint with bound local intake and development-only package evidence
  - Genuine hosted Windows MSVC and Linux GNU candidate plus strict evidence with byte-identical per-platform artifacts
  - Final schema-v2 hosted cutover record accepted by the strict Rust compatibility gate
affects: [v3.0-milestone-closure, release, ci, compatibility, migration]

tech-stack:
  added: []
  patterns:
    - Candidate evidence is committed as pending before the ordinary strict push run can start
    - Remote evidence commits advance non-force only after their exact Git tree matches the local tree
    - Windows MSVC release linking uses /Brepro and candidate/strict artifact equality is mandatory

key-files:
  created:
    - .planning/debug/resolved/hosted-evidence-schema-drift.md
    - .planning/debug/resolved/hosted-product-fingerprint-eol-drift.md
    - .planning/debug/resolved/linux-launcher-enoexec-fallback.md
    - .planning/debug/resolved/linux-migration-fixture-line-ending-drift.md
  modified:
    - README.md
    - docs/release/install-upgrade-rollback.md
    - docs/release/cutover.md
    - .github/workflows/ci.yml
    - bin/minimax-codex.cjs
    - crates/compat-harness/src/baseline.rs
    - crates/compat-harness/src/source_authority.rs
    - crates/compat-harness/tests/compat_report.rs
    - crates/compat-harness/tests/source_authority.rs
    - fixtures/compat/release/hosted-gates.v1.json

key-decisions:
  - "Keep local x86_64-pc-windows-gnullvm evidence development_only; only hosted x86_64-pc-windows-msvc and x86_64-unknown-linux-gnu can close RCUT-02."
  - "Require a candidate-only pending record to be remotely visible before one ordinary push may run strict verification."
  - "Use universal LF checkout policy for fingerprint inputs while retaining the manifest-bound migration fixture as an explicit CRLF -text exception."
  - "Pass /Brepro only on the Windows MSVC CI matrix entry and require exact candidate/strict binary, native archive, npm archive, size, and capability hashes."
  - "Advance the evidence branch through exact-tree Git Data API commits without force because local and remote histories intentionally differ."

patterns-established:
  - "Hosted closure order: freeze -> candidate -> validate -> pending record -> strict -> compare -> final record."
  - "Every hosted failure stops before strict or final-record mutation; successful workflow status alone is insufficient without composable artifacts."

requirements-completed: [RCUT-02, RCUT-03]

coverage:
  - id: D1
    description: Rust-only user and maintainer documentation covers architecture, npm/native installation, supported platforms, fail-closed errors, migration/rollback, and the two-release compatibility window.
    requirement: RCUT-03
    verification:
      - kind: integration
        ref: npm run verify:rust-contracts
        status: pass
    human_judgment: true
    rationale: Documentation completeness and clarity require human-readable review in addition to hash and source-authority checks.
  - id: D2
    description: One final 235-file fingerprint binds the local binary, native/npm packages, release evidence, milestone evidence, and intake without using local GNU-LLVM as hosted authority.
    requirement: RCUT-02
    verification:
      - kind: integration
        ref: node scripts/release/product-fingerprint.mjs plus final-v3 intake hash binding
        status: pass
      - kind: integration
        ref: npm run verify:rust-release and npm run verify:milestone-flow with final-v3 paths
        status: pass
    human_judgment: false
  - id: D3
    description: Hosted candidate and strict Windows MSVC/Linux GNU jobs pass every required gate and produce identical per-platform binary/native/npm artifacts.
    requirement: RCUT-02
    verification:
      - kind: e2e
        ref: GitHub Actions runs 29638773706 and 29639243817
        status: pass
      - kind: integration
        ref: hosted_cutover_evidence_matches_current_product
        status: pass
    human_judgment: false
  - id: D4
    description: The final hosted record preserves offline and zero Provider, credential-read, and model-download counters while Linux proves its adversarial sandbox canary.
    requirement: RCUT-02
    verification:
      - kind: integration
        ref: npm run verify:rust-contracts
        status: pass
      - kind: e2e
        ref: fixtures/compat/release/hosted-gates.v1.json
        status: pass
    human_judgment: false

duration: 5h 11m
completed: 2026-07-18
status: complete
---

# Phase 14 Plan 03: Manual Hosted Evidence and v3 Closure Summary

**The Rust-only product is frozen to one 235-file fingerprint and closed by reproducible hosted Windows MSVC/Linux GNU candidate and strict evidence.**

## Performance

- **Duration:** 5h 11m
- **Started:** 2026-07-18T04:39:28Z
- **Completed:** 2026-07-18T09:50:08Z
- **Tasks:** 3
- **Tracked files modified:** 21

## Accomplishments

- Replaced remaining dual-runtime documentation with Rust-only architecture, npm/native installation, supported-target, no-fallback, migration/rollback, and two-release compatibility guidance.
- Froze product fingerprint `513c7565593b3e3088131d2854709be4773f0a81c2445c146f4a5acb597d29b6` across 235 files and bound the exact development-only binary, package, release, milestone, and intake files.
- Collected successful candidate run `29638773706` and strict run `29639243817` from hosted Windows MSVC and Linux GNU jobs.
- Proved candidate/strict artifact reproducibility on both platforms; Windows binary/native/npm hashes are `8cfb7ffd...a2bf`, `d71c337a...389c`, and `9dd6b7bd...7523`, while Linux remains `e380053f...9137`, `eeebb397...e062`, and `13e504ce...5c4d`.
- Sealed the final hosted record with offline true and Provider/credential/model-download counters all zero; strict Rust validation and the full exact-root local closure pass.

## Task Commits

1. **Task 1: Finalize Rust-only docs, freeze fingerprint, and prepare intake** - `4e3ba0b` (docs)
2. **Task 2: Obtain authorized hosted evidence and establish the pending strict precondition** - `e80d1cf`, `581da6c`, `1eff73b`, `f43cdfd`, `7e25d35`, `25ff903`, `7683289`, `f99d7d8` (fix/chore)
3. **Task 3: Validate strict artifacts and seal the combined record** - `508d671` (chore)

## Files Created/Modified

- `README.md` - Rust-only architecture, supported installation paths, and development guidance.
- `docs/release/install-upgrade-rollback.md` - Supported targets, deterministic install/upgrade/rollback, and no-fallback failures.
- `docs/release/cutover.md` - Candidate/pending/strict evidence order and two-release migration-fixture window.
- `.github/workflows/ci.yml` - Candidate/strict lifecycle, exact evidence upload, cross-platform fingerprint policy, and Windows `/Brepro` linking.
- `bin/minimax-codex.cjs` - Native PE/ELF preflight preventing Linux ENOEXEC shell fallback.
- `.gitattributes` - Universal LF checkout policy plus the manifest-bound CRLF migration exception.
- `fixtures/compat/release/hosted-gates.v1.json` - Final candidate+strict hosted closure record.
- `.planning/debug/resolved/*.md` - Reproducible diagnoses for hosted schema, launcher, fixture, and fingerprint failures.

## Decisions Made

- Workflow success is necessary but not sufficient: downloaded evidence must compose into one fingerprint and exact per-platform artifact identity before closure.
- Candidate and strict are distinct independent builds; Windows MSVC `/Brepro` is enforced by source-authority tests and exact artifact equality.
- The evidence branch never used force. Every remote ref update required an unchanged parent and a Git tree equal to the local committed tree.
- Local GNU-LLVM remains useful development evidence but is never relabeled as Windows MSVC hosted release evidence.

## Deviations from Plan

### Auto-fixed Issues

**1. Hosted evidence lifecycle and schema could not represent the required pending-to-strict order**
- **Found during:** Task 2 first candidate preparation
- **Issue:** Candidate and strict evidence ownership was not encoded tightly enough to prevent premature strict closure.
- **Fix:** Added explicit candidate/pending/passed validation, exact run/job URLs, ordering, target tiers, and artifact identity checks.
- **Verification:** Dedicated pending/final negative tests and `npm run verify:rust-contracts` pass.
- **Committed in:** `e80d1cf`

**2. Linux could invoke `/bin/sh` through ENOEXEC for a non-ELF sibling**
- **Found during:** hosted candidate run `29633595951`
- **Issue:** The launcher returned shell exit 127 instead of the stable fail-closed launcher error.
- **Fix:** Added platform-native PE/ELF magic preflight before process spawn.
- **Verification:** Linux launcher regression and source-authority tests pass.
- **Committed in:** `581da6c`

**3. One historical migration fixture had platform-dependent checkout bytes**
- **Found during:** hosted candidate run `29634802180`
- **Issue:** Linux observed LF while the immutable manifest required the canonical 34-byte CRLF payload.
- **Fix:** Marked the migration fixture subtree `-text` and restored the exact manifest-bound blob.
- **Verification:** Migration-support tests pass on both hosted targets.
- **Committed in:** `1eff73b`

**4. Five text inputs produced different Windows/Linux product fingerprints**
- **Found during:** hosted candidate run `29635653025`
- **Issue:** Working-tree-byte fingerprint v3 had no universal checkout policy.
- **Fix:** Added `* text=auto eol=lf`, retained the explicit migration exception, and added cross-checkout regression coverage.
- **Verification:** clean `core.autocrlf=true/false` worktrees produce the same 235-file fingerprint.
- **Committed in:** `f43cdfd`

**5. Windows MSVC candidate and strict builds were not byte-reproducible**
- **Found during:** candidate run `29636689979` versus strict run `29637083930`
- **Issue:** Both runs passed, but Windows binary and archive hashes changed because the linker embedded nondeterministic data.
- **Fix:** Added Windows-only `-C link-arg=/Brepro` and fail-closed workflow contract tests.
- **Verification:** Replacement candidate/strict Windows binary and both archives are exactly equal.
- **Committed in:** `25ff903`, `7683289`

**6. The initial job-level expression used an unavailable GitHub context**
- **Found during:** replacement dispatch preflight
- **Issue:** GitHub rejected `runner.os` at job-level `env` with HTTP 422 before creating a run.
- **Fix:** Switched the expression to the available `matrix.os` context and locked it with RED/GREEN tests.
- **Verification:** Replacement candidate and strict workflows both evaluate and pass.
- **Committed in:** `7683289`

**7. One local cold-start measurement exceeded the threshold transiently**
- **Found during:** final local release verification
- **Issue:** The first p95 was 525.121 ms while all frozen artifacts were unchanged.
- **Fix:** Re-ran only the verifier against the same binary and archives; no rebuild or repackaging occurred.
- **Verification:** Final exact-root verification reports 95.942 ms p95 below the 500 ms limit and intake binds the current evidence hashes.
- **Committed in:** N/A; generated evidence only.

---

**Total deviations:** 7 auto-fixed (hosted contract, cross-platform behavior, immutable fixture bytes, fingerprint portability, MSVC reproducibility, workflow syntax, and transient measurement).
**Impact on plan:** Every fix was required to make the planned evidence truthful and reproducible. No publication, tag, PR, merge, Provider call, credential read, model download, or real-data migration entered scope.

## Issues Encountered

- Four earlier candidate attempts and the first strict attempt exposed independent fail-closed gaps. None was reused as final proof; each invalid result stopped the sequence until its product tree was repaired and re-frozen.
- Git HTTPS transport was unavailable in this environment. Authenticated GitHub Git Data API updates were used with exact-tree and unchanged-parent checks, never force.

## User Setup Required

None - no external service configuration is required.

## Next Phase Readiness

- Phase 14 and all v3.0 implementation plans are complete.
- The code and evidence are ready for milestone audit/completion when the user chooses; npm publication, tag, PR, and merge remain separate unauthorized actions.
- `actions/checkout@v4` and `actions/setup-node@v4` emit an upstream Node 20 runtime deprecation warning, but GitHub ran them under Node 24 and all gates passed.

## Self-Check: PASSED

- All three 14-03 tasks and acceptance criteria pass.
- Local commits `4e3ba0b` through `508d671` exist; hosted candidate/strict runs and job URLs are recorded.
- The final hosted record fingerprint equals the current product fingerprint and strict Rust verification succeeds.
- The exact `final-v3` binary and package artifacts were reused for final release/milestone closure without cleaning, rebuilding, or repackaging.

---
*Phase: 14-typescript-removal-and-hosted-closure*
*Completed: 2026-07-18*
