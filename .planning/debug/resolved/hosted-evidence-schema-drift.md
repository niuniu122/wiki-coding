---
status: resolved
trigger: "Phase 14-03 requires candidate-only interim evidence followed by combined candidate and strict hosted evidence, but the Rust validator only accepts the legacy single-run main-branch schema."
created: 2026-07-18
updated: 2026-07-18
---

# Hosted Evidence Schema Drift

## Symptoms

- Expected behavior: Task 2 records one candidate run with Windows MSVC and Linux GNU jobs, marks strict evidence pending, then Task 3 records a later strict run for the same frozen product identity.
- Actual behavior: `HostedReleaseGate` accepts only one `runId`/`runUrl`, hard-codes `branch == "main"`, and has no candidate/strict sections or pending state.
- Error messages: no remote failure was allowed to occur; pre-push contract inspection proved the planned document would be rejected by `deny_unknown_fields` or could not represent both runs.
- Timeline: discovered during the Phase 14-03 authorization preflight before the first remote write.
- Reproduction: compare `.planning/phases/MMX-14-typescript-removal-and-hosted-closure/14-03-PLAN.md` Task 2/3 with `crates/compat-harness/src/baseline.rs::HostedReleaseGate`.

## Current Focus

- hypothesis: Phase 14-02 added fingerprint-v3 freshness but did not migrate the hosted evidence schema required by the already-written Phase 14-03 execution plan.
- test: add fail-first fixtures/tests for interim candidate-only and final combined records, branch/head/tree/run/job URL identity, target-specific artifacts, installed identities, counters, and Linux sandbox canary.
- expecting: interim records fail strict validation with an explicit pending category while final combined records pass only when both ordered runs bind the same frozen product.
- next_action: commit the verified repair, regenerate the final fingerprint/intake, then resume the authorized candidate/interim/strict sequence.
- reasoning_checkpoint: direct initial push is unsafe because it would start strict mode before an interim candidate record exists; use a same-tree `[skip ci]` candidate-anchor commit only after the schema repair and local refreeze.
- tdd_checkpoint: passed

## Evidence

- timestamp: 2026-07-18T00:00:00Z
  observation: `HostedReleaseGate` has one run and rejects unknown fields; validation also requires `branch == "main"`.
- timestamp: 2026-07-18T00:00:01Z
  observation: Plan 14-03 explicitly requires candidate, interim pending, strict, and final combined evidence on the authorized branch.

## Eliminated

- hypothesis: The legacy single-run record can truthfully encode both candidate and strict runs.
  reason: It has only one run identity and one pair of jobs.
- hypothesis: The first ordinary branch push can be accepted as the strict run.
  reason: It necessarily precedes candidate artifact validation and the remotely visible interim record.

## Resolution

- root_cause: Phase 14-02 refreshed fingerprint freshness but retained the legacy single-run hosted schema and candidate-only artifact upload, creating a circular strict-run dependency for the Phase 14-03 candidate/interim/strict plan.
- fix: Added separate candidate, strict-precondition, and final-closure verification modes; schema-v2 combined run/job evidence; unconditional post-milestone artifact upload; and fail-closed target, URL, identity, sandbox, hash, security, license, performance, and offline validation.
- verification: Synthetic pending/final/ordering/tier-confusion tests, source-authority tests, full candidate workspace/doc tests, workspace Clippy with warnings denied, candidate contract verification, and package corruption tests all passed offline.
- files_changed: .github/workflows/ci.yml, package.json, README.md, docs/release/cutover.md, crates/compat-harness/src/{baseline,lib,main,report,source_authority}.rs, crates/compat-harness/tests/{compat_report,source_authority}.rs
