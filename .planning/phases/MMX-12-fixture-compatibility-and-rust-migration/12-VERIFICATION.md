---
phase: MMX-12-fixture-compatibility-and-rust-migration
verified: 2026-07-18T01:14:50Z
status: passed
score: 7/7 must-haves verified
behavior_unverified: 0
overrides_applied: 0
gaps: []
deferred:
  - truth: "Refresh the stale hosted Windows MSVC/Linux GNU product fingerprint and hosted release evidence."
    addressed_in: "Phase 14"
    evidence: "Phase 14 success criterion 3 requires final hosted Windows x64 MSVC and Linux x64 GNU evidence bound to one final product fingerprint and rejects stale/local evidence."
  - truth: "Delete the inert TypeScript/TSX product and test source after package/release closure."
    addressed_in: "Phase 14"
    evidence: "Phase 14 success criterion 1 and plan 14-01 own deletion; Phase 12 only proves fixture-owned compatibility and preserves TypeScript-v1 migration data."
---

# Phase 12: Fixture Compatibility and Rust Migration Verification Report

**Phase Goal:** Existing users can verify compatibility and migrate TypeScript-era durable data through Rust without keeping the TypeScript runtime executable.
**Verified:** 2026-07-18T01:14:50Z
**Status:** passed
**Re-verification:** Yes - Plans 12-03 and 12-04 closed every deterministic gap from the initial report

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|---|---|---|
| 1 | Compatibility compares the current Rust product with an immutable versioned public-contract fixture and explicit approved differences. | VERIFIED | `public-contract.v1.json` has 34 sorted unique `contract.*` identities, a strict provenance/fingerprint, exact command/profile/protocol closure, and five linked difference IDs. Manifest tamper/schema/evidence tests passed. |
| 2 | The report is deterministic, complete, machine-readable, and contains contract/Rust evidence rather than live TypeScript product rows. | VERIFIED | Golden/repeat/completeness tests passed; the independent report command emitted 34 unique entries with the exact `sha256:91d2...cec5` contract fingerprint and no live `typescript.*` row. |
| 3 | The current fixture compatibility path runs successfully without a TypeScript product/test/generated tree. | VERIFIED | `compatibility_report_and_verify_are_hermetic_without_typescript_runtime` passed from a copy lacking top-level `src/`, `dist/`, and `test/`; `verify-candidate` exited 0. |
| 4 | The no-TypeScript dependency gate fail-closes over every module compiled into/reached by the compatibility executable. | VERIFIED | Compatibility derives all eleven Rust modules from `lib.rs`/`main.rs`, requires exact recursive inventory equality, and the omitted-module executable-edge mutation passed inside the 23-test compatibility suite. |
| 5 | Rust migration inventory, dry-run, apply, verify, idempotency, collision, interruption recovery, and narrow rollback work from immutable temporary fixture copies while source bytes remain unchanged. | VERIFIED | All 17 migration integration tests passed, including deterministic dry-run, bounds, secrets/private reasoning, drift, collision, replay, recovery, exact-byte rollback, forged ownership, and target-ancestor symlink cases. |
| 6 | Adversarial ownership and target-path boundaries fail closed for recomputed receipts, forged operations, and symlinked paths. | VERIFIED | Fixed durable plan/operation/receipt evidence must agree on pre-write disposition and content; all three allowlisted forgery cases preserve existing bytes, and both target-ancestor symlink cases prove no external write. |
| 7 | TypeScript-v1 migration evidence is exact/fingerprinted and cannot be marked removal-eligible before two distinct ordered public releases after v3.0.0. | VERIFIED | Eight evidence files exactly match path/length/SHA-256/policy rows; support metadata records zero subsequent releases and `removalEligible: false`; all four manifest/window positive and negative tests passed. |

**Score:** 7/7 truths verified (0 present-but-behavior-unverified)

### Required Artifacts

| Artifact | Expected | Status | Details |
|---|---|---|---|
| `fixtures/compat/public-contract.v1.json` | Immutable public-contract identities/provenance/evidence/difference links | VERIFIED | 34 required IDs equal 34 items; strict schema, provenance `84784f5`, product entry, evidence existence, and fingerprint checks are substantive. |
| `crates/compat-harness/src/manifest.rs` | Strict manifest loading and exact contract closure | VERIFIED | Rejects unknown fields, schema/provenance/fingerprint drift, duplicate/lost IDs, pending/empty evidence, missing evidence paths, and invalid difference links. |
| `fixtures/compat/report.expected.json` | Byte-stable contract-versus-Rust golden | VERIFIED | Exact report test and independent report command agree. |
| `crates/compat-harness/src/report.rs` | Deterministic report plus complete no-TypeScript boundary | VERIFIED | Standard-root module derivation and exact recursive source-inventory equality cover every compiled compatibility module. |
| `fixtures/compat/migration/typescript-v1/manifest.v1.json` | Exact recursive fixture inventory and policy | VERIFIED | Eight non-metadata files are sorted and bound by length/hash/role/disposition; validator recomputes all values. |
| `fixtures/compat/migration/typescript-v1/support-window.v1.json` | Machine-checkable two-release retention record | VERIFIED | Cutover `3.0.0`, minimum 2, observed 0, eligibility false; strict ordered SemVer validator is wired. |
| `crates/compat-harness/src/migration_support.rs` | Fixture fingerprint and support-window gate | VERIFIED | Substantive, exported, and called by repository verification before compatibility/release evidence. |
| `crates/cli/src/migration.rs` | Source-preserving Rust migration with fail-closed ownership/path boundaries | VERIFIED | Pre-write ownership is persisted and cross-bound across three fixed records; every target-side ancestor is no-symlink and canonically contained before mutation. |
| `crates/cli/tests/migration.rs` | Complete hostile/lifecycle integration evidence | VERIFIED | 17 tests pass, including all allowlisted forged-created claims and top-level/nested target-ancestor symlink no-external-write cases. |

### Key Link Verification

| From | To | Via | Status | Details |
|---|---|---|---|---|
| `manifest.rs` | `public-contract.v1.json` | Strict deserialize, expected-ID derivation, evidence existence, and fingerprint recomputation | WIRED | Exact fixture mutations fail closed. |
| `report.rs` | public contract + command differences | `build_report` exact join and `validate_report` rebuild equality | WIRED | Every contract item appears once; differences exactly match links and locked outcomes. |
| `main.rs` | migration fixture/support gate | `verify_repository` calls both validators before compatibility checks | WIRED | `verify-candidate` exercises the link. |
| no-TypeScript source boundary | complete compat executable module closure | standard-root module derivation + exact recursive inventory | WIRED | All eleven compiled sources are covered and omitted/orphaned/executable legacy edges fail closed. |
| `MigrationReceipt` | authoritative created/reused ownership | fixed durable plan/operation/receipt equality | WIRED | Migration identity, plan hash, target set, content, and pre-write disposition must agree before deletion authority exists. |
| migration target paths | canonical target root | component-wise no-symlink resolution + canonical containment | WIRED | Existing and newly created parents are revalidated before lock, create, open, write, verify, recover, and remove operations. |

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
|---|---|---|---|---|
| Compatibility report | `entries` | strict public contract + exact approved-difference fixture + live Rust evidence paths | Yes, 34 deterministic entries | FLOWING |
| Migration plan/artifacts | `included`, `excluded`, `targets`, staged bytes | bounded recursive source scan and Rust normalization | Yes, temp-fixture config/session/capability outputs | FLOWING |
| Migration receipt/rollback | `created` and `reused` | durable pre-write plan disposition cross-checked with operation and receipt | Yes, exact created/reused ownership and narrow rollback | FLOWING |
| Support status | observed releases / computed eligibility | support-window fixture bound to migration fixture fingerprint | Yes, zero releases and false eligibility | FLOWING |

### Behavioral Spot-Checks

All Rust commands used the already-installed `1.97.0-x86_64-pc-windows-gnullvm` toolchain with `rust-lld`, `CARGO_NET_OFFLINE=true`, and an isolated target directory. This is local development evidence only, not Windows MSVC, Linux GNU, hosted, or release evidence.

| Behavior | Command | Result | Status |
|---|---|---|---|
| Fixture-owned compatibility, golden, no-TypeScript hermetic path, complete module closure, architecture negatives | `cargo test -p minimax-compat-harness --test compat_report --test migration_support --locked -- --skip hosted_cutover_evidence_matches_current_product` | 23 compat tests passed (1 hosted filtered); migration support 4/4 passed | PASS |
| Exact migration manifest/support-window validation | `cargo test -p minimax-compat-harness --test migration_support --locked` | 4 passed | PASS |
| Migration lifecycle and hostile inputs | `cargo test -p minimax-cli --test migration --locked` | 17 passed, including forged allowlisted ownership, target symlink containment, and the 51.9 s bounds case | PASS |
| Deterministic machine-readable report | `cargo run -p minimax-compat-harness --locked -- report --format json` | exited 0; 34 exact entries/fingerprint | PASS |
| Repository candidate gate | `cargo run -p minimax-compat-harness --locked -- verify-candidate` | exited 0 | PASS |
| Formatting and diff hygiene | `cargo fmt --all -- --check`; `git diff --check` | both exited 0 before this report was added | PASS |
| Transitional full regression | `npm test` | 434 passed, 0 failed | PASS |
| Hosted product fingerprint | exact `hosted_cutover_evidence_matches_current_product` test | expected `CutoverEvidence` failure | DEFERRED TO PHASE 14 |

The two Plan 12-04 attack classes were executed only through committed Rust integration tests over temporary fixture copies: recomputed forged records for all three allowlisted targets and top-level/nested target-ancestor symlinks. Every case preserved source and external-directory bytes and performed no live Provider, credential, network, or real-data operation.

### Probe Execution

No Phase 12 plan or summary declares a `probe-*` script or `<human-check>` block. Step 7c is not applicable.

### Requirements Coverage

| Requirement | Source Plan | Status | Evidence |
|---|---|---|---|
| RCMP-01 | 12-01, 12-03 | VERIFIED | Immutable fixture, deterministic report, hermetic Rust-only behavior, complete derived module closure, and executable legacy-edge negatives pass. |
| RCMP-02 | 12-02, 12-04 | VERIFIED | Positive lifecycle, source preservation, exact fixture/release gates, durable created/reused provenance, forged-record rejection, and target symlink containment pass. |
| RCMP-03 | none | NOT DEFINED | No RCMP-03 exists in `REQUIREMENTS.md`, `ROADMAP.md`, Phase 12 SPEC, or either plan. Phase 12's declared contract is RCMP-01 and RCMP-02 only, so this is a scope note rather than an orphaned implementation requirement. |

Both requirements declared by Phase 12 are claimed by a plan; there is no orphaned defined requirement.

### Anti-Patterns Found

None. The former fixed source allowlist, self-declared deletion authority, path-allowlist ownership shortcut, and lexical-only target containment findings were removed and covered by committed negative tests. No unreferenced `TBD`, `FIXME`, or `XXX` debt marker and no implementation placeholder was found in the Phase 12 changed artifacts.

### Human Verification Required

None. The passing behaviors and the blocking gaps are deterministic code/test concerns; no visual, external-service, or subjective verification is needed.

### Deferred Items

| Item | Addressed In | Evidence |
|---|---|---|
| Stale hosted product fingerprint and final Windows MSVC/Linux GNU evidence | Phase 14 | Roadmap success criterion 3 owns fresh hosted evidence for one final fingerprint. The independently run exact hosted test fails only with `CutoverEvidence`; this must not be refreshed locally. |
| TypeScript/TSX product/test source deletion | Phase 14 | Success criterion 1 and plan 14-01 own deletion; static TypeScript-v1 migration fixtures remain protected by the two-release gate. |

### Gaps Summary

No Phase 12 implementation or verification gaps remain. All seven observable truths pass with deterministic automated evidence. The stale hosted product fingerprint and final removal of inert TypeScript/TSX sources remain explicit Phase 14 scope and are not represented as locally satisfied.

---

_Verified: 2026-07-18T01:14:50Z_
_Verifier: Codex acting under the gsd-verifier contract_
