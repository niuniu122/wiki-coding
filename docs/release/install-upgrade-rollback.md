# Installation, Upgrade, and Rollback

The recommended distribution is the `minimax-codex` package from the npm
registry. It contains one small CJS launcher plus prebuilt Windows x64 and Linux
x64 binaries. Users need Node.js 20 or newer, but do not need Rust, Cargo, a
C/C++ compiler, Git, or a source checkout.

The package contains no TypeScript product, fallback command, runtime
dependency, lifecycle hook, model, or automatic downloader.

## Supported release artifacts

Only these hosted release targets are supported:

| Target ID | Rust host | Binary | Tier |
|---|---|---|---|
| `windows-x86_64-msvc` | `x86_64-pc-windows-msvc` | `minimax-codex.exe` | `hosted_release` |
| `linux-x86_64-gnu` | `x86_64-unknown-linux-gnu` | `minimax-codex` | `hosted_release` |

`windows-x86_64-gnullvm-dev` / `x86_64-pc-windows-gnullvm` is local
`development_only` evidence. Do not distribute it as Windows MSVC evidence.

Each release publishes one universal npm package containing both hosted
binaries. Advanced/offline users can also use the per-target native `.tar.gz`
artifacts. Every downloadable release artifact has a `.sha256` sidecar, and its
manifest binds the version, target, product fingerprint, binary, launcher, and
archive. `embeddingIncluded` must be `false`.

For the normal npm registry path, npm performs package-integrity verification.
For a manually downloaded tarball or native archive:

1. Obtain the artifact and sidecar from the same release.
2. Compare the artifact SHA-256 with the sidecar.
3. Inspect the matching release manifest.
4. Require the exact version, supported target identities, archive names, and
   hashes you intend to install.

## npm global installation

Install the published CLI directly from the npm registry:

```bash
npm install --global minimax-codex
minimax-codex --version
minimax-codex doctor
```

The installed `minimax-codex` command launches only the fixed sibling
`minimax-codex.exe` or `minimax-codex` from the same installed package. It
performs no shell invocation, `PATH` search, override lookup, fallback, or
download.

## Configure the MiniMax credential and start chat

Set the credential for the current terminal session, verify the environment,
and launch chat without a subcommand.

PowerShell:

```powershell
$env:MINIMAX_API_KEY = "<your-key>"
minimax-codex doctor
minimax-codex
```

POSIX shell:

```bash
export MINIMAX_API_KEY="<your-key>"
minimax-codex doctor
minimax-codex
```

The CLI reads `MINIMAX_API_KEY` at startup and never copies its value into
project configuration, runtime journals, traces, or Vault evidence. `/api`
prints setup commands with a placeholder and never accepts or echoes the key.

## Project-local installation

Install the CLI as a development dependency when a project needs a pinned
version:

```bash
npm install --save-dev minimax-codex
npx minimax-codex --version
npx minimax-codex doctor
```

## One-time npx execution

Run the current registry release once without a global installation:

```bash
npx minimax-codex --version
npx minimax-codex doctor
```

For an offline install, first download the release tarball and `.sha256`
sidecar on a connected machine, verify them, then install the exact file:

```bash
npm install --global --ignore-scripts ./minimax-codex-0.1.1.tgz
```

The universal tarball still supports only Windows x64 and Linux x64.

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

Upgrade a global installation, then verify it before normal work:

```bash
npm install --global minimax-codex@latest
minimax-codex --version
minimax-codex doctor
```

For a project-local installation, use
`npm install --save-dev minimax-codex@latest` and commit the updated lockfile.
Before migrating TypeScript-era state, inspect and verify the plan:

```bash
minimax-codex migrate inventory
minimax-codex migrate dry-run --json
minimax-codex migrate apply --plan <plan> --confirmation <printed-value>
minimax-codex migrate verify --receipt <receipt>
```

Save the dry-run JSON outside `.mini-codex`. Review every inclusion, exclusion,
and collision. Apply only with the exact printed confirmation. Package upgrades
do not migrate project data automatically.

## Binary rollback

Install the exact previous published version, replacing `<previous-version>`
with a known good version number:

```bash
npm install --global minimax-codex@<previous-version>
minimax-codex --version
minimax-codex doctor
```

For a project-local rollback, run
`npm install --save-dev minimax-codex@<previous-version>` and commit the lockfile.
Migration receipts, imported files, the Vault, and `.mini-codex` source data are
not modified by package rollback.

For example, after a future upgrade beyond `0.1.0`, the explicit command to
return to the first release is `npm install --global minimax-codex@0.1.0`.

## Uninstall

Remove a global installation with:

```bash
npm uninstall --global minimax-codex
```

For a project-local installation, use `npm uninstall minimax-codex` and commit
the updated package manifest and lockfile. Uninstalling the package does not
delete project Vaults, migration receipts, or `.mini-codex` source data.

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
release-count rule, not a day count. Cutover release `0.1.0` must be followed by
at least two distinct, ordered public releases recorded in
`fixtures/compat/migration/typescript-v1/support-window.v1.json` before fixture
removal can become eligible. The current source data remains immutable during
inventory, dry-run, apply, verify, rollback, and the entire support window.

The detailed authority and hosted-evidence contract is in
[the Rust-only cutover guide](cutover.md).
