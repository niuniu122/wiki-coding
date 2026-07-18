# Rust Release Installation, Upgrade, and Rollback

Rust is the sole product runtime. The npm package contains one CJS launcher and
one native Rust binary; it contains no TypeScript product, fallback command,
runtime dependency, lifecycle hook, model, or automatic downloader.

## Supported release artifacts

Only these hosted release targets are supported:

| Target ID | Rust host | Binary | Tier |
|---|---|---|---|
| `windows-x86_64-msvc` | `x86_64-pc-windows-msvc` | `minimax-codex.exe` | `hosted_release` |
| `linux-x86_64-gnu` | `x86_64-unknown-linux-gnu` | `minimax-codex` | `hosted_release` |

`windows-x86_64-gnullvm-dev` / `x86_64-pc-windows-gnullvm` is local
`development_only` evidence. Do not distribute it as Windows MSVC evidence.

Each target has a versioned native `.tar.gz`, a platform npm `.tgz`, and a
`.sha256` sidecar for each archive. `RELEASE-MANIFEST.json` binds the version,
target, product fingerprint, Rust binary, launcher, native archive, and npm
archive. `embeddingIncluded` must be `false`.

Before installation:

1. Obtain the artifact and sidecar from the same release and target.
2. Compare the artifact SHA-256 with the sidecar.
3. Inspect the matching `RELEASE-MANIFEST.json` from the native archive.
4. Require the exact target ID/Rust host, archive names, and hashes you intend
   to install.

## npm global installation

Node.js 20 or newer is required for the thin launcher. Install the already
downloaded platform npm tarball without lifecycle scripts:

```bash
npm install --global --ignore-scripts ./minimax-codex-v0.1.0-<target>-npm.tgz
minimax-codex --version
minimax-codex doctor
```

The installed `minimax-codex` command launches only the fixed sibling
`minimax-codex.exe` or `minimax-codex`. It performs no shell invocation,
`PATH` search, override lookup, fallback, or download.

## One-time npx execution

Run the already-downloaded platform tarball without a global install:

```bash
npx --offline --yes --package ./minimax-codex-v0.1.0-<target>-npm.tgz minimax-codex --version
npx --offline --yes --package ./minimax-codex-v0.1.0-<target>-npm.tgz minimax-codex doctor
```

`--offline` prevents registry access. The package still requires its included
native binary for the current supported host.

## Native archive installation

1. Extract the verified native archive into a new versioned directory such as
   `minimax-codex/versions/0.1.0`.
2. Run `minimax-codex.exe --version` on Windows or
   `./minimax-codex --version` on Linux.
3. Run the native binary's `doctor` command.
4. Run `node bin/minimax-codex.cjs doctor` and require the same Rust identity.
5. Point the stable command or wrapper at the new version only after both paths
   succeed.
6. Keep the previous versioned directory until normal work passes.

The archives and launcher never read credentials, download an embedding model,
or migrate data automatically.

## Actionable no-fallback failures

The launcher exits nonzero with one stable category:

- `E_UNSUPPORTED_HOST`: install a package for Windows x64 or Linux x64;
- `E_BINARY_MISSING`: reinstall the complete matching platform package;
- `E_BINARY_UNSAFE`: replace a linked/directory/non-regular sibling with the
  authentic package;
- `E_BINARY_NOT_EXECUTABLE`: restore the packaged Linux executable mode or
  reinstall;
- `E_START_FAILED`: check target compatibility and reinstall the correct
  platform artifact;
- `E_SIGNAL_TERMINATION`: inspect the host/runtime failure and do not treat it
  as a successful launch.

There is no TypeScript or alternate-runtime fallback. Do not work around these
errors by adding an unrelated executable to `PATH`.

## Subprocess sandbox prerequisite

On Linux, install Bubblewrap through the operating-system package manager before
using confirm-mode Cargo/Git/npm diagnostics. Run `minimax-codex doctor` and
require `subprocess_sandbox` to report the enforced Bubblewrap-plus-seccomp
backend. A missing or namespace-blocked backend fails before project code starts.

Ubuntu 24.04 may restrict unprivileged user namespaces through AppArmor. Prefer
an administrator-managed, targeted `userns` profile for `/usr/bin/bwrap`; do not
disable host protections or use `full-access` for an unfamiliar project merely
to bypass the check.

Windows remains a supported product platform, but this release has no native
restricted-token/WFP subprocess backend. Confirm-mode process diagnostics fail
closed. `full-access` is a process-scoped escape hatch only for projects you
already trust and provides ordinary host filesystem/network access.

See [the complete sandbox trust boundary](subprocess-sandbox.md).

## Upgrade

Install the new release beside the active version. Verify its sidecars and
manifest, run `--version` and `doctor`, then evaluate TypeScript-era state before
changing the stable command:

```bash
minimax-codex migrate inventory
minimax-codex migrate dry-run --json
minimax-codex migrate apply --plan <plan> --confirmation <printed-value>
minimax-codex migrate verify --receipt <receipt>
```

Save the dry-run JSON outside `.mini-codex`. Review every inclusion, exclusion,
and collision. Apply only with the exact printed confirmation. Never overwrite
the active version directory; this keeps binary rollback independent of data
rollback.

## Binary rollback

Point the stable command back to the previous verified versioned directory.
Migration receipts, imported files, the Vault, and `.mini-codex` source data are
not modified by binary rollback.

## Data rollback

Verify the receipt first. If every receipt-owned target is still unchanged, run:

```bash
minimax-codex migrate rollback --receipt <receipt> --confirmation ROLLBACK:<receipt-hash>
```

Rollback removes only unchanged targets marked `created` by that receipt. It
never removes reused files, modified targets, the immutable apply receipt, or
anything in `.mini-codex`. There is no `--force` path; resolve collisions or
changed targets manually and preserve the receipt as audit evidence.

## Migration fixture support window

Static TypeScript-era migration fixtures are retained by a machine-checkable
release-count rule, not a day count. Cutover release `3.0.0` must be followed by
at least two distinct, ordered public releases recorded in
`fixtures/compat/migration/typescript-v1/support-window.v1.json` before fixture
removal can become eligible. The current source data remains immutable during
inventory, dry-run, apply, verify, rollback, and the entire support window.

The detailed authority and hosted-evidence contract is in
[the Rust-only cutover guide](cutover.md).
