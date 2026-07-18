---
phase: MMX-14-typescript-removal-and-hosted-closure
verified: 2026-07-18T09:50:08Z
status: passed
score: 10/10 must-haves verified
behavior_unverified: 0
overrides_applied: 0
gaps: []
deferred: []
---

# Phase 14: TypeScript Removal and Hosted Closure Verification Report

**Phase Goal:** The repository and release evidence describe one Rust-only product after the replaced TypeScript implementation is deleted.
**Verified:** 2026-07-18T09:50:08Z
**Status:** passed
**Re-verification:** No - initial phase verification after all three plans

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|---|---|---|
| 1 | No TypeScript/TSX product or test source, compiler configuration, generated `dist`, legacy command, or executable fallback remains. | VERIFIED | Source-authority 19/19 and full strict compatibility verification pass; RCUT-01 was completed by 14-01 and made permanent by 14-02. |
| 2 | CI and local verification reject reintroduced TypeScript, dependencies, fallback launchers, extra writable roots, or missing retained migration fixtures. | VERIFIED | Full workspace tests, source-authority negatives, package 19/19, and strict Rust contracts pass. |
| 3 | Rust is the sole product, verification, evaluation, compatibility, migration, and installed-package authority. | VERIFIED | Full workspace/doc tests and `npm run verify:rust-contracts` pass without a TypeScript runtime. |
| 4 | User and maintainer documentation covers Rust-only architecture, npm/native installation, Windows/Linux support, actionable failures, migration/rollback, and the two-release window. | VERIFIED | README and both release guides were finalized before fingerprint freeze and are included in the current product identity. |
| 5 | One final fingerprint identifies the exact post-documentation product tree. | VERIFIED | Rust/MJS fingerprint v3 is `513c7565593b3e3088131d2854709be4773f0a81c2445c146f4a5acb597d29b6` across 235 files. |
| 6 | Hosted Windows x64 MSVC and Linux x64 GNU candidate jobs pass the complete required release chain. | VERIFIED | Candidate run `29638773706`; Windows job `88065594381`, Linux job `88065594400`, both successful. |
| 7 | An ordinary strict push run validates the remotely visible pending candidate record before final closure. | VERIFIED | Strict run `29639243817`; Windows job `88066830361`, Linux job `88066830338`, both successful. |
| 8 | Candidate and strict artifacts are exactly reproducible per platform. | VERIFIED | Windows and Linux binary/native/npm hashes, compressed sizes, and capability-output hashes are pairwise identical. |
| 9 | Local GNU-LLVM evidence cannot substitute for hosted MSVC, and all accepted evidence is offline with zero external-effect counters. | VERIFIED | Exact rustc hosts/support tiers are schema checked; offline true and Provider/credential/model-download counters are 0 in all four hosted evidence files. |
| 10 | Linux proves its real Bubblewrap namespace preflight and adversarial sandbox canary while the evidence workflow has no publication authority. | VERIFIED | Both candidate and strict Linux jobs passed the install/preflight/canary steps; workflow permissions remain read-only. |

**Score:** 10/10 truths verified (0 present-but-behavior-unverified)

### Required Artifacts

| Artifact | Expected | Status | Details |
|---|---|---|---|
| `README.md` | Rust-only architecture and supported install surface | VERIFIED | No dual-runtime or legacy command remains; npm/native/no-fallback facts match package and launcher contracts. |
| `docs/release/install-upgrade-rollback.md` | Supported target installation, upgrade, rollback, and failures | VERIFIED | Windows MSVC/Linux GNU are the release targets; rollback is checksum- and identity-bound. |
| `docs/release/cutover.md` | Candidate/pending/strict process and two-release migration window | VERIFIED | The documented order matches the enforced hosted record lifecycle. |
| `.github/workflows/ci.yml` | Read-only candidate/strict matrix with reproducible Windows linking | VERIFIED | Windows gets `/Brepro`; Linux keeps sandbox preflight/canary; evidence upload is after all gates. |
| `fixtures/compat/release/hosted-gates.v1.json` | Final combined candidate and strict evidence | VERIFIED | Schema v2, `strictStatus: passed`, exact run/job URLs, targets, hashes, counters, and thresholds. |
| `target/release-evidence/final-v3/fingerprint.json` | Frozen current product identity | VERIFIED | `513c7565...d29b6`, 235 files. |
| `target/release-evidence/final-v3/intake.json` | Exact local binary/artifact/evidence binding | VERIFIED | Six bound files match recorded SHA-256/size; intake SHA-256 `dbbaaa1f...3840`. |
| `target/release-evidence/hosted-candidate/` | Genuine downloaded candidate evidence | VERIFIED | Four schema-v2/v3 Windows/Linux files from run `29638773706`. |
| `target/release-evidence/hosted-strict/` | Genuine downloaded strict evidence | VERIFIED | Four schema-v2/v3 Windows/Linux files from run `29639243817`. |

### Key Link Verification

| From | To | Via | Status | Details |
|---|---|---|---|---|
| hosted record | current product tree | fingerprint/file count | WIRED | Record and current Rust/MJS computation match exactly. |
| candidate run | strict run | pending record committed before ordinary push | WIRED | Candidate `29638773706` precedes strict `29639243817`; run events are workflow_dispatch then push. |
| Windows candidate | Windows strict | `/Brepro` artifact identity | WIRED | Binary `8cfb7ffd...a2bf`, native `d71c337a...389c`, npm `9dd6b7bd...7523` match. |
| Linux candidate | Linux strict | deterministic GNU artifact identity | WIRED | Binary `e380053f...9137`, native `eeebb397...e062`, npm `13e504ce...5c4d` match. |
| final-v3 intake | local frozen artifacts/evidence | SHA-256 and size | WIRED | All six referenced files pass the final binding audit. |
| workflow | release authorization boundary | read-only permissions and no publish step | WIRED | No npm publish, tag, PR, merge, or migration action exists in the evidence workflow. |

### Data-Flow Trace

| Artifact | Data Variable | Source | Produces Real Data | Status |
|---|---|---|---|---|
| Product fingerprint | `productFingerprint`, `productFileCount` | Current 235-file Rust-only working tree | Yes | FLOWING |
| Candidate hosted run | run/job/target/evidence identity | Manual `workflow_dispatch` at remote tree `01b1193b...` | Yes | FLOWING |
| Pending record | candidate evidence and `strictStatus: pending` | Validated candidate downloads | Yes | FLOWING |
| Strict hosted run | run/job/target/evidence identity | Ordinary push at remote tree `fcfd3410...` | Yes | FLOWING |
| Final record | candidate plus strict composition | Validated downloads and exact artifact comparison | Yes | FLOWING |
| Local closure | binary/packages/evidence/intake | Unchanged `target/release-evidence/final-v3/` artifact root | Yes | FLOWING |

## Requirement Coverage

| Requirement | Status | Evidence |
|---|---|---|
| RCUT-01 | SATISFIED | TypeScript deletion and permanent reintroduction gates from 14-01/14-02 remain green in full final workspace tests. |
| RCUT-02 | SATISFIED | Hosted candidate/strict Windows MSVC and Linux GNU evidence passes and is reproducible against one final fingerprint. |
| RCUT-03 | SATISFIED | Rust-only README/install/cutover documentation is finalized and fingerprint-bound. |

## Verification Commands and Results

- `cargo test --workspace --locked` with the final combined hosted record - passed, including 32/32 compatibility tests and all doc tests.
- `cargo run -p minimax-compat-harness --locked -- verify` - passed.
- `npm run verify:rust-contracts` - passed.
- `npm run verify:rust-release -- --binary target/release-evidence/final-v3/cargo/release/minimax-cli.exe --artifacts target/release-evidence/final-v3/artifacts --evidence-dir target/release-evidence/final-v3/evidence` - passed.
- `npm run verify:milestone-flow -- --artifacts target/release-evidence/final-v3/artifacts --evidence-dir target/release-evidence/final-v3/evidence --fingerprint-file target/release-evidence/final-v3/fingerprint.json` - passed.
- Final intake fingerprint and SHA-256/size binding audit - passed for six files.
- Candidate/strict per-platform binary/native/npm/size/capability equality audit - passed.

## Non-Blocking Follow-up

- GitHub warns that `actions/checkout@v4` and `actions/setup-node@v4` target the deprecated Node 20 action runtime. GitHub forced Node 24 and every gate passed; upgrade when the upstream action versions are adopted.
- npm publication, tag, PR, merge, and milestone archival were not part of this closure and remain separate user-directed actions.

## Conclusion

Phase 14 achieves its goal with no gaps: the repository is Rust-only, documentation is current, local evidence remains honestly development-only, and genuine reproducible hosted Windows/Linux candidate plus strict evidence closes the final fingerprint.

---
*Phase: MMX-14-typescript-removal-and-hosted-closure*
*Verified: 2026-07-18*
