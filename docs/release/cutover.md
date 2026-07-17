# Rust Default-Entry Cutover

## Current contract

The public `minimax-codex` npm command is the shell-free launcher in `bin/minimax-codex.cjs`. It accepts only Windows x64 or Linux x64 and starts the fixed sibling binary `minimax-codex.exe` or `minimax-codex`. It does not search `PATH`, read an override environment variable, invoke a shell, download a binary or model, read credentials, or fall back to TypeScript.

`minimax-codex-legacy` remains an explicit command for `dist/cli.js`. A missing, non-executable, linked, or unsupported Rust artifact produces a nonzero error that names the legacy command; choosing it is always the operator's decision.

## Evidence that authorized the switch

The prerequisite release gate is GitHub Actions run `29474558013`, tree `b4c19d5f776850808d138cf51a694789eb67be38`. Both hosted jobs passed the complete offline matrix:

- Windows x64 MSVC: 27.905 ms cold-start p95, 7,213,056-byte maximum idle RSS, 4,000,125-byte compressed archive, and 1.366 ms 10k-Wiki BM25 p95.
- Linux x64 GNU: 4.113 ms cold-start p95, 5,664,768-byte maximum idle RSS, 4,745,674-byte compressed archive, and 2.099 ms 10k-Wiki BM25 p95.

Both stayed below the 500 ms, 150 MiB, 50 MiB, and 100 ms limits. They checked 234 dependency packages and recorded zero invalid licenses, unsafe Rust files, database packages, migration network/credential paths, credentials read, Provider calls, or model downloads. The machine-readable record is `fixtures/compat/release/hosted-gates.v1.json`; the final cutover tree must pass the same matrix again.

## Refreshing hosted evidence

An ordinary push or pull request always runs the strict gate and rejects stale hosted evidence. To refresh evidence after an intentional product change, manually dispatch the `CI` workflow. Manual dispatch is the only candidate mode: it skips only comparison with the previous hosted record, while retaining the complete Windows/Linux matrix, offline compatibility and architecture checks, release packaging, performance/security budgets, and the Linux malicious-sandbox canary. Each matrix job uploads its release-evidence JSON for seven days.

After both candidate jobs succeed, bind `fixtures/compat/release/hosted-gates.v1.json` to that run, its two job IDs, the current tree, and the identical product fingerprint from both artifacts. Commit the record, then require a subsequent ordinary push run to pass in strict mode. Candidate mode is never selected for push or pull-request events and cannot make the final strict gate optional.

## Fresh install and first migration

1. Verify the release archive against its `.sha256` sidecar and inspect `RELEASE-MANIFEST.json`.
2. Extract into a new versioned directory. Keep the prior release and all `.mini-codex` source data.
3. Run the Rust `doctor` command and a normal read-only status command.
4. If TypeScript state must move, run `migrate inventory`, save `migrate dry-run --json`, review every inclusion/exclusion/collision, then use only the exact printed `MIGRATE:<hash>` confirmation.
5. Run `migrate verify` against the immutable receipt before making the new version the stable command.

Inventory and dry-run write nothing. Apply never changes, renames, truncates, or deletes the TypeScript source. Secrets, private reasoning, summaries, caches, locks, databases, and unknown records are excluded.

## Upgrade and rollback

Install each upgrade beside the active version, verify it, and change the stable launcher only after `doctor` and migration verification pass. Binary rollback points the stable command back to the prior version; it does not touch Vault content, migrated files, or receipts.

Data rollback requires the exact `ROLLBACK:<receipt-hash>` confirmation and removes only unchanged targets marked `created` by that receipt. Reused or modified files and the immutable receipt remain. There is no force path.

## Legacy support window and removal rule

The explicit TypeScript command is supported throughout the v0.1 release line and for at least 90 days after the first published Rust-default build. Removal requires a separately approved milestone, a fresh compatibility and migration audit, and a documented user notice. This cutover does not delete TypeScript source or user data.

## Distribution boundary

The base archive contains the launcher, one native Rust binary, documentation, both licenses, and the release manifest; it never contains an embedding resource. The platform npm package contains that exact verified launcher/binary plus the built TypeScript legacy entry, so one install preserves both explicit commands. Verification extracts and starts the packaged Rust default and checks both bin mappings. Hosted evidence also records a deterministic fingerprint of every tracked product input except planning documents and the evidence record itself; any product change therefore invalidates the old gate. Publishing, tagging, opening or merging a PR, and deleting the legacy implementation are separate operator actions and were not performed by this phase.
