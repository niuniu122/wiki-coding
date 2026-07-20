# Prebuilt npm Distribution Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Publish `minimax-codex` as one deterministic npm package that supported Windows x64 and Linux x64 users can install and run with Node.js 20 alone, without Rust, Cargo, a linker, lifecycle scripts, or install-time downloads.

**Architecture:** Keep the existing per-host Rust build and package verification as the source of native artifacts. Extend the existing release contract and packager to consume one verified Windows package and one verified Linux package, extract only their locked binaries, and assemble one universal npm tarball around the existing CJS launcher. A tag-only GitHub Actions workflow must build, assemble, smoke-install, and then publish the exact same tarball through a protected environment.

**Tech Stack:** Node.js 20 for package contracts and consumer smoke tests; Node.js 24/npm 11.5.1+ for npm trusted publishing; Rust 1.97.0/MSVC and GNU hosted builds; GitHub Actions; npm public registry; existing Rust compatibility harness and JSON source-authority fixtures.

## Global Constraints

- Preserve the approved design in `docs/superpowers/specs/2026-07-20-prebuilt-npm-distribution-design.md`.
- Do not add a second launcher, a runtime download path, a shell fallback, a binary override, or any npm lifecycle script.
- Do not add runtime, optional, peer, bundled, or development dependencies to the published tarball.
- Keep the ordinary `.github/workflows/ci.yml` non-publishing and credential-free.
- Reuse `scripts/release/package-contract.mjs`, `scripts/release/package-rust.mjs`, and `scripts/release/package-contract.test.mjs`; adding another JavaScript release file would widen the exact JavaScript authority allowlist without improving the boundary.
- Treat `linux-x86_64-gnu` and `windows-x86_64-msvc` as the only universal-package inputs. The existing `windows-x86_64-gnullvm-dev` target remains development-only.
- Use `fixtures/compat/release/thresholds.v1.json#baseCompressedBytes` as the maximum compressed universal package size and as the maximum accepted size for each input binary.
- All new failure paths must fail closed with stable categories; tests assert categories rather than full prose.
- During development use candidate verification because product-authority edits intentionally make hosted evidence stale. Refresh hosted candidate/final evidence before a release tag, never by weakening freshness checks.
- Do not create, push, or publish a release tag while implementing this plan. Registry ownership, GitHub environment approval, npm credentials, and the first public publish are operator-authorized steps.

Release-environment assumptions were checked on 2026-07-20 against npm's official [trusted publishing](https://docs.npmjs.com/trusted-publishers/) and [provenance](https://docs.npmjs.com/generating-provenance-statements/) documentation. Recheck those two pages immediately before the first public release because registry authentication requirements can change independently of this repository.

---

## Task 1: Define the Universal npm Archive Contract

**Files:**

- Modify: `scripts/release/package-contract.mjs`
- Modify: `scripts/release/package-contract.test.mjs`
- Read only: `fixtures/compat/release/targets.v1.json`
- Read only: `fixtures/compat/release/thresholds.v1.json`

### Interfaces

Add these exports to the existing contract module:

```js
export function expectedUniversalNpmEntries(version) {}

export function createPublishedPackageJson(sourcePackage, version) {}

export function loadReleaseThresholds(path) {}

export function validateReleaseThresholds(
  thresholds,
  contract = loadTargetContract()
) {}

export function validateUniversalNpmManifest(
  manifest,
  contract = loadTargetContract(),
  thresholds = loadReleaseThresholds()
) {}

export function validateUniversalNpmCandidate({
  manifest,
  contract,
  thresholds,
  expectedProduct,
  archiveBytes,
  checksumBytes
}) {}

export function parseTarGzip(bytes, label) {}

export function sha256(bytes) {}
```

The universal manifest must have one exact schema:

```js
{
  schemaVersion: 1,
  name: "minimax-codex",
  version,
  product: {fingerprint, fileCount},
  launcher: {path, mode, bytes, sha256},
  binaries: [
    {targetId: "linux-x86_64-gnu", path: "minimax-codex", mode: 0o755, bytes, sha256},
    {targetId: "windows-x86_64-msvc", path: "minimax-codex.exe", mode: 0o755, bytes, sha256}
  ],
  npmPackage: {name, bytes, sha256, entries}
}
```

The published `package/package.json` must be generated from an allowlist, not copied from the source package:

```js
{
  name: "minimax-codex",
  version,
  description: sourcePackage.description,
  license: "MIT OR Apache-2.0",
  type: "module",
  bin: {"minimax-codex": "bin/minimax-codex.cjs"},
  engines: {node: ">=20"},
  repository: {
    type: "git",
    url: "git+https://github.com/niuniu122/wiki-coding.git"
  },
  homepage: "https://github.com/niuniu122/wiki-coding#readme",
  bugs: {url: "https://github.com/niuniu122/wiki-coding/issues"},
  publishConfig: {access: "public"}
}
```

It must not contain `scripts`, `dependencies`, `devDependencies`, `optionalDependencies`, `peerDependencies`, or `bundledDependencies`.

### Steps

- [ ] Add a failing test named `universal npm archive has one exact two-platform allowlist` that calls `expectedUniversalNpmEntries("0.1.0")` and asserts the sorted `package/` root, `bin/`, `docs/release/`, launcher, both binaries, README, two licenses, four release documents, and sanitized `package.json` with exact modes.

- [ ] Run `node --test --test-name-pattern="universal npm archive" scripts/release/package-contract.test.mjs` and confirm it fails because `expectedUniversalNpmEntries` is not exported.

- [ ] Implement `expectedUniversalNpmEntries(version)` by reusing the existing `directory`, `file`, version-safety, and deterministic sort helpers. Do not derive entries by scanning the repository.

- [ ] Implement `loadReleaseThresholds` and `validateReleaseThresholds` for the existing `fixtures/compat/release/thresholds.v1.json` exact schema. Bind its `targetContractSchemaVersion` to the target contract and require positive safe integers; do not invent a second npm-only budget.

- [ ] Add a failing table-driven test named `published package metadata excludes install-time code and dependencies` covering a healthy source package plus mutations for every lifecycle script and dependency class. Assert stable `PACKAGE_METADATA_FORBIDDEN` or `PACKAGE_METADATA_IDENTITY` categories.

- [ ] Implement `createPublishedPackageJson(sourcePackage, version)` with `exactObject` checks for nested metadata and an explicit output allowlist. Serialize later with `Buffer.from(`${JSON.stringify(value, null, 2)}\n`)` so the archive is byte-stable.

- [ ] Add failing manifest/candidate tests for unknown fields, target order drift, missing target, development-only target substitution, product-fingerprint mismatch, launcher drift, renamed binary, wrong mode, PE/ELF magic mismatch, checksum mismatch, unexpected executable, symlink/hardlink entry, path traversal, duplicate path, empty binary, and compressed/extracted size overflow.

- [ ] Export the existing `parseTarGzip` and `sha256` helpers instead of duplicating tar or hashing logic. Extend tar parsing only as needed to return safe regular-file and directory entries; continue rejecting every other tar type.

- [ ] Implement `validateUniversalNpmManifest` and `validateUniversalNpmCandidate`. Bind each manifest payload to its exact tar entry, require Windows bytes to begin `4d 5a` (`MZ`), require Linux bytes to begin `7f 45 4c 46` (`ELF`), and enforce the existing compressed size threshold.

- [ ] Run `node --test scripts/release/package-contract.test.mjs`; expected result: all existing platform-package tests and all new universal-package tests pass.

- [ ] Commit the red/green contract slice:

```bash
git add scripts/release/package-contract.mjs scripts/release/package-contract.test.mjs
git commit -m "feat(release): define universal npm package contract"
```

---

## Task 2: Assemble the Universal Package from Verified Host Artifacts

**Files:**

- Modify: `scripts/release/package-rust.mjs`
- Modify: `scripts/release/package-contract.mjs`
- Modify: `scripts/release/package-contract.test.mjs`

### CLI Contract

Keep the existing platform mode unchanged. Add a mutually exclusive universal mode:

```bash
node scripts/release/package-rust.mjs \
  --universal-npm \
  --windows-artifacts target/npm-release-input/windows \
  --linux-artifacts target/npm-release-input/linux \
  --output target/npm-universal \
  --fingerprint-file target/npm-release-input/fingerprint.json \
  --version 0.1.0
```

Universal mode output must contain exactly:

```text
minimax-codex-0.1.0.tgz
minimax-codex-0.1.0.tgz.sha256
minimax-codex-v0.1.0-NPM-MANIFEST.json
```

The successful stdout record must be machine-readable:

```js
{
  schemaVersion: 1,
  mode: "universal-npm",
  manifest: absoluteManifestPath,
  npmArchive: absoluteArchivePath,
  npmSha256,
  productFingerprint,
  version
}
```

### Steps

- [ ] Add a failing subprocess test named `universal packager consumes exactly one verified hosted artifact per OS`. Build synthetic Linux and Windows artifact directories with the existing deterministic fixture helpers and invoke the CLI shown above.

- [ ] Run `node --test --test-name-pattern="universal packager" scripts/release/package-contract.test.mjs` and confirm the current parser rejects `--universal-npm`.

- [ ] Refactor `parseArgs` into two exact schemas. Platform mode continues requiring `--binary`, `--output`, and `--fingerprint-file`; universal mode requires `--universal-npm`, `--windows-artifacts`, `--linux-artifacts`, `--output`, `--fingerprint-file`, and `--version`. Reject mixed, duplicate, absent, or unknown arguments.

- [ ] In universal mode, constrain every input/output path to the repository `target/` directory with `assertWithinTarget`, use `lstatSync` to reject links and non-regular files, and reject any artifact directory whose file set is not the existing manifest plus the two archives and two checksum sidecars.

- [ ] Load the target contract and current product fingerprint. Locate exactly one manifest for `linux-x86_64-gnu` and exactly one for `windows-x86_64-msvc`; call `validateArtifactCandidate` for both before reading a binary from either archive.

- [ ] Require both platform manifests to match the explicit current fingerprint, requested version, launcher bytes/hash/mode, package metadata bytes, README, licenses, and release-document bytes. Reject drift as `UNIVERSAL_INPUT_DRIFT`.

- [ ] Extract only `package/minimax-codex` from the verified Linux npm tarball and `package/minimax-codex.exe` from the verified Windows npm tarball. Do not trust a loose binary copied beside the artifacts.

- [ ] Generate the sanitized publishable `package.json`, materialize `expectedUniversalNpmEntries(version)`, call `createDeterministicTarGzip`, create the universal manifest, and validate the completed candidate before writing any output.

- [ ] Write the archive, checksum sidecar, and external manifest with exclusive filenames. If any intended output already exists, fail instead of overwriting it. Print the exact stdout record above.

- [ ] Expand the subprocess mutation table to cover swapped directories, a gnullvm input, a second manifest, missing sidecar, stale fingerprint, version mismatch, launcher drift, wrong binary magic, unsafe input link, output outside `target/`, pre-existing output, and unknown CLI arguments.

- [ ] Run `node --test scripts/release/package-contract.test.mjs`; expected result: deterministic double assembly produces byte-identical tarballs/manifests and every corrupt input fails before output.

- [ ] Commit the assembly slice:

```bash
git add scripts/release/package-rust.mjs scripts/release/package-contract.mjs scripts/release/package-contract.test.mjs
git commit -m "feat(release): assemble universal npm artifact"
```

---

## Task 3: Make npm Publication Metadata Part of Source Authority

**Files:**

- Modify: `package.json`
- Modify: `package-lock.json`
- Modify: `crates/compat-harness/src/source_authority.rs`
- Modify: `crates/compat-harness/tests/source_authority.rs`
- Modify: `fixtures/compat/source-authority.v1.json`

### Source Metadata

Add only these top-level source-package fields:

```json
{
  "license": "MIT OR Apache-2.0",
  "repository": {
    "type": "git",
    "url": "git+https://github.com/niuniu122/wiki-coding.git"
  },
  "homepage": "https://github.com/niuniu122/wiki-coding#readme",
  "bugs": {
    "url": "https://github.com/niuniu122/wiki-coding/issues"
  },
  "publishConfig": {
    "access": "public"
  }
}
```

Do not add `provenance` to `publishConfig`: trusted publishing generates provenance automatically, while the bootstrap token path passes `--provenance` explicitly.

### Steps

- [ ] Add failing Rust tests that reject a missing or changed license, repository URL, homepage, issue URL, public access setting, package/Cargo version drift, any dependency class, and any lifecycle script. Keep existing exact script and file-list assertions.

- [ ] Update `PACKAGE_TOP_LEVEL_KEYS` to the exact sorted set `bin`, `bugs`, `description`, `engines`, `files`, `homepage`, `license`, `name`, `publishConfig`, `repository`, `scripts`, `type`, `version` and add exact nested-value validation in `validate_package_product_scripts`. Replace the hard-coded `0.1.0` check with `env!("CARGO_PKG_VERSION")` so every future release compares npm metadata with the workspace version compiled into the verifier.

- [ ] Update `package.json` with the five approved metadata fields. Run `npm install --package-lock-only --ignore-scripts` so npm mechanically updates only the root package-lock metadata; inspect the diff and reject any dependency entry.

- [ ] Extend `validate_package_lock` so the dependency-free lock must mirror the approved license metadata while still containing only the root package, bin, engines, license, and version identity that npm actually records.

- [ ] Run `cargo test -p minimax-compat-harness --test source_authority package_ --locked`; expected result before hash refresh: metadata tests pass and the committed source-authority test reports JavaScript hash drift.

- [ ] Recompute SHA-256 for the three modified allowlisted JavaScript files (`package-contract.mjs`, `package-contract.test.mjs`, `package-rust.mjs`) and update only their matching entries in `fixtures/compat/source-authority.v1.json`. Do not change purposes or forbidden capabilities.

- [ ] Run `cargo test -p minimax-compat-harness --test source_authority --locked`; expected result: all source-package and JavaScript-authority tests pass.

- [ ] Run `npm ci --ignore-scripts` followed by `npm run test:package`; expected result: dependency-free install and package corruption suite both pass.

- [ ] Commit the metadata/authority slice:

```bash
git add package.json package-lock.json crates/compat-harness/src/source_authority.rs crates/compat-harness/tests/source_authority.rs fixtures/compat/source-authority.v1.json
git commit -m "feat(release): authorize npm publication metadata"
```

---

## Task 4: Add a Fail-Closed Tag-Only npm Release Workflow

**Files:**

- Create: `.github/workflows/npm-release.yml`
- Modify: `crates/compat-harness/src/source_authority.rs`
- Modify: `crates/compat-harness/tests/source_authority.rs`

### Workflow Shape

Use five jobs representing four logical stages: `preflight` is the first half of the build stage, followed by the host build matrix, universal assembly, consumer smoke matrix, and protected publish.

```yaml
name: npm Release

on:
  push:
    tags:
      - "v*"

permissions:
  contents: read

concurrency:
  group: npm-release-${{ github.ref }}
  cancel-in-progress: false
```

Required dependency graph:

```text
preflight -> build (Windows + Linux) -> assemble -> smoke (Windows + Linux) -> publish
```

### Preflight Requirements

The preflight job must:

1. use `fetch-depth: 0` and fetch `origin/main`;
2. parse `vMAJOR.MINOR.PATCH` without accepting prerelease/build suffixes in this first release channel;
3. prove `GITHUB_SHA` is an ancestor of `origin/main`;
4. compare the tag version with `package.json#version` and `[workspace.package].version` in `Cargo.toml`;
5. query `npm view minimax-codex versions --json` without credentials;
6. treat public-registry `E404` as “no versions yet,” but fail on every other npm/network error;
7. fail when the exact version already exists;
8. expose the validated version as a job output.

### Build and Assembly Requirements

- The build matrix uses Node 20, Rust 1.97.0, the Linux sandbox canary, and Windows `/Brepro`, then runs the existing candidate authority/tests/evaluations/package verification in the same order as ordinary CI.
- Each matrix child uploads the full package-artifact directory, its exact fingerprint JSON, and installed verification evidence. Artifact names are fixed to the target ID.
- `assemble` downloads both artifacts, proves their fingerprint files are byte-identical, invokes universal mode, runs `validateUniversalNpmCandidate`, and uploads only the universal tgz, its sidecar, and its npm manifest.

### Consumer Smoke Requirements

The smoke matrix uses Node 20 and does not install Rust or run Cargo. On both Windows and Linux it must:

```bash
npm install --global --ignore-scripts --prefix target/npm-global ./minimax-codex-<version>.tgz
<installed-command> --version
mkdir -p target/npm-local
cd target/npm-local
npm init --yes
npm install --ignore-scripts --save-dev ../minimax-codex-<version>.tgz
npx --no-install minimax-codex --version
MINIMAX_API_KEY=fixture-no-network <installed-command> doctor --json
```

Assert both version outputs equal `minimax-codex-rust <version>` and `doctor --json` reports `healthy: true`. Run the commands from an isolated empty project directory so `.minimax` state cannot touch the checkout.

### Publish Requirements

The publish job must:

- declare only job-level `permissions: {contents: read, id-token: write}`;
- use GitHub-hosted Ubuntu, environment `npm-production`, Node 24, npm 11.5.1 or newer, and registry `https://registry.npmjs.org`;
- download the universal artifact uploaded before the smoke jobs, not run any pack/build command;
- recompute SHA-256 and compare it with the sidecar and external manifest;
- repeat the exact-version registry absence check immediately before authentication;
- run `npm publish ./minimax-codex-<version>.tgz --dry-run --json --access public` and assert the reported name/version/file set match the external manifest;
- publish the exact tested file using `npm publish ./minimax-codex-<version>.tgz --access public --provenance`;
- expose `NODE_AUTH_TOKEN: ${{ secrets.NPM_TOKEN }}` only in this job for the first-package bootstrap. After the first publish, configure npm trusted publishing for repository `niuniu122/wiki-coding`, workflow `npm-release.yml`, environment `npm-production`, then remove the repository secret; npm will use OIDC and generate provenance automatically.

### Steps

- [ ] Add `pub const NPM_RELEASE_WORKFLOW: &str = ".github/workflows/npm-release.yml"` plus a failing `npm_release_workflow_is_tag_only_ordered_and_secret_isolated` test that loads the future workflow and calls `validate_npm_release_workflow_text`.

- [ ] Add mutation tests that reject branch/PR/manual/release-event triggers, a trigger other than `v*`, removal or loosening of the strict semantic-version preflight parser, missing main ancestry check, version-gate removal, E404-as-any-error handling, existing-version acceptance, top-level write permissions, missing OIDC permission, a secret outside publish, a Rust command in smoke, a lifecycle-enabled install, absent local install, publish-before-smoke, rebuild/repack in publish, a missing/different dry-run path, a different real publish path, missing digest recheck, unprotected publish, and `continue-on-error`.

- [ ] Run `cargo test -p minimax-compat-harness --test source_authority npm_release_workflow --locked`; confirm the initial failure is the absent workflow/validator.

- [ ] Implement `validate_npm_release_workflow_text` as an exact textual authority gate following the style of `validate_ci_workflow_text`. Check one top-level read-only permission block, job order through `needs`, exact commands, action count/order, environment, Node versions, secret placement, and forbidden commands by job section.

- [ ] Create `.github/workflows/npm-release.yml` with the exact job graph and requirements above. Pin GitHub-owned actions to the repository's approved major versions and set `if-no-files-found: error` on every upload.

- [ ] Add the new workflow read/validation call to `validate_source_authority` without weakening the separate ordinary-CI validator.

- [ ] Run `cargo test -p minimax-compat-harness --test source_authority --locked`; expected result: the committed workflow passes and every mutation fails with its intended category.

- [ ] Run a YAML syntax/load check available in the repository environment. If no YAML parser is installed, use GitHub Actions' workflow validation on the branch before merge and record that as a required remote check; do not claim local semantic validation.

- [ ] Commit the workflow slice:

```bash
git add .github/workflows/npm-release.yml crates/compat-harness/src/source_authority.rs crates/compat-harness/tests/source_authority.rs
git commit -m "feat(release): add protected npm publication workflow"
```

---

## Task 5: Document the One-Command Consumer Experience

**Files:**

- Modify: `README.md`
- Modify: `docs/release/install-upgrade-rollback.md`
- Modify: `docs/release/cutover.md`
- Modify: `scripts/release/package-contract.test.mjs`
- Modify: `fixtures/compat/source-authority.v1.json`

### Required User Commands

```bash
# Install globally
npm install --global minimax-codex

# Install in one project
npm install --save-dev minimax-codex
npx minimax-codex

# Upgrade
npm install --global minimax-codex@latest

# Roll back
npm install --global minimax-codex@0.1.0
```

### Steps

- [ ] Add a failing package-contract test named `consumer docs lead with registry install and separate source development`. Assert the README and operator guide contain all four command forms, Node.js 20+, Windows/Linux support, `E_UNSUPPORTED_HOST`, and a separate source-development section naming Rust 1.97.0.

- [ ] Assert the same test rejects instructions that tell consumers to download a platform tgz, run Cargo, compile during npm install, install directly from Git, or depend on `preinstall`/`install`/`postinstall`.

- [ ] Rewrite the README installation section so the first path is `npm install --global minimax-codex`; explain in plain language that the npm package already contains the two compiled binaries and that Rust is required only for contributors building source.

- [ ] Update `docs/release/install-upgrade-rollback.md` with global and project-local installation, version verification, supported platforms, upgrade, explicit-version rollback, uninstall, and stable unsupported-host behavior.

- [ ] Update `docs/release/cutover.md` with the operator sequence: merge to `main`, refresh hosted evidence, verify npm ownership, create protected environment, bootstrap the first publish with a granular token, configure trusted publisher, remove the long-lived token, create/push a version tag, and verify registry install from a clean Windows/Linux machine.

- [ ] Refresh the exact JavaScript hashes for any JavaScript changed after Task 3 in `fixtures/compat/source-authority.v1.json`; do not touch unchanged allowlist entries.

- [ ] Run `node --test scripts/release/package-contract.test.mjs` and `cargo test -p minimax-compat-harness --test source_authority --locked`; expected result: package/docs/source-authority tests pass.

- [ ] Commit the user/operator documentation slice:

```bash
git add README.md docs/release/install-upgrade-rollback.md docs/release/cutover.md scripts/release/package-contract.test.mjs fixtures/compat/source-authority.v1.json
git commit -m "docs: make npm the primary installation path"
```

---

## Task 6: Run End-to-End Gates and Prepare the First Release Handoff

**Files:**

- Modify if generated by the established hosted-evidence workflow: `fixtures/compat/release/hosted-candidate-evidence.v1.json`
- Modify only after the final hosted run required by the existing cutover process: `fixtures/compat/release/hosted-cutover-evidence.v1.json`
- Modify if the verification contract requires new responsibility evidence: `fixtures/compat/verification/typescript-responsibilities.v1.json`
- Verify: all files changed in Tasks 1-5

### Local Candidate Gate

- [ ] Run the dependency-free/package gate:

```bash
npm ci --ignore-scripts
npm run test:package
```

- [ ] Run formatting and linting:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
```

- [ ] Run the hosted-evidence-safe Rust suite and candidate authority verifier:

```bash
cargo test --workspace --locked -- \
  --skip hosted_cutover_evidence_matches_current_product \
  --skip hosted_candidate_evidence_matches_current_product
cargo run -p minimax-compat-harness --locked -- verify-candidate
```

- [ ] Build and verify a Windows candidate locally using the existing platform commands. Do not claim Linux package execution from Windows:

```powershell
node scripts/release/product-fingerprint.mjs > target/npm-plan/fingerprint.json
cargo build -p minimax-cli --release --locked
node scripts/release/package-rust.mjs --binary target/release/minimax-cli.exe --output target/npm-plan/windows --fingerprint-file target/npm-plan/fingerprint.json
node scripts/release/verify-rust-release.mjs --binary target/release/minimax-cli.exe --artifacts target/npm-plan/windows --evidence-dir target/npm-plan/evidence
```

- [ ] Push the implementation branch only with user authorization and require the ordinary Windows/Linux CI matrix plus a non-publishing release-workflow validation run to pass.

### Hosted Evidence and Release Checkpoint

- [ ] Use the existing hosted candidate workflow to refresh candidate evidence for the final product fingerprint. Review the downloaded evidence before committing it; never hand-edit run IDs, conclusions, hashes, or fingerprints.

- [ ] Run the full strict verification after candidate evidence is current:

```bash
npm run verify:release
npm run verify:rust-contracts:strict-precondition
```

- [ ] Complete the repository's established hosted cutover evidence process and run `npm run verify:rust-contracts`. This step is mandatory before a release tag because Tasks 1-5 changed product-authority files.

- [ ] Commit only genuine hosted-evidence updates with the originating workflow URLs recorded by the existing evidence schema:

```bash
git add fixtures/compat/release fixtures/compat/verification
git commit -m "test(release): refresh hosted npm release evidence"
```

- [ ] Stop for operator confirmation. Confirm all of the following before tagging: `minimax-codex` ownership on npm, `npm-production` approval rules, bootstrap token or trusted publisher configuration, default branch `main`, clean worktree, exact `package.json`/Cargo version, and no existing registry version.

- [ ] After explicit release authorization, create and push `v<version>`. Observe every release job; a failure must leave npm `latest` unchanged. Do not retry by reusing the same published version.

- [ ] On successful publication, verify from clean Windows x64 and Linux x64 Node 20 environments:

```bash
npm install --global minimax-codex@<version> --ignore-scripts
minimax-codex --version
minimax-codex doctor --json
```

- [ ] Verify rollback by installing the previous explicit version when one exists. For the first release, record that rollback begins with version two and that failed first publication leaves no `latest` version.

## Acceptance Checklist

- [ ] `npm install --global minimax-codex` is the primary documented installation path.
- [ ] A clean supported consumer needs Node.js 20+ and npm, but no Rust/Cargo/linker.
- [ ] One immutable tarball contains only the approved launcher, Windows/Linux binaries, metadata, docs, README, and licenses.
- [ ] Installation with `--ignore-scripts` works globally and project-locally.
- [ ] The launcher keeps the existing stable failure categories and never downloads or compiles a fallback.
- [ ] The exact tarball smoke-tested on both hosted operating systems is the tarball passed to `npm publish`.
- [ ] Ordinary CI remains non-publishing and credential-free.
- [ ] Tag, package, Cargo, binary, fingerprint, manifest, and registry versions agree before authentication.
- [ ] Publication is tag-only, protected by `npm-production`, and fails closed on any mismatch.
- [ ] Hosted candidate and cutover evidence match the final product fingerprint before the release tag.
