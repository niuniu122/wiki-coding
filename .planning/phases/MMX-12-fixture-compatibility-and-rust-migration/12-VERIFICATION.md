---
phase: MMX-12-fixture-compatibility-and-rust-migration
verified: 2026-07-17T21:30:08Z
status: gaps_found
score: 5/7 must-haves verified
behavior_unverified: 0
overrides_applied: 0
gaps:
  - truth: "The no-TypeScript dependency gate covers every Rust module compiled into and reached by the compatibility executable."
    status: partial
    reason: "The current fixture-only path is Rust-only, but validate_compatibility_source_boundary scans four fixed files while the same executable compiles and calls additional compat-harness modules. The negative mutation test only targets report.rs, so a TypeScript process/import/build dependency in an omitted module is not rejected by this gate."
    artifacts:
      - path: "crates/compat-harness/src/report.rs"
        issue: "compatibility_sources is a four-file allowlist and excludes lib.rs, provider_eval.rs, retrieval_eval.rs, migration_support.rs, coverage.rs, source_authority.rs, and architecture.rs."
      - path: "crates/compat-harness/tests/compat_report.rs"
        issue: "All three dependency mutations are appended only to crates/compat-harness/src/report.rs."
    missing:
      - "Enumerate the complete compat-harness Rust source tree or derive the executable module closure and fail closed on forbidden TypeScript import/build/process links in every compiled module."
      - "Add at least one deterministic negative mutation in a currently omitted module that is reached by verify/verify-candidate."
  - truth: "Forged operation manifests and recomputed receipts cannot promote pre-existing allowlisted targets to created ownership."
    status: failed
    reason: "Receipt integrity is an unkeyed, recomputable SHA-256 of self-declared fields. Validation limits paths to three names but never binds created versus reused ownership back to the original plan/operation. Rollback therefore trusts the forged created list and deletes matching files. Interrupted recovery similarly compares candidates with every expected target, not with an authenticated set that was absent before publish."
    artifacts:
      - path: "crates/cli/src/migration.rs"
        issue: "MigrationReceipt::validate recomputes the supplied body hash; validate_receipt_ownership checks only an allowlist; rollback_created then removes every matching created entry without loading authoritative plan/operation ownership."
      - path: "crates/cli/tests/migration.rs"
        issue: "The recomputed-receipt and forged-operation tests attack only an unowned path outside the allowlist or use a wrong plan hash; they do not reclassify a pre-existing byte-identical allowlisted target."
    missing:
      - "Bind receipt created/reused classification to durable plan/operation evidence that cannot be changed by recomputing the receipt body alone."
      - "Reject a recomputed receipt or operation manifest that claims a pre-existing byte-identical .minimax/config.json (and the other allowlisted targets) as created."
  - truth: "All source and target symlink paths fail closed before migration mutates the target."
    status: failed
    reason: "Source symlinks are rejected, but safe_join proves only lexical starts_with containment. It does not inspect or canonicalize existing parent components, so a symlinked .minimax parent can redirect lock, staging, receipt, and migrated-target writes outside the canonical target root."
    artifacts:
      - path: "crates/cli/src/migration.rs"
        issue: "safe_join validates relative components but does not reject symlinked target ancestors before write_new_file/create_dir_all follows them."
      - path: "crates/cli/tests/migration.rs"
        issue: "The symlink test replaces only a source config file; no target-parent symlink case exists."
    missing:
      - "Reject symlinks in every existing target ancestor and prove the resolved destination remains under the canonical target root before any lock, staging, receipt, rollback, or artifact write."
      - "Add a deterministic target .minimax symlink negative test asserting no outside file or directory is created."
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
**Verified:** 2026-07-17T21:30:08Z
**Status:** gaps_found
**Re-verification:** No - initial independent verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|---|---|---|
| 1 | Compatibility compares the current Rust product with an immutable versioned public-contract fixture and explicit approved differences. | VERIFIED | `public-contract.v1.json` has 34 sorted unique `contract.*` identities, a strict provenance/fingerprint, exact command/profile/protocol closure, and five linked difference IDs. Manifest tamper/schema/evidence tests passed. |
| 2 | The report is deterministic, complete, machine-readable, and contains contract/Rust evidence rather than live TypeScript product rows. | VERIFIED | Golden/repeat/completeness tests passed; the independent report command emitted 34 unique entries with the exact `sha256:91d2...cec5` contract fingerprint and no live `typescript.*` row. |
| 3 | The current fixture compatibility path runs successfully without a TypeScript product/test/generated tree. | VERIFIED | `compatibility_report_and_verify_are_hermetic_without_typescript_runtime` passed from a copy lacking top-level `src/`, `dist/`, and `test/`; `verify-candidate` exited 0. |
| 4 | The no-TypeScript dependency gate fail-closes over every module compiled into/reached by the compatibility executable. | FAILED | `report.rs:203-208` scans only four fixed files, but `report.rs:248-249` directly calls omitted Provider/retrieval modules and `lib.rs:6-14` compiles further omitted modules. Mutations at `compat_report.rs:165-172` only target `report.rs`. |
| 5 | Rust migration inventory, dry-run, apply, verify, idempotency, collision, interruption recovery, and narrow rollback work from immutable temporary fixture copies while source bytes remain unchanged. | VERIFIED | All 15 existing migration integration tests passed, including deterministic dry-run, bounds, secrets/private reasoning, source drift, target drift, collision, replay, idempotency, recovery, verify, and narrow rollback cases. |
| 6 | Adversarial ownership and target-path boundaries fail closed for recomputed receipts, forged operations, and symlinked paths. | FAILED | Receipt created/reused ownership is self-declared behind a recomputable hash and path allowlist; recovery accepts candidates from the full expected-target set; target joins are lexical and do not reject symlink ancestors. Existing negative tests do not exercise these exact cases. |
| 7 | TypeScript-v1 migration evidence is exact/fingerprinted and cannot be marked removal-eligible before two distinct ordered public releases after v3.0.0. | VERIFIED | Eight evidence files exactly match path/length/SHA-256/policy rows; support metadata records zero subsequent releases and `removalEligible: false`; all four manifest/window positive and negative tests passed. |

**Score:** 5/7 truths verified (0 present-but-behavior-unverified)

### Required Artifacts

| Artifact | Expected | Status | Details |
|---|---|---|---|
| `fixtures/compat/public-contract.v1.json` | Immutable public-contract identities/provenance/evidence/difference links | VERIFIED | 34 required IDs equal 34 items; strict schema, provenance `84784f5`, product entry, evidence existence, and fingerprint checks are substantive. |
| `crates/compat-harness/src/manifest.rs` | Strict manifest loading and exact contract closure | VERIFIED | Rejects unknown fields, schema/provenance/fingerprint drift, duplicate/lost IDs, pending/empty evidence, missing evidence paths, and invalid difference links. |
| `fixtures/compat/report.expected.json` | Byte-stable contract-versus-Rust golden | VERIFIED | Exact report test and independent report command agree. |
| `crates/compat-harness/src/report.rs` | Deterministic report plus complete no-TypeScript boundary | PARTIAL | Report composition/validation is substantive and wired; the no-TypeScript source boundary is not complete over its executable module set. |
| `fixtures/compat/migration/typescript-v1/manifest.v1.json` | Exact recursive fixture inventory and policy | VERIFIED | Eight non-metadata files are sorted and bound by length/hash/role/disposition; validator recomputes all values. |
| `fixtures/compat/migration/typescript-v1/support-window.v1.json` | Machine-checkable two-release retention record | VERIFIED | Cutover `3.0.0`, minimum 2, observed 0, eligibility false; strict ordered SemVer validator is wired. |
| `crates/compat-harness/src/migration_support.rs` | Fixture fingerprint and support-window gate | VERIFIED | Substantive, exported, and called by repository verification before compatibility/release evidence. |
| `crates/cli/src/migration.rs` | Source-preserving Rust migration with fail-closed ownership/path boundaries | PARTIAL | Positive lifecycle and many hostile inputs are substantive; forged created/reused ownership and target-parent symlink containment remain unsafe. |
| `crates/cli/tests/migration.rs` | Complete hostile/lifecycle integration evidence | PARTIAL | 15 tests pass, but the forged cases stop at non-allowlisted paths/wrong plan hash and symlink coverage is source-only. |

### Key Link Verification

| From | To | Via | Status | Details |
|---|---|---|---|---|
| `manifest.rs` | `public-contract.v1.json` | Strict deserialize, expected-ID derivation, evidence existence, and fingerprint recomputation | WIRED | Exact fixture mutations fail closed. |
| `report.rs` | public contract + command differences | `build_report` exact join and `validate_report` rebuild equality | WIRED | Every contract item appears once; differences exactly match links and locked outcomes. |
| `main.rs` | migration fixture/support gate | `verify_repository` calls both validators before compatibility checks | WIRED | `verify-candidate` exercises the link. |
| no-TypeScript source boundary | complete compat executable module closure | fixed four-file list | NOT_WIRED | Several compiled/reached modules are outside the scan and outside the mutation test. |
| `MigrationReceipt` | authoritative created/reused ownership | self-hash + three-path allowlist | NOT_WIRED | No original plan/operation ownership is loaded before deletion. |
| migration target paths | canonical target root | lexical `safe_join` | NOT_WIRED | Existing parent symlinks are not rejected/resolved. |

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
|---|---|---|---|---|
| Compatibility report | `entries` | strict public contract + exact approved-difference fixture + live Rust evidence paths | Yes, 34 deterministic entries | FLOWING |
| Migration plan/artifacts | `included`, `excluded`, `targets`, staged bytes | bounded recursive source scan and Rust normalization | Yes, temp-fixture config/session/capability outputs | FLOWING |
| Migration receipt/rollback | `created` and `reused` | receipt body supplied by file | Data is present, but ownership is not independently anchored | UNSAFE OWNERSHIP |
| Support status | observed releases / computed eligibility | support-window fixture bound to migration fixture fingerprint | Yes, zero releases and false eligibility | FLOWING |

### Behavioral Spot-Checks

All Rust commands used the already-installed `1.97.0-x86_64-pc-windows-gnullvm` toolchain with `rust-lld`, `CARGO_NET_OFFLINE=true`, and an isolated target directory. This is local development evidence only, not Windows MSVC, Linux GNU, hosted, or release evidence.

| Behavior | Command | Result | Status |
|---|---|---|---|
| Fixture-owned compatibility, golden, no-TypeScript hermetic path, architecture negatives | `cargo test -p minimax-compat-harness --test compat_report --test migration_support --locked -- --skip hosted_cutover_evidence_matches_current_product` | 22 compat tests passed (1 hosted filtered); migration support independently rerun 4/4 passed | PASS |
| Exact migration manifest/support-window validation | `cargo test -p minimax-compat-harness --test migration_support --locked` | 4 passed | PASS |
| Migration lifecycle and hostile inputs | `cargo test -p minimax-cli --test migration --locked` | 15 passed, including the 47.9 s bounds case | PASS |
| Deterministic machine-readable report | `cargo run -p minimax-compat-harness --locked -- report --format json` | exited 0; 34 exact entries/fingerprint | PASS |
| Repository candidate gate | `cargo run -p minimax-compat-harness --locked -- verify-candidate` | exited 0 | PASS |
| Formatting and diff hygiene | `cargo fmt --all -- --check`; `git diff --check` | both exited 0 before this report was added | PASS |
| Hosted product fingerprint | exact `hosted_cutover_evidence_matches_current_product` test | expected `CutoverEvidence` failure | DEFERRED TO PHASE 14 |

The two forged-ownership cases were not executed as custom attack probes. Per the verification constraint, their failure is established from the exact deterministic deletion path and the missing authoritative link, not by creating a custom executable or launching a Node/TypeScript payload.

### Probe Execution

No Phase 12 plan or summary declares a `probe-*` script or `<human-check>` block. Step 7c is not applicable.

### Requirements Coverage

| Requirement | Source Plan | Status | Evidence |
|---|---|---|---|
| RCMP-01 | 12-01 | PARTIAL | Immutable fixture, deterministic report, and current hermetic Rust-only behavior pass; whole-executable no-TypeScript regression enforcement is incomplete. |
| RCMP-02 | 12-02 | BLOCKED | Positive lifecycle, source preservation, fixture inventory, and release gate pass; forged created/reused ownership and target symlink containment do not fail closed. |
| RCMP-03 | none | NOT DEFINED | No RCMP-03 exists in `REQUIREMENTS.md`, `ROADMAP.md`, Phase 12 SPEC, or either plan. Phase 12's declared contract is RCMP-01 and RCMP-02 only, so this is a scope note rather than an orphaned implementation requirement. |

Both requirements declared by Phase 12 are claimed by a plan; there is no orphaned defined requirement.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|---|---|---|---|---|
| `crates/compat-harness/src/report.rs` | 203 | Fixed security-source list narrower than executable module closure | BLOCKER | Future TypeScript runtime dependency can be introduced in an omitted compat module without this gate detecting it. |
| `crates/cli/src/migration.rs` | 292 | Recomputable self-hash treated as ownership evidence | BLOCKER | A recomputed receipt can reclassify an existing allowlisted target as created. |
| `crates/cli/src/migration.rs` | 1296 | Path allowlist substitutes for created/reused provenance | BLOCKER | Allowed name does not prove the migration created the file. |
| `crates/cli/src/migration.rs` | 1485 | Lexical containment without target-ancestor symlink rejection | BLOCKER | Writes can follow a symlinked `.minimax` parent outside the canonical target root. |

No unreferenced `TBD`, `FIXME`, or `XXX` debt marker and no implementation placeholder was found in the Phase 12 changed artifacts.

### Human Verification Required

None. The passing behaviors and the blocking gaps are deterministic code/test concerns; no visual, external-service, or subjective verification is needed.

### Deferred Items

| Item | Addressed In | Evidence |
|---|---|---|
| Stale hosted product fingerprint and final Windows MSVC/Linux GNU evidence | Phase 14 | Roadmap success criterion 3 owns fresh hosted evidence for one final fingerprint. The independently run exact hosted test fails only with `CutoverEvidence`; this must not be refreshed locally. |
| TypeScript/TSX product/test source deletion | Phase 14 | Success criterion 1 and plan 14-01 own deletion; static TypeScript-v1 migration fixtures remain protected by the two-release gate. |

### Gaps Summary

Phase 12 has a strong positive-path implementation: fixture identity, deterministic reporting, current no-TypeScript execution, source-preserving migration behavior, exact migration evidence, and the release-count support window all pass. The phase is not yet safe to close because three negative authority boundaries are incomplete: the no-TypeScript scan does not cover the full compatibility executable, receipt/operation ownership can be recomputed or self-declared for allowlisted targets, and target ancestor symlinks are not rejected before writes. These are local Rust/test gaps and are not the intentionally deferred hosted fingerprint failure.

---

_Verified: 2026-07-17T21:30:08Z_
_Verifier: Codex acting under the gsd-verifier contract_
