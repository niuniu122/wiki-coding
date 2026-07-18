# Hosted Linux launcher ENOEXEC fallback

## Status

Resolved locally and verified on hosted Linux on 2026-07-18. The second candidate run advanced past the launcher regression before failing later on an independent migration-fixture byte drift.

## Evidence

- Candidate workflow run: `29633595951`
- Linux job: `88051818712`
- Failing test: `rust_command_permission_provider_and_product_baselines_are_executable`
- Observed result: an executable text fixture returned exit `127` with `/bin/sh` reporting `not: not found`, instead of the stable launcher `E_START_FAILED` exit `1`.
- The same candidate run's Windows job `88051818666` completed the full release-evidence chain successfully.

## Root cause

On Linux, Node's `spawnSync` handling for an executable file with an invalid native image can reach the platform `ENOEXEC` shell fallback even when `shell: false`. The launcher checked regular-file and executable-mode properties but did not verify that the fixed sibling was an ELF image before spawning it.

## Fix

- Verify the fixed sibling's platform-native magic before spawn: PE `MZ` on Windows and ELF on Linux.
- Classify an unreadable or invalid image as the existing stable `E_START_FAILED` category.
- Pin the revised launcher hash in `fixtures/compat/source-authority.v1.json`.
- Require the native-format preflight in the executable Rust product baseline.

## Local verification

- `node --check bin/minimax-codex.cjs`
- Focused failed compatibility test: passed.
- Source-authority tests: 19 passed.
- Full workspace format and Clippy with warnings denied: passed.
- Candidate workspace and doc tests: passed with only the two hosted-record positive tests intentionally skipped.
- Candidate contract verification: passed.
- Package contract tests: 19 passed.
- Product fingerprint: `7008f47f394eecfd3f7efae205ec89b02bda813ff961f88ca96467409967bdcd` with 235 inputs.

## Remaining proof

Hosted run `29634802180`, Linux job `88055149591`, passed the launcher compatibility regression. A complete dual-platform candidate is still required before candidate evidence can be recorded.
