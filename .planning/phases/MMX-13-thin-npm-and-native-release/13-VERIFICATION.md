---
phase: MMX-13-thin-npm-and-native-release
verified: 2026-07-18T02:59:00Z
status: passed
score: 8/8 must-haves verified
behavior_unverified: 0
overrides_applied: 0
gaps: []
deferred:
  - truth: "Run the exact final candidate on hosted Windows x64 MSVC and Linux x64 GNU and refresh the final shared product-fingerprint evidence."
    addressed_in: "Phase 14"
    evidence: "Phase 14 success criterion 3 and RCUT-02 require fresh hosted evidence and explicitly reject local GNU-LLVM development evidence."
  - truth: "Delete the now-inert TypeScript/TSX product, tests, and build configuration."
    addressed_in: "Phase 14"
    evidence: "Phase 14 plan 14-01 and RCUT-01 own destructive removal after a fresh authorization checkpoint."
---

# Phase 13: Thin npm and Native Release Verification Report

**Phase Goal:** Users can install through npm or native archives and always run the verified Rust binary through one clear, no-fallback command path.
**Verified:** 2026-07-18T02:59:00Z
**Status:** passed
**Re-verification:** No - initial phase verification after all three plans

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|---|---|---|
| 1 | npm exposes exactly one `minimax-codex` bin and one dependency-free JavaScript distribution launcher with no legacy or TypeScript fallback command. | VERIFIED | Exact metadata/launcher tests, source-authority inventory, and full workspace regression pass; package scripts are locked to fourteen reviewed routes. |
| 2 | Native and npm candidates contain the expected target Rust binary, fixed launcher, docs/licenses, strict metadata, checksums, and no embedding/runtime dependency payload. | VERIFIED | Schema-v2 manifest and deterministic archive tests pass; final native/npm archives bind the same binary and product fingerprint. |
| 3 | Missing, wrong-target, renamed, non-executable, unsafe-type, hash-drifted, fingerprint-drifted, launcher-drifted, extra-executable, corrupt-archive, and invalid-sidecar candidates fail by stable category before installation. | VERIFIED | `npm run test:package` passed 19 tests including all eleven named corruption subtests; alternate-child/fallback markers remained absent. |
| 4 | Release commands require explicit current fingerprint, binary, artifact, and evidence inputs and do not select default or stale target outputs. | VERIFIED | Missing/malformed/stale/mismatched argument tests pass; final flow consumed only `target/phase13-final-d4fa386` fingerprint/artifact/evidence paths. |
| 5 | Native and npm packages are extracted into independent roots and both execute the expected checksummed Rust binary offline. | VERIFIED | Both evidence identities report `minimax-codex-rust 0.1.0`, binary SHA `942ada...ffee`, and capability-output SHA `7aba6c...b3b0`. |
| 6 | Installed smoke cannot read credentials, call a Provider, download a runtime/model, search PATH for another product runtime, or compensate for a failed package with a healthy source binary. | VERIFIED | Both installed identities and aggregate schema-v2 evidence record offline true and Provider/credential/download counts 0; missing/unsafe sibling rejection is recorded. |
| 7 | Release evidence enforces license, security, size, startup, memory, and Wiki retrieval budgets against the exact artifact set. | VERIFIED | 234 licenses checked with 0 invalid; 0 unsafe/database packages; cold p95 82.616 ms, RSS 4,943,872 bytes, archive 3,374,761 bytes, Wiki p95 2.381 ms. |
| 8 | CI blocks candidate evidence upload until Rust quality, evaluators, compatibility/migration, corruption, package, installed, and milestone gates pass, without publication/write authority. | VERIFIED | Source-authority CI contract tests and `verify-candidate` pass; workflow permissions remain `contents: read`, with no publish/tag/push/PR step. |

**Score:** 8/8 truths verified (0 present-but-behavior-unverified)

### Required Artifacts

| Artifact | Expected | Status | Details |
|---|---|---|---|
| `package.json` | One dependency-free npm command and exact local verification aliases | VERIFIED | One `minimax-codex` bin, no runtime/development dependencies, exact `test:package`, and safe pre-package `verify:release`. |
| `bin/minimax-codex.cjs` | Fixed no-search/no-download native launcher | VERIFIED | Stable `E_*` failures, exact sibling resolution, argv forwarding, and no legacy fallback. |
| `fixtures/compat/release/targets.v1.json` | Exact hosted and development target identities | VERIFIED | Windows MSVC, Linux GNU, and development-only Windows GNU-LLVM identities are schema locked; callers cannot relabel a host. |
| `scripts/release/package-contract.mjs` | Deterministic assembly and fail-closed byte/metadata/fingerprint validation | VERIFIED | Exports canonical archive construction, artifact validation, and explicit fingerprint binding used by tests and release scripts. |
| `scripts/release/package-contract.test.mjs` | Positive/reproducibility/corruption/argument contract | VERIFIED | 19 tests pass, including eleven isolated negative categories and explicit fingerprint command cases. |
| `scripts/release/package-rust.mjs` | Explicit deterministic native/npm package assembly | VERIFIED | Requires `--binary`, `--output`, and `--fingerprint-file`; emits the exact five-file candidate set. |
| `scripts/release/verify-rust-release.mjs` | Independent native/npm extraction, execution, and release evidence | VERIFIED | Requires explicit binary/artifacts/evidence roots and records both installed identities, hashes, no-I/O counters, security, license, and performance. |
| `scripts/release/verify-milestone-flow.mjs` | Exact package-evidence composition gate | VERIFIED | Requires explicit artifacts/evidence/fingerprint, binds both installed identities, and passes four cross-phase product flows. |
| `.github/workflows/ci.yml` | Strict read-only release gate order | VERIFIED | Candidate upload follows every required gate; supported hosted matrix remains Ubuntu/Windows with no publish authority. |
| `fixtures/compat/source-authority.v1.json` | Exact JavaScript lifecycle classifications | VERIFIED | Fourteen package scripts plus distribution-only/package-test-only files are hash-pinned and executable fallback constructs fail closed. |

### Key Link Verification

| From | To | Via | Status | Details |
|---|---|---|---|---|
| `package.json` | `bin/minimax-codex.cjs` | Sole npm bin mapping | WIRED | Exact metadata tests reject any second bin, legacy path, dependency, or lifecycle hook. |
| launcher | packaged target binary | Fixed sibling name selected from exact host contract | WIRED | Missing/unsafe sibling cases fail; no PATH search or runtime download exists. |
| package contract tests | package contract helpers | Synthetic archive/manifest/sidecar mutation matrix | WIRED | All eleven corruption categories stop before installed smoke. |
| `package-rust.mjs` | current product fingerprint | Required `--fingerprint-file` and exact recomputation | WIRED | Absent, malformed, stale, and mismatched fingerprints fail. |
| release verifier | native/npm archives | Independent extraction plus exact hash/version/capability checks | WIRED | Both paths bind the same binary and output while preserving separate evidence objects. |
| milestone flow | release evidence | Mandatory installed identities and artifact/fingerprint hashes | WIRED | A passing source command cannot replace either package path. |
| CI workflow | candidate upload | Enforced ordered gates | WIRED | Upload is unreachable after any Rust/evaluator/compat/package/installed failure. |

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
|---|---|---|---|---|
| Product fingerprint | `fingerprint`, `fileCount` | Current tracked product files | Yes, `39bbe8...eaee` / 438 files | FLOWING |
| Release manifest | target/product/binary/launcher/archive records | Exact rustc host, explicit binary, current fingerprint, canonical entry bytes | Yes, strict schema-v2 manifest and sidecars | FLOWING |
| Native installed evidence | version/hash/capability/no-I/O counters | Separately extracted native archive | Yes, exact Rust identity and read-only command | FLOWING |
| npm installed evidence | version/hash/capability/no-I/O counters | Separately extracted npm archive through package launcher | Yes, same Rust identity through npm path | FLOWING |
| Milestone evidence | artifact hashes + both installed identities + cross-phase flows | Final artifacts and release evidence roots | Yes, four product-flow suites and both package identities | FLOWING |

### Behavioral Spot-Checks

Rust commands used the already-installed `1.97.0-x86_64-pc-windows-gnullvm` toolchain with `rust-lld` and `CARGO_NET_OFFLINE=true`. This is local development evidence only, not Windows MSVC, Linux GNU, hosted, or publishable release evidence.

| Behavior | Command / Evidence | Result | Status |
|---|---|---|---|
| Format and strict static checks | `cargo fmt --all -- --check`; workspace all-target Clippy with warnings denied | passed | PASS |
| Full Rust regression | `cargo test --workspace --locked -- --skip hosted_cutover_evidence_matches_current_product` | workspace and doc tests passed; only intentional hosted freshness test skipped | PASS |
| Provider evaluation | Rust Provider evaluator | 20/20 checks passed | PASS |
| Retrieval evaluation | Rust retrieval evaluator | 175 cases; all locked metrics 1.0; disabled-path I/O counts 0 | PASS |
| Package byte/argument/corruption contract | `npm run test:package` | 19 passed including eleven corruption subtests | PASS |
| Current candidate release verification | `target/phase13-final-d4fa386/evidence/windows-x86_64-gnullvm-dev.json` | both installed identities, hashes, budgets, license/security, and no-I/O counters pass | PASS |
| Complete product composition | `target/phase13-final-d4fa386/evidence/milestone-flow-windows-x86_64-gnullvm-dev.json` | four cross-phase suites and both installed paths pass | PASS |
| Repository candidate gate | `cargo run -p minimax-compat-harness --locked -- verify-candidate` | exited 0 | PASS |
| Hosted final fingerprint | exact strict hosted freshness test | intentionally skipped/stale | DEFERRED TO PHASE 14 |

### Probe Execution

No Phase 13 plan or summary declares a `probe-*` script or `<human-check>` block. No subjective or external-service probe is required.

### Requirements Coverage

| Requirement | Source Plan | Status | Evidence |
|---|---|---|---|
| RNPM-01 | 13-01, 13-02, 13-03 | VERIFIED | One npm command, exact native/npm artifacts, and independent offline installed identities bind the same checksummed Rust binary. |
| RNPM-02 | 13-01, 13-02 | VERIFIED | Exact packed metadata and source-authority gates reject legacy bins, `dist/cli.js`, TypeScript/React/Ink/runtime dependencies, and lifecycle fallback. |
| RNPM-03 | 13-02, 13-03 | VERIFIED | Strict checksums/manifests plus eleven corruption categories reject every locked invalid candidate before release. |

All three Phase 13 requirements are claimed by plans and marked complete in `REQUIREMENTS.md`.

### Anti-Patterns Found

None. No fallback/search/download path, default fingerprint, caller-controlled target label, package-evidence bypass, publication authority, or new unsupported platform was found. Stale full-regression assertions were updated without weakening product behavior or thresholds.

### Human Verification Required

None for Phase 13. All package, installed identity, byte-contract, CI-order, and budget requirements have deterministic automated evidence.

### Deferred Items

| Item | Addressed In | Evidence |
|---|---|---|
| Fresh hosted Windows MSVC/Linux GNU execution and one final hosted product fingerprint | Phase 14 / RCUT-02 | Roadmap success criterion 3 rejects local GNU-LLVM development evidence and stale hosted records. |
| TypeScript/TSX source/test/config deletion | Phase 14 / RCUT-01 | Plan 14-01 owns the destructive removal behind a fresh authorization checkpoint. |
| Final Rust-only user and maintainer documentation | Phase 14 / RCUT-03 | Plan 14-03 owns final architecture, install, failure, migration, rollback, and support-window docs. |

### Gaps Summary

No Phase 13 implementation or verification gaps remain. All eight observable truths and RNPM-01/02/03 pass with deterministic local evidence. Hosted target refresh and TypeScript removal remain explicit Phase 14 scope and are not represented as locally satisfied.

---

_Verified: 2026-07-18T02:59:00Z_
_Verifier: Codex acting under the gsd-verifier contract_
