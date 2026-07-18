# Hosted Linux migration-fixture line-ending drift

## Status

Resolved locally on 2026-07-18. A fresh hosted candidate run is still required.

## Evidence

- Candidate workflow run: `29634802180`
- Linux job: `88055149591`
- Failing tests: `fixture_manifest_covers_every_source_evidence_file_exactly_once` and `support_window_is_counted_from_distinct_ordered_public_releases_after_v3`
- Stable failure: `MigrationSupportError("migration fixture content drift: indexes/capability.cache")`
- The launcher compatibility regression that failed the preceding candidate passed before this failure.

## Root cause

The migration manifest intentionally pins exact historical bytes. Its `indexes/capability.cache` row records 34 bytes and SHA-256 `4db8560c006edb8f658a6608803f376262e8745a79baa780b16c96cb84f1e142`, including a CRLF terminator. Because `.cache` had no repository line-ending rule, Git stored a 33-byte LF blob while the Windows checkout rewrote it to the expected CRLF bytes. Windows therefore passed and Linux read the canonical LF blob and failed closed. All seven other manifest entries matched both their worktree and Git-blob bytes.

## Fix

- Mark the complete `fixtures/compat/migration/**` corpus as `-text` because every source-evidence byte is manifest-bound and must never be rewritten at checkout.
- Renormalize `indexes/capability.cache` into the canonical Git tree as the manifest's original 34-byte CRLF evidence.
- Leave the manifest and migration validator unchanged; the fix restores their byte-exact contract instead of weakening it.

## Local verification

- The staged blob is 34 bytes and hashes to the manifest's exact SHA-256.
- Cached attributes report `text: unset` for the fixture.
- All four `migration_support` tests pass.

## Remaining proof

The new canonical tree must pass a hosted Linux GNU plus Windows MSVC candidate before candidate evidence can be recorded. No additional workflow dispatch is authorized by this resolution alone.
