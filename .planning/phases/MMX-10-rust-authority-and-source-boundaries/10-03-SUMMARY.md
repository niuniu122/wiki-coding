---
phase: MMX-10-rust-authority-and-source-boundaries
plan: "03"
subsystem: release-and-ci-authority
tags: [rust, npm-launcher, release-packaging, source-authority, ci]
requires:
  - phase: MMX-10-rust-authority-and-source-boundaries
    plan: "02"
    provides: Sole supported npm command, fixed Rust launcher, and Rust-owned writable state authority
provides:
  - native and npm candidate archives with only the launcher and one Rust executable product path
  - direct-versus-installed Rust version and binary-hash identity evidence
  - fail-closed CI ordering that runs Rust authority before packaging and installed smoke
affects: [phase-11-rust-verification, phase-13-thin-npm, phase-14-hosted-closure]
tech-stack:
  added: []
  patterns: [single-product release manifests, controlled installed smoke environments, Rust-first CI authority]
key-files:
  created:
    - crates/cli/tests/product_identity.rs
  modified:
    - scripts/release/package-rust.mjs
    - scripts/release/verify-rust-release.mjs
    - scripts/release/verify-milestone-flow.mjs
    - crates/compat-harness/src/source_authority.rs
    - crates/compat-harness/tests/source_authority.rs
    - .github/workflows/ci.yml
    - fixtures/compat/source-authority.v1.json
key-decisions:
  - "Candidate archives expose only the fixed launcher and one platform Rust binary; generated TypeScript output is not packaged."
  - "Installed identity is proven by matching direct and launcher version output plus the exact packaged binary SHA-256 under a credential-free, no-PATH-lookup environment."
  - "CI runs strict or candidate Rust contracts before transitional Node checks and all packaging/smoke steps; Node no longer builds the TypeScript product."
  - "Milestone evidence is selected by the actual rustc host so GNU-LLVM development evidence cannot satisfy Windows MSVC or Linux GNU release authority."
patterns-established:
  - "Release smoke extracts into isolated target directories, validates safe tar entries, and removes temporary direct/installed smoke roots."
  - "CI authority tests mutate workflow text to prove contracts, ordering, permissions, credentials, matrix, and canary boundaries fail closed."
requirements-completed: [RUST-01, RUST-02, RUST-03]
coverage:
  - id: D1
    description: "Native and npm candidates contain no legacy manifest field or generated TypeScript payload and expose only one Rust product path."
    requirement: RUST-01
    verification:
      - kind: integration
        ref: "npm run package:rust -- --output target/release-artifacts-10-03-final && npm run verify:rust-release -- --artifacts target/release-artifacts-10-03-final --binary target/gnullvm-dev/release/minimax-cli.exe"
        status: pass
    human_judgment: false
  - id: D2
    description: "Direct Rust and extracted npm launcher commands report the same package identity and execute the exact manifest-bound binary."
    requirement: RUST-01
    verification:
      - kind: integration
        ref: "crates/cli/tests/product_identity.rs"
        status: pass
      - kind: integration
        ref: "installedRustIdentity in target/release-evidence/windows-x86_64-gnullvm-dev.json"
        status: pass
    human_judgment: false
  - id: D3
    description: "Rust source authority fails CI closed before packaging while permissions, credentials, matrix targets, and the Linux canary remain constrained."
    requirement: RUST-02
    verification:
      - kind: integration
        ref: "crates/compat-harness/tests/source_authority.rs#ci_keeps_rust_authority_ahead_of_packaging_and_fails_closed"
        status: pass
      - kind: integration
        ref: "cargo run -p minimax-compat-harness --locked -- verify-candidate"
        status: pass
    human_judgment: false
duration: 31min
completed: 2026-07-17
status: complete
---

# Phase 10 Plan 03: Sole-Authority Release and Entry Gates Summary

**Deterministic native/npm candidates now contain one Rust product path, prove installed identity by version and SHA-256, and cannot bypass Rust-first CI authority.**

## Performance

- **Duration:** 31 min
- **Started:** 2026-07-17T12:49:21Z
- **Completed:** 2026-07-17T13:20:24Z
- **Tasks:** 3
- **Files created/modified:** 8

## Accomplishments

- Removed `dist` and the legacy product field from deterministic candidate assembly while preserving licenses, documentation, checksum, tar-safety, size, security, license, embedding-exclusion, and performance gates.
- Added Rust-owned identity tests and installed-package smoke that compare `minimax-codex-rust 0.1.0` across direct and launcher paths, bind execution to the packaged binary hash, exclude credentials/PATH lookup, and reject missing or unsafe siblings.
- Made source authority the first post-install CI gate for both strict and hosted-candidate branches, removed the transitional TypeScript product build from CI, and added mutation tests for ordering, permissions, credentials, matrix, canary, and publish boundaries.
- Bound milestone-flow evidence to the active Rust host and the complete installed identity object, preserving GNU-LLVM as development-only evidence.

## Task Commits

Each task was committed atomically with its TDD evidence where required:

1. **Task 1 RED: reject legacy release authority** - `d0343b5` (test)
2. **Task 1 GREEN: ship Rust-only candidate archives** - `cd166af` (feat)
3. **Task 2 RED: packaged identity contract** - `f9e0d8b` (test)
4. **Task 2 GREEN: bind installed launcher to Rust identity** - `162df17` (feat)
5. **Task 3: enforce Rust authority before CI packaging** - `4f0cf3d` (chore)
6. **Deviation fix: bind milestone evidence to Rust host** - `ce2963f` (fix)

The summary and planning trackers are committed together in the final plan metadata commit.

## Files Created/Modified

- `crates/cli/tests/product_identity.rs` - Direct Rust identity and structural installed-smoke contract tests.
- `scripts/release/package-rust.mjs` - Single-product native/npm deterministic archive construction.
- `scripts/release/verify-rust-release.mjs` - Exact archive, extracted binary, installed identity, controlled-environment, sibling, and functional smoke verification.
- `scripts/release/verify-milestone-flow.mjs` - Rust-host-bound evidence selection and installed identity validation.
- `crates/compat-harness/src/source_authority.rs` - Mandatory CI authority contract validation.
- `crates/compat-harness/tests/source_authority.rs` - Workflow mutation tests for ordering and safety invariants.
- `.github/workflows/ci.yml` - Rust contracts before transitional checks/package; no TypeScript product build.
- `fixtures/compat/source-authority.v1.json` - Reviewed hashes for the modified JavaScript authority files.

## Decisions Made

- Kept npm as the thin supported distribution shell while excluding the generated TypeScript application from both candidate archive types.
- Kept transitional TypeScript checks/evaluations temporarily, but made Rust contracts authoritative and removed `npm run build` from CI; later phases still own test/evaluation retirement and metadata minimization.
- Used a controlled environment with absolute executable paths for installed smoke. The GNU-LLVM-only `libunwind.dll` is copied into temporary smoke roots and explicitly reported as development augmentation; it is never packaged or represented as MSVC evidence.
- Preserved prior generated MSVC artifacts and used isolated Phase 10 artifact directories rather than deleting unrelated evidence.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Refreshed hash-pinned JavaScript authority entries**

- **Found during:** Tasks 1 and 2 plus final milestone correction
- **Issue:** The Phase 10 source manifest deliberately hash-pins every permitted JavaScript file, so the required release-script edits made the authority baseline stale.
- **Fix:** Updated only the reviewed SHA-256 entries for package, release verification, and milestone verification scripts.
- **Files modified:** `fixtures/compat/source-authority.v1.json`
- **Verification:** `repository_source_inventory` and all seven source-authority tests pass.
- **Committed in:** `d0343b5`, `cd166af`, `162df17`, `ce2963f`

**2. [Rule 3 - Blocking Environment] Used the installed GNU-LLVM/rust-lld development fallback and isolated artifact directories**

- **Found during:** Task 1 and overall release verification
- **Issue:** The workstation lacks both the MSVC linker and GNU-LLVM's default `x86_64-w64-mingw32-clang`; the default artifact directory also contained preserved prior MSVC archives.
- **Fix:** Used the installed `1.97.0-x86_64-pc-windows-gnullvm` toolchain with its bundled `rust-lld`, supplied `libunwind.dll` only to temporary development smoke roots, and wrote new candidates to isolated target directories.
- **Files modified:** None for the toolchain/artifact isolation; development-runtime handling is in `verify-rust-release.mjs`.
- **Verification:** Build, package, installed identity, milestone flow, formatting, tests, and strict Clippy pass locally. Evidence remains `windows-x86_64-gnullvm-dev` with `supportTier: development_only`.
- **Committed in:** `162df17` (runtime handling); environment setup is not committed.

**3. [Rule 1 - Bug] Bound milestone evidence to the actual Rust host and complete identity schema**

- **Found during:** Overall `verify:milestone-flow`
- **Issue:** The script preferred stale MSVC evidence on a GNU-LLVM host and still checked Task 1's interim `binarySha256` field after Task 2 renamed it to `packagedBinarySha256` and added version equality.
- **Fix:** Select the exact evidence tier from `rustc -vV`, require its environment host to match, and validate packaged SHA-256 plus direct/installed version equality.
- **Files modified:** `scripts/release/verify-milestone-flow.mjs`, `fixtures/compat/source-authority.v1.json`
- **Verification:** All 23 milestone Rust flow tests pass and the emitted report remains explicitly `windows-x86_64-gnullvm-dev`.
- **Committed in:** `ce2963f`

---

**Total deviations:** 3 auto-fixed (2 blocking integration/environment, 1 correctness bug)
**Impact on plan:** All fixes strengthen the planned authority boundary; none broaden product scope or claim hosted release evidence.

## Issues Encountered

- The exact strict `compat_report` target has 19 passing cases and the known stale `hosted_cutover_evidence_matches_current_product` mismatch because Phase 10 intentionally changes tracked product inputs. Candidate verification and all other compatibility cases pass. No hosted fixture was forged or refreshed locally; Phase 14 retains final hosted closure.
- The initial GNU-LLVM build attempted its absent default clang driver. Explicitly selecting the bundled `rust-lld` resolved local development compilation without changing repository or release configuration.

## Verification

- `cargo fmt --all -- --check` - passed.
- `cargo clippy -p minimax-cli -p minimax-compat-harness --all-targets --locked -- -D warnings` - passed.
- `cargo test -p minimax-cli --test product_identity --test state_authority --locked` - 6 passed.
- `cargo test -p minimax-compat-harness --test source_authority --locked` - 7 passed.
- `cargo test -p minimax-compat-harness --test compat_report --locked -- --skip hosted_cutover_evidence_matches_current_product` - 19 passed, 1 filtered.
- `cargo run -p minimax-compat-harness --locked -- verify-candidate` - passed.
- Fresh isolated `build:rust:release`, `package:rust`, and `verify:rust-release` - passed with exact identity/hash, offline counters, security, licenses, cold-start, RSS, size, and 10k Wiki p95 gates.
- Two consecutive Task 1 package runs produced byte-identical native and npm archive hashes.
- `npm run verify:milestone-flow` - 23 Rust flow tests passed and emitted host-bound installed identity evidence.

## Known Stubs

None. Stub-pattern matches were language syntax and test fixtures only; no placeholder or unfinished product path was introduced.

## User Setup Required

None - all execution was local/offline and used no credentials, Provider calls, model downloads, publication, or hosted workflow runs.

## Next Phase Readiness

- Phase 10 is complete: source, package, installed-command, writable-state, and CI boundaries now have one Rust authority.
- Phase 11 can replace transitional TypeScript verification/evaluation coverage without ambiguity about release or CI product authority.
- Hosted Windows MSVC/Linux GNU evidence remains intentionally stale until the final stable fingerprint in Phase 14.

## Self-Check: PASSED

- The created product-identity test and canonical summary both exist.
- All six task/deviation commits resolve in git history.
- The final worktree diff has no tracked deletions or whitespace errors.
- Focused identity, state, source-authority, candidate compatibility, release, milestone, formatting, and strict Clippy gates pass within the authorized local evidence tier.

---
*Phase: MMX-10-rust-authority-and-source-boundaries*
*Completed: 2026-07-17*
