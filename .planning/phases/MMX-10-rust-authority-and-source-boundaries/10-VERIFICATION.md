---
phase: MMX-10-rust-authority-and-source-boundaries
verified: 2026-07-17T14:02:46Z
status: gaps_found
score: 10/12 must-haves verified
behavior_unverified: 0
overrides_applied: 0
gaps:
  - truth: "Every supported and legacy CLI/TUI product path executes Rust, and transitional TypeScript is non-executable."
    status: failed
    reason: "package.json still exposes `npm run dev -> tsx src/cli.tsx`; that file is a live Ink TUI entry that calls runCliMain when executed. The Rust authority gates pass with this route present because package-script validation rejects only dist/cli.js."
    artifacts:
      - path: "package.json"
        issue: "The dev script directly executes the transitional TypeScript product entry."
      - path: "src/cli.tsx"
        issue: "The file renders App and invokes runCliMain when it is the process entrypoint."
      - path: "crates/compat-harness/src/baseline.rs"
        issue: "validate_product_entry checks the sole bin, start:legacy, and dist/cli.js, but does not reject scripts that execute src/cli.tsx."
      - path: "crates/compat-harness/src/source_authority.rs"
        issue: "The repository gate inventories and hashes TypeScript but does not prove classified TypeScript is non-executable from package scripts."
    missing:
      - "Remove or disable every package script that starts the TypeScript product entry, while retaining only the explicitly transitional test/evaluation commands assigned to later phases."
      - "Add a Rust-owned negative package/source-authority test that fails for any script executing src/cli.tsx or another TS/TSX product entry."
  - truth: "No supported or legacy command can create or mutate `.mini-codex` after the authority cutover."
    status: failed
    reason: "The escaped TypeScript dev command initializes the legacy ApplicationKernel, whose default state root is join(cwd, `.mini-codex`); current state-authority tests exercise Rust routes and migration but never the remaining legacy command."
    artifacts:
      - path: "src/runtime/application-kernel.ts"
        issue: "The live TypeScript runtime defaults stateRoot to `.mini-codex`."
      - path: "crates/cli/tests/state_authority.rs"
        issue: "The tests prove Rust state and migration invariants but do not guard against a package command starting the legacy writer."
    missing:
      - "Close the live TypeScript command route and add a regression that the supported package/script surface cannot start a `.mini-codex` writer."
deferred:
  - truth: "Replace transitional TypeScript behavioral, Provider, and retrieval verification and disposition each legacy diagnostic fixture."
    addressed_in: "Phase 11"
    evidence: "Phase 11 success criteria require deterministic Rust coverage/evaluations and explicit retirement decisions before TypeScript-covered source is removed."
  - truth: "Rebase compatibility and migration evidence on immutable fixtures without executing TypeScript."
    addressed_in: "Phase 12"
    evidence: "Phase 12 success criteria require fixture-only compatibility reports and complete source-preserving Rust migration coverage."
  - truth: "Remove TypeScript-only npm metadata/dependencies and prove normal npm/npx installation paths."
    addressed_in: "Phase 13"
    evidence: "Phase 13 success criteria remove TypeScript compiler/runtime and React/Ink dependencies from the packed package and add offline installed-package corruption checks."
  - truth: "Delete the transitional TypeScript tree and refresh final hosted Windows MSVC/Linux GNU evidence for one final fingerprint."
    addressed_in: "Phase 14"
    evidence: "Phase 14 success criteria explicitly own TypeScript removal and final hosted target/fingerprint closure; development-only GNU-LLVM evidence cannot satisfy that gate."
---

# Phase 10: Rust Authority and Source Boundaries Verification Report

**Phase Goal:** Users and maintainers have one executable product and writable runtime authority in Rust, while any JavaScript that remains is visibly limited to distribution orchestration.
**Verified:** 2026-07-17T14:02:46Z
**Status:** gaps_found
**Re-verification:** No - initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|---|---|---|
| 1 | Every supported CLI/TUI, Provider, session, tool, Vault/Wiki, retrieval, capability, migration, and compatibility product path executes Rust; current Rust/public behavior is authoritative and transitional TypeScript is not executable. | FAILED | `package.json:20` maps `dev` to `tsx src/cli.tsx`; `src/cli.tsx:33,67` renders the live Ink application and invokes `runCliMain`. The current Rust gates still pass. |
| 2 | The repository reports the complete JavaScript allowlist and rejects product imports, domain behavior, runtime download, fallback, and unknown JS paths. | VERIFIED | Manifest has exactly five reviewed JS entries plus three separately governed fixtures; 8/8 tracked JS paths and all hashes match. The focused negative/source inventory suite passed 7/7. |
| 3 | Runtime commands write only Rust-owned `.minimax`; no supported or legacy command can create or mutate `.mini-codex`. | FAILED | Rust paths and migration pass 3/3 state-authority tests, but the remaining TS dev entry reaches `src/runtime/application-kernel.ts:609`, which defaults to writable `.mini-codex`. |
| 4 | The Rust CLI and current npm-installed command remain usable after the authority boundary. | VERIFIED | Fresh development-only candidate verification ran the extracted launcher and direct binary: both reported `minimax-codex-rust 0.1.0`, the exact binary SHA-256 matched, capability status smoke passed, and missing/unsafe siblings were rejected. |
| 5 | A checked-in strict manifest classifies Rust roots, executable/package entries, allowed JS, transitional TS, legacy JS fixtures, immutable fixtures, supported targets, and state roots. | VERIFIED | `source-authority.v1.json` parses strictly with 9 Rust roots, 1 executable entry, 5 JS authorities, 191 TS entries, 3 legacy fixtures, 7 immutable roots, 2 hosted targets, and exactly one writable/migration root. |
| 6 | Transitional TS is an exact hash-pinned shrinking inventory, and new TS/TSX or JS paths fail closed. | VERIFIED | Independent comparison found 191 tracked TS/TSX = 191 manifest entries, no missing/extra paths, and zero hash drift; negative tests reject unclassified TS and JS. Its claimed non-executable status is accounted for separately in failed truth 1. |
| 7 | The JavaScript allowlist is exact and limited to launcher/release/package/fingerprint orchestration. | VERIFIED | Validator constants and manifest agree on the five exact paths; all five are hash-pinned and the actual sources contain orchestration only. |
| 8 | The three diagnostic JS fixtures are hash-pinned outside executable/JS authority and carry Phase 11 disposition plus Phase 14 zeroing metadata. | VERIFIED | Exactly `diag-large.js`, `diag-ok.js`, and `diag-slow.js` appear only in `transitionalLegacyTestFixtures`; their hashes match and smuggling tests fail closed. |
| 9 | A missing, unsafe, non-executable, unsupported, failed, or signaled Rust binary produces actionable non-zero failure with no legacy fallback. | VERIFIED | `bin/minimax-codex.cjs` uses one fixed sibling map, `lstatSync`, shell-free `spawnSync`, and reinstall guidance; Rust and transitional launcher tests pass. |
| 10 | Native and npm candidates contain one executable Rust product path and no packaged legacy application entry. | VERIFIED | A fresh candidate was assembled and both tar listings contain only the launcher plus `minimax-codex.exe` as product executables; full release verification rejected extra entries and passed. |
| 11 | Direct Rust and extracted npm launcher smokes report the same identity and execute the exact manifest-bound binary. | VERIFIED | Fresh `verify-rust-release.mjs` output recorded equal direct/installed versions, matching packaged binary SHA-256, isolated environment, no credentials/PATH lookup, and passing capability smoke. |
| 12 | CI runs Rust source-authority/contracts before transitional Node checks and packaging, keeps Node non-authoritative, and cannot publish or expand permissions/platforms. | VERIFIED | `.github/workflows/ci.yml` is read-only, Windows/Linux only, Rust-contracts-first, and contains no TS product build/publish/credentials; workflow mutation coverage is included in the 7/7 source-authority suite and the synchronized TS suite passed. |

**Score:** 10/12 truths verified (0 present-but-behavior-unverified)

### Deferred Items

| # | Item | Addressed In | Evidence |
|---|---|---|---|
| 1 | Rust-owned replacement of transitional TypeScript tests/evaluations and legacy-fixture disposition | Phase 11 | Roadmap 11 SC 1-4 |
| 2 | Fixture-only compatibility and complete Rust migration support-window evidence | Phase 12 | Roadmap 12 SC 1-4 |
| 3 | Thin normal npm/npx metadata, dependency, and corruption closure | Phase 13 | Roadmap 13 SC 1-4 |
| 4 | TypeScript deletion and final hosted Windows MSVC/Linux GNU fingerprint evidence | Phase 14 | Roadmap 14 SC 1-4 |

The stale hosted fingerprint is intentionally not a Phase 10 failure. No hosted workflow, network/API call, package publication, push, model download, or hosted-evidence refresh was performed. The local `windows-x86_64-gnullvm-dev` run below is development-only and cannot satisfy Phase 14.

### Required Artifacts

| Artifact | Expected | Status | Details |
|---|---|---|---|
| `fixtures/compat/source-authority.v1.json` | Complete, strict ownership/state manifest | VERIFIED | Exact inventories, hashes, targets, and lifecycle metadata independently matched. |
| `crates/compat-harness/src/source_authority.rs` | Deterministic fail-closed source/CI validator | PARTIAL | Substantive and wired, but package-script execution of classified TS is outside its repository checks. |
| `crates/compat-harness/tests/source_authority.rs` | Positive inventory and adversarial negatives | PARTIAL | 7/7 pass; no negative covers a package script launching `src/cli.tsx`. |
| `package.json` | Sole Rust product command | PARTIAL | Sole `bin` is correct, but `dev` remains a direct live TS product route. |
| `bin/minimax-codex.cjs` | Fixed, no-fallback Rust launcher | VERIFIED | Safe sibling checks, shell-free argv forwarding, child status preservation, stable failures. |
| `crates/cli/tests/state_authority.rs` | Black-box `.minimax`/migration invariants | PARTIAL | Strong Rust/migration filesystem proof, but it does not cover the escaped legacy command. |
| `crates/cli/tests/product_identity.rs` | Direct/installed identity contract | VERIFIED | 3/3 pass; release verification supplies the runtime installed smoke. |
| `scripts/release/package-rust.mjs` | Single-product deterministic candidate assembly | VERIFIED | Fresh native/npm archives contain no `dist` payload or second product entry. |
| `scripts/release/verify-rust-release.mjs` | Archive/hash/installed identity verification | VERIFIED | Fresh development-only run passed identity, archive, security, license, size, and performance checks. |
| `.github/workflows/ci.yml` | Rust-first, read-only authority gates | VERIFIED | Mandatory contracts precede TS checks and package/smoke; permissions remain `contents: read`. |

### Key Link Verification

| From | To | Via | Status | Details |
|---|---|---|---|---|
| `crates/compat-harness/src/main.rs` | `source_authority.rs` | shared `verify_repository` preflight | WIRED | `load_source_authority` and `validate_source_authority` run before compatibility loading for both modes. |
| `source_authority.rs` | `source-authority.v1.json` | strict serde, inventory, hash, package, JS, and CI checks | WIRED | 7/7 focused tests and verify-candidate pass. |
| `source-authority.v1.json` | `typescript-responsibilities.v1.json` | legacy-fixture disposition | DEFERRED | Target file does not yet exist; Phase 11 explicitly owns its creation/disposition. |
| `package.json` | `bin/minimax-codex.cjs` | sole npm bin | WIRED | Exactly one `minimax-codex` bin points to the fixed launcher. |
| `state_authority.rs` | `crates/cli/src/migration.rs` | inventory/dry-run/apply/rollback hashes | WIRED | Migration source preservation and receipt-scoped rollback pass. |
| `verify-rust-release.mjs` | `bin/minimax-codex.cjs` | extracted launcher identity and sibling failures | WIRED | Fresh installed smoke passed with exact version/hash binding. |
| `.github/workflows/ci.yml` | Rust source authority | `verify:rust-contracts` strict/candidate commands | WIRED | Both matrix branches run contracts before packaging. |

### Data-Flow Trace (Level 4)

Not applicable: Phase 10 artifacts are manifests, validators, launch/package scripts, filesystem tests, and CI configuration rather than dynamic UI/data-rendering artifacts.

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|---|---|---|---|
| Source/CI authority positives and adversarial negatives | `cargo +1.97.0-x86_64-pc-windows-gnullvm test -p minimax-compat-harness --test source_authority --locked` with `rust-lld` | 7 passed | PASS |
| Rust writable-root and migration invariants | `cargo +1.97.0-x86_64-pc-windows-gnullvm test -p minimax-cli --test state_authority --locked` with `rust-lld` | 3 passed | PASS |
| Rust product identity contract | `cargo +1.97.0-x86_64-pc-windows-gnullvm test -p minimax-cli --test product_identity --locked` with `rust-lld` | 3 passed | PASS |
| Candidate preflight | `cargo +1.97.0-x86_64-pc-windows-gnullvm run -p minimax-compat-harness --locked -- verify-candidate` | exit 0 | PASS |
| Full transitional suite (only full suite run) | `npm test` | 440 passed, 0 failed | PASS |
| Candidate package and installed identity | `package-rust.mjs` plus `verify-rust-release.mjs` against fresh verifier artifacts | exact single-product archives and installed/direct identity passed; support tier `development_only` | PASS |
| Escaped TS product entry | Parse package script and live CLI source | `{"dev":"tsx src/cli.tsx","liveEntrypoint":true,"rendersInk":true}` while verify-candidate still exits 0 | FAIL |

The first development release-verifier attempt lacked the GNU-LLVM runtime DLL on its process search path and failed before cold-start measurement. Re-running with the installed toolchain runtime directory prepended succeeded. This is local environment handling, not Windows MSVC/Linux GNU release evidence.

### Probe Execution

No `probe-*.sh` files or phase-declared probes exist. The declared runnable candidate/package checks are recorded under Behavioral Spot-Checks.

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|---|---|---|---|---|
| RUST-01 | 10-02, 10-03 | Rust is the only executable product implementation | BLOCKED | Sole bin/candidate are Rust, but `npm run dev` still starts the live TS CLI. |
| RUST-02 | 10-01, 10-03 | JavaScript is restricted to reviewed distribution orchestration | SATISFIED | Exact 5-file allowlist, 8/8 total JS inventory, zero drift, source/CI negatives pass. |
| RUST-03 | 10-02, 10-03 | `.minimax` is the only writable runtime authority | BLOCKED | Rust and migration tests pass, but the remaining TS CLI defaults to writable `.mini-codex`. |

No Phase 10 requirement is orphaned: all three IDs appear in PLAN frontmatter and REQUIREMENTS traceability.

### Prohibition Verification

| Prohibition | Status | Evidence |
|---|---|---|
| Do not delete transitional TS in Phase 10 | VERIFIED | 191 tracked TS/TSX paths remain hash-pinned; Phase 10 diff contains no TS deletion. |
| Do not allow JS product behavior/fallback/download | VERIFIED | Exact reviewed sources plus negative validator tests. |
| Do not put diagnostic fixtures in executable/JS authority | VERIFIED | Separate three-entry class and smuggling rejection. |
| Do not expand platform/provider/tool/installer scope | VERIFIED | Supported targets remain Windows x64 MSVC and Linux x64 GNU; no feature expansion found. |
| Do not keep a runnable supported legacy CLI/fallback | FAILED | `npm run dev` starts `src/cli.tsx`. |
| Do not write `.mini-codex` after cutover | FAILED | That live TS runtime defaults its writable state root to `.mini-codex`. |
| Do not package compiled TS as a release entry | VERIFIED | Fresh native/npm archives contain no `dist` or legacy executable. |
| Do not call Provider/publish/push/open PR | VERIFIED | Verification stayed offline; CI has read-only permissions and no remote mutation commands. |

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|---|---|---|---|---|
| `package.json` | 20 | Live TypeScript product entry remains executable | BLOCKER | Breaks sole executable authority. |
| `src/runtime/application-kernel.ts` | 609 | Escaped legacy entry defaults to `.mini-codex` writer | BLOCKER | Breaks sole writable authority. |
| `baseline.rs` | 720-723 | Package-script guard recognizes only `dist/cli.js` | WARNING | Lets `tsx src/cli.tsx` pass the Rust-owned product-entry gate. |

No unreferenced `TBD`, `FIXME`, or `XXX` debt markers were found in Phase 10 implementation files. Empty-array matches in `test/ci-contract.ts` are ordinary parser error returns, not stubs.

### Human Verification Required

None. The two failures are directly observable in package/source wiring; remaining behavior-dependent truths have passing automated evidence.

### Gaps Summary

Phase 10 does not yet achieve its stated executable and writable authority cutover. The same escaped route causes both failures: `npm run dev` directly starts the live TypeScript TUI, and that runtime defaults to `.mini-codex`. The sole npm bin, launcher, candidate archives, JavaScript boundary, Rust state tests, direct/installed identity, CI ordering, and synchronized 440-test suite are otherwise verified.

The later phases remain correctly scoped for Rust verification replacement (11), fixture compatibility/migration (12), thin package/dependency closure (13), and source deletion plus hosted fingerprint closure (14). Those deferred tasks do not excuse the Phase 10 requirement that the legacy product be non-executable before deletion.

---

_Verified: 2026-07-17T14:02:46Z_
_Verifier: the agent (gsd-verifier)_
