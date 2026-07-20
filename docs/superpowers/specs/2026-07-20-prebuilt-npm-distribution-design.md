# Prebuilt npm Distribution Design

**Status:** Approved in conversation on 2026-07-20

**Product:** `minimax-codex`

**Distribution channel:** npm only

## Problem

The repository can build and verify platform release archives, but consumers
cannot install a published product. Git-based installation gives them source,
the native binaries are not tracked, and no npm package is published. A clean
consumer therefore needs the Rust toolchain to produce the CLI before using it.

## Goal

Make the supported CLI installable with one ordinary command:

```bash
npm install --global minimax-codex
```

The installed command must run without Cargo, `rustc`, a C/C++ linker, an
install-time download, or any local compilation. Node.js 20 or newer remains the
only prerequisite because npm installs and invokes the thin CJS launcher.

Project-local installation must also work:

```bash
npm install --save-dev minimax-codex
npx minimax-codex
```

## Scope

The first published package supports exactly the product's existing hosted
targets:

- Windows x64 (`win32/x64`, Rust target `x86_64-pc-windows-msvc`);
- Linux x64 (`linux/x64`, Rust target `x86_64-unknown-linux-gnu`).

macOS, Arm, native installers, GitHub Release assets, automatic updates, and
installing the product directly from a Git repository are outside this design.
Unsupported hosts must fail with the existing stable `E_UNSUPPORTED_HOST`
category instead of compiling or downloading a fallback.

## Package Architecture

Publish one universal npm package named `minimax-codex`. Each version contains:

- `bin/minimax-codex.cjs`;
- `minimax-codex.exe`, built on the hosted Windows x64 runner;
- `minimax-codex`, built on the hosted Linux x64 runner;
- the release documentation, README, and licenses;
- package metadata required by npm.

The existing launcher selects one fixed sibling binary from
`process.platform` and `process.arch`. It must never search `PATH`, honor a
binary override, invoke a shell, download a binary, or fall back to source.

The package has no runtime dependency and no lifecycle script (`preinstall`,
`install`, or `postinstall`). Installation with `--ignore-scripts` must remain
fully functional. Including both supported binaries makes the download larger
than a platform-split package, but avoids optional-package resolution, package
scope ownership, install-time network logic, and two-package version drift.

## Version Authority

A release tag uses `vMAJOR.MINOR.PATCH`. Before building, the workflow must
prove that:

1. the tag's version equals `package.json#version`;
2. the tag's version equals `[workspace.package].version` in `Cargo.toml`;
3. the tagged commit is contained in the repository's default `main` branch;
4. the built CLI reports the same version;
5. the npm registry does not already contain that exact version.

Any mismatch stops before npm authentication or publication. Published npm
versions are immutable and are never overwritten.

## Release Pipeline

Add a dedicated tag-triggered release workflow with four isolated stages:

1. **Build matrix** — Windows and Linux jobs run the existing Rust checks, build
   the locked release binary, validate the product fingerprint, and upload the
   exact native binary plus target evidence as temporary CI artifacts.
2. **Assemble** — a non-publishing job downloads both verified binaries and
   produces one deterministic universal npm tarball. It emits a package manifest
   and SHA-256 digest and rejects unexpected files.
3. **Installed smoke matrix** — clean Windows and Linux jobs download the same
   tarball, install it into an isolated npm prefix using `--ignore-scripts`, and
   run `minimax-codex --version` and `minimax-codex doctor`. These jobs do not
   build the workspace or invoke Rust tooling.
4. **Publish** — only after both smoke jobs pass, a protected
   `npm-production` environment publishes that exact tested tarball. The job
   receives npm credentials; build, assembly, and smoke jobs do not.

The initial npm owner/package setup and the `npm-production` environment secret
are explicit operator prerequisites. The repository stores no npm credential.
The publish job uses the least-privilege npm token available to the project and
GitHub's OIDC permission for npm provenance.

## Package Contract and Failure Handling

The package contract must reject:

- absent, empty, linked, or incorrectly formatted native binaries;
- a Windows binary without PE magic or a Linux binary without ELF magic;
- a Linux binary without executable mode;
- source trees, Cargo output, caches, credentials, fixtures, planning files, or
  unapproved executable content;
- package metadata containing lifecycle scripts or runtime dependencies;
- launcher, binary, manifest, fingerprint, tag, or version drift;
- a package whose compressed or extracted size exceeds the declared release
  limits.

At runtime, the launcher retains the current stable error categories:
`E_UNSUPPORTED_HOST`, `E_BINARY_MISSING`, `E_BINARY_UNSAFE`,
`E_BINARY_NOT_EXECUTABLE`, `E_START_FAILED`, and `E_SIGNAL_TERMINATION`. None of
these errors may trigger compilation or a network fallback.

## Verification

Automated coverage must include:

- deterministic assembly from one Windows and one Linux binary fixture;
- exact allowlist and mode checks for the npm tarball;
- corrupt, missing, linked, wrong-platform, and oversized binary rejection;
- lifecycle-script and runtime-dependency rejection;
- launcher target selection and stable failure categories;
- global-style and project-local installation from the packed tarball with
  `--ignore-scripts`;
- installed `--version` and `doctor` smoke tests on Windows and Linux;
- tag/package/Cargo/runtime version mismatch rejection;
- a publish dry run that proves the tested tarball, rather than a rebuilt copy,
  is the publication input.

The existing ordinary CI remains non-publishing. Pull requests and branch pushes
may build and test package candidates, but only a valid version tag reaching the
protected release workflow can publish.

## Upgrade and Rollback

Users upgrade with:

```bash
npm install --global minimax-codex@latest
```

They roll back by installing an explicit previously published version:

```bash
npm install --global minimax-codex@0.1.0
```

A failed release leaves the previous npm version and `latest` tag unchanged.
Moving `latest` occurs only as part of the successful final publish operation.

## Acceptance Criteria

The design is complete when:

- a supported user can install and launch the CLI with npm and Node.js alone;
- no consumer installation or startup path invokes or downloads Rust tooling;
- one immutable npm tarball contains and selects both supported native binaries;
- the exact tarball tested on Windows and Linux is the tarball published;
- unsupported platforms and corrupt packages fail explicitly without fallback;
- npm publication is isolated behind a protected, tag-only workflow;
- documentation distinguishes product installation from source development.
