# Rust Release Installation, Upgrade, and Rollback

## Supported base artifacts

The v1 release matrix is `windows-x86_64-msvc` and `linux-x86_64-gnu`. Each release has one versioned `.tar.gz`, a `.sha256` sidecar, `RELEASE-MANIFEST.json`, the Rust executable, and both project licenses. A `*-gnullvm-dev` archive is local development evidence only and is not a supported Windows release.

Before installation, compare the archive SHA-256 with the sidecar and inspect `RELEASE-MANIFEST.json`. The manifest must name the expected version/platform, match the executable hash, and say `embeddingIncluded: false`.

## Fresh install

1. Download the base archive and matching `.sha256` from the same version.
2. Verify SHA-256 with the operating-system checksum tool.
3. Extract into a versioned directory such as `minimax-codex/versions/0.1.0`.
4. Point the stable `minimax-codex` launcher location at that version only after `minimax-codex doctor` succeeds.
5. Keep the prior versioned directory until the new version has passed normal work.

The archive and launcher never download an embedding model, read a credential, or migrate data automatically.

## Upgrade

Install the new archive beside the current version, verify its checksum/manifest, run `doctor`, then run `migrate inventory` and `migrate dry-run` if TypeScript data must be imported. Save the dry-run JSON outside `.mini-codex`; apply only with the exact printed confirmation. Run `migrate verify` against the resulting receipt before changing the stable launcher.

Never overwrite the existing version directory. This keeps binary rollback independent from data rollback.

## Binary rollback

Point the stable launcher back to the previous verified versioned directory. Migration receipts and imported evidence remain untouched. During the support window, `minimax-codex-legacy` runs the TypeScript entry directly.

## Data rollback

Run `migrate verify --receipt <receipt>` first. If every receipt-owned target is unchanged, run `migrate rollback --receipt <receipt> --confirmation ROLLBACK:<receipt-hash>`. Rollback removes only unchanged files marked `created`; it never removes reused files, modified targets, the immutable apply receipt, or anything in `.mini-codex`.

There is no `--force` path. Resolve a collision or changed target manually and preserve the receipt as audit evidence.
