# Rust Release Installation, Upgrade, and Rollback

## Supported base artifacts

The v1 release matrix is `windows-x86_64-msvc` and `linux-x86_64-gnu`. Each platform has a versioned base `.tar.gz` and platform npm `.tgz`, each with a `.sha256` sidecar. The base archive contains `RELEASE-MANIFEST.json`, the shell-free `bin/minimax-codex.cjs` launcher, one Rust executable, documentation, and both project licenses. The npm package contains that exact launcher and native binary plus `dist/cli.js` as the explicit `minimax-codex-legacy` fallback. A `*-gnullvm-dev` artifact is local development evidence only and is not a supported Windows release.

Before installation, compare the chosen artifact SHA-256 with its sidecar and inspect `RELEASE-MANIFEST.json` from the matching base archive. The manifest must name the expected version/platform, match the executable and npm-package hashes, name `dist/cli.js` as the legacy entry, and say `embeddingIncluded: false`.

## Subprocess sandbox prerequisite

On Linux, install Bubblewrap from the operating-system package manager before using confirm-mode Cargo/Git/npm diagnostics. For example, Debian/Ubuntu use `sudo apt-get install bubblewrap`. Run `bwrap --version`, then run `minimax-codex doctor`; the `subprocess_sandbox` check must report an enforced Bubblewrap backend. A missing or namespace-blocked backend causes process tools to fail closed before target code starts.

Windows remains a supported CLI/Provider/file-tool/retrieval/Vault/Wiki platform, but this version does not bundle Codex's large restricted-token/ACL/firewall subsystem. Therefore confirm-mode process diagnostics report `sandbox_unavailable`. `full-access` is the explicit process-scoped escape hatch for a project you already trust; it disables subprocess filesystem/network isolation and must not be used for unknown repositories.

See [the complete sandbox trust boundary](subprocess-sandbox.md) before asking another person or coding agent to test an untrusted project.

## Fresh install

1. Download the base archive and matching `.sha256` from the same version.
2. Verify SHA-256 with the operating-system checksum tool.
3. Extract into a versioned directory such as `minimax-codex/versions/0.1.0`.
4. On Linux, install/verify Bubblewrap; then run the native executable's `doctor` command and confirm `node bin/minimax-codex.cjs doctor` reaches that same executable.
5. Point the stable `minimax-codex` command at that version only after both checks succeed.
6. Keep the prior versioned directory until the new version has passed normal work.

The archives and launcher never download an embedding model, read a credential, or migrate data automatically. Release verification extracts the npm package and starts its actual Rust default before an artifact is accepted.

## Upgrade

Install the new archive beside the current version, verify its checksum/manifest, run `doctor`, then run `migrate inventory` and `migrate dry-run` if TypeScript data must be imported. Save the dry-run JSON outside `.mini-codex`; apply only with the exact printed confirmation. Run `migrate verify` against the resulting receipt before changing the stable launcher.

Never overwrite the existing version directory. This keeps binary rollback independent from data rollback.

## Binary rollback

Point the stable launcher back to the previous verified versioned directory. Migration receipts and imported evidence remain untouched. During the v0.1 support window (and for at least 90 days after the first published Rust-default build), `minimax-codex-legacy` runs the TypeScript entry directly. It is never selected automatically.

The detailed entrypoint and removal contract is in `docs/release/cutover.md`.

## Data rollback

Run `migrate verify --receipt <receipt>` first. If every receipt-owned target is unchanged, run `migrate rollback --receipt <receipt> --confirmation ROLLBACK:<receipt-hash>`. Rollback removes only unchanged files marked `created`; it never removes reused files, modified targets, the immutable apply receipt, or anything in `.mini-codex`.

There is no `--force` path. Resolve a collision or changed target manually and preserve the receipt as audit evidence.
