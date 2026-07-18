# Rust-Only Cutover and Hosted Closure

## Current authority contract

Rust is the sole product, test, compatibility, Provider-evaluation, and
retrieval-evaluation authority. The public npm surface contains exactly one bin:

```text
minimax-codex -> bin/minimax-codex.cjs -> fixed sibling Rust binary
```

The CJS launcher and MJS release scripts are distribution orchestration only.
They do not implement Provider, retrieval, session, Vault, Wiki, tool,
migration, or fallback behavior. Static TypeScript-era files under
`fixtures/compat/` are immutable compatibility/migration data and are never
built or executed.

The launcher accepts only `win32/x64` or `linux/x64` and starts
`minimax-codex.exe` or `minimax-codex` beside the package. It does not search
`PATH`, read a binary override, invoke a shell, download a binary/model, read
credentials, or fall back to another runtime.

## Release target identity

Final hosted release authority requires both exact targets:

- `windows-x86_64-msvc` with Rust host `x86_64-pc-windows-msvc`;
- `linux-x86_64-gnu` with Rust host `x86_64-unknown-linux-gnu`.

Local `windows-x86_64-gnullvm-dev` evidence is `development_only`. It can prove
local package behavior but can never substitute for hosted Windows MSVC.

## Distribution boundary

The native archive contains one Rust binary, `bin/minimax-codex.cjs`, release
documentation, both licenses, and the release manifest. The platform npm package
contains the same launcher and Rust binary plus npm metadata. Neither archive
contains TypeScript/TSX source, `dist/`, a legacy bin, runtime dependencies,
install hooks, an embedding resource, or a downloader.

Release verification checks:

- exact target ID, Rust host, support tier, product fingerprint, and file count;
- archive, npm, binary, launcher, and entry hashes/modes;
- independently extracted native and npm installed Rust identities;
- missing/unsafe sibling rejection with no alternate child process;
- Rust tests, Provider/retrieval evaluation, compatibility, and migration;
- package corruption, licenses, unsafe-code/database/migration boundaries;
- cold-start, idle RSS, compressed-size, and 10k-Wiki BM25 thresholds;
- offline execution with zero Provider calls, credential reads, and model
  downloads.

## Product fingerprint v3

Fingerprint v3 hashes the current bytes of every tracked and untracked product
input together with its canonical path and authority mode. Source,
configuration, fixtures, launcher, release scripts, and user/maintainer docs are
included. Only `.planning/**` and
`fixtures/compat/release/hosted-gates.v1.json` are excluded to avoid planning
noise and a self-referential evidence hash.

Any product edit invalidates an older fingerprint, package, intake, and hosted
record. The final fingerprint is frozen only after documentation is complete.

## Candidate and strict hosted evidence

The checked-in hosted record is not trusted merely because it is well-formed.
It must match the final fingerprint/file count and exact successful Windows MSVC
and Linux GNU jobs.

The auditable refresh order is:

1. Complete all tracked product/docs edits and generate the local final intake.
2. Obtain fresh, explicit authorization for the bounded Git/hosted-CI actions.
3. Manually dispatch one `CI` workflow candidate run. Candidate mode skips only
   comparison with the prior hosted record; all other Rust, sandbox,
   evaluation, compatibility, package, install, security, license, and
   performance gates remain mandatory.
4. Download and validate both exact target artifacts against the frozen
   fingerprint and hashes.
5. Write and push an interim candidate-only evidence record with strict status
   pending.
6. Allow the ordinary push-triggered strict run to validate that exact pending
   candidate record through the strict-precondition test and contract routes;
   it must not require its own not-yet-produced strict evidence.
7. Download strict artifacts, validate run/job/head/tree/target/hash fields, and
   complete the combined hosted record.
8. Run local strict verification against the unchanged frozen local root.

Strict mode rejects stale, pre-deletion, mixed-run, missing-target,
development-tier, or hash-mismatched evidence. A candidate run never makes the
subsequent strict run optional.

Push, workflow dispatch, publication, tagging, PR creation, and merge are
external actions. They require fresh authorization and are not implied by local
verification. npm publication, tagging, PR creation, merge, and real user-data
migration are outside this closure workflow.

## Actionable no-fallback failures

Launcher errors are nonzero and stable:

- `E_UNSUPPORTED_HOST`: use a supported Windows x64 or Linux x64 package;
- `E_BINARY_MISSING`: reinstall the complete matching package;
- `E_BINARY_UNSAFE`: reject a symlink, directory, or non-regular sibling;
- `E_BINARY_NOT_EXECUTABLE`: restore the packaged Linux executable mode;
- `E_START_FAILED`: verify target compatibility and authentic package bytes;
- `E_SIGNAL_TERMINATION`: investigate the host/runtime termination.

None of these errors starts TypeScript, searches `PATH`, downloads a runtime, or
tries an alternate command.

## Fresh install

Choose the native archive or platform npm tarball for the exact supported
target. Verify its `.sha256` sidecar and `RELEASE-MANIFEST.json`, install into a
new versioned location, then run:

```bash
minimax-codex --version
minimax-codex doctor
```

For native extraction, also verify `node bin/minimax-codex.cjs doctor` reaches
the same Rust binary. For global/npm and one-time `npx` examples, see
[installation, upgrade, and rollback](install-upgrade-rollback.md).

## TypeScript-era state migration

Keep the previous release and all `.mini-codex` source data. Migration is
explicit and source-preserving:

```bash
minimax-codex migrate inventory
minimax-codex migrate dry-run --json
minimax-codex migrate apply --plan <plan> --confirmation <printed-value>
minimax-codex migrate verify --receipt <receipt>
```

Inventory and dry-run write nothing. Review every inclusion, exclusion, and
collision before using the exact printed `MIGRATE:<hash>` confirmation. Apply
stages and validates allowlisted targets, never modifies `.mini-codex`, and
writes an immutable receipt. Credentials, private reasoning, summaries, caches,
locks, databases, and unknown records are excluded.

## Upgrade and rollback

Install upgrades beside the active version. Verify hashes/manifest, run
`--version` and `doctor`, and complete migration verification before switching
the stable command. Binary rollback points the command back to the previous
verified version without touching Vault content, imported files, receipts, or
source data.

Data rollback requires:

```bash
minimax-codex migrate rollback --receipt <receipt> --confirmation ROLLBACK:<receipt-hash>
```

Only unchanged targets marked `created` by that receipt are removed. Reused or
modified targets, the receipt, and `.mini-codex` remain. There is no force path.

## Two-subsequent-public-release support window

Migration fixture retention is machine-checkable in
`fixtures/compat/migration/typescript-v1/support-window.v1.json`:

- cutover release: `3.0.0`;
- minimum: two distinct, ordered subsequent public releases;
- removal eligibility remains false until both releases are recorded and the
  fixture fingerprint still matches.

This rule preserves TypeScript-era data compatibility without retaining an
executable TypeScript product. Fixture removal, when eligible, is a separate
reviewed action; this cutover never modifies real user source data.

## Sandbox truthfulness

Linux confirm-mode process tools require the enforced Bubblewrap-plus-seccomp
backend and run the malicious build-script canary in hosted CI. Windows remains
a supported product/release target, but confirm-mode process diagnostics fail
closed until a native Windows sandbox backend exists. `full-access` explicitly
uses ordinary host access and is appropriate only for trusted projects.

See [Subprocess Sandbox and Network Boundary](subprocess-sandbox.md) for the
complete platform and trust model.
