import assert from "node:assert/strict";
import {createHash} from "node:crypto";
import {
  chmodSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync
} from "node:fs";
import test from "node:test";
import {dirname, resolve} from "node:path";
import {spawnSync} from "node:child_process";
import {fileURLToPath} from "node:url";

import {
  createPublishedPackageJson,
  createDeterministicTarGzip,
  expectedArchiveEntries,
  expectedUniversalNpmEntries,
  loadReleaseThresholds,
  loadTargetContract,
  parseTarGzip,
  sha256 as contractSha256,
  validateArtifactCandidate,
  validateReleaseManifest,
  validateReleaseThresholds,
  validateTargetContract,
  validateUniversalNpmCandidate,
  validateUniversalNpmManifest
} from "./package-contract.mjs";
import {computeProductFingerprint} from "./product-fingerprint.mjs";

const HASHES = Object.freeze({
  product: "a".repeat(64),
  binary: "b".repeat(64),
  launcher: "c".repeat(64),
  content: "d".repeat(64),
  archive: "e".repeat(64),
  npm: "f".repeat(64)
});
const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

test("universal npm archive has one exact two-platform allowlist", () => {
  const actual = expectedUniversalNpmEntries("0.1.0");
  const directories = [
    "package/",
    "package/bin/",
    "package/docs/",
    "package/docs/release/"
  ].map((path) => ({path, type: "directory", mode: 0o755}));
  const files = [
    ["package/bin/minimax-codex.cjs", 0o755],
    ["package/minimax-codex", 0o755],
    ["package/minimax-codex.exe", 0o755],
    ["package/README.md", 0o644],
    ["package/LICENSE-APACHE", 0o644],
    ["package/LICENSE-MIT", 0o644],
    ["package/docs/release/cutover.md", 0o644],
    ["package/docs/release/embedding-package.md", 0o644],
    ["package/docs/release/install-upgrade-rollback.md", 0o644],
    ["package/docs/release/subprocess-sandbox.md", 0o644],
    ["package/package.json", 0o644]
  ].map(([path, mode]) => ({path, type: "file", mode}));
  const expected = [...directories, ...files]
    .sort((left, right) => left.path.localeCompare(right.path, "en"));

  assert.deepEqual(actual, expected);
});

test("published package metadata excludes install-time code and dependencies", () => {
  const healthy = healthySourcePackage();
  assert.deepEqual(createPublishedPackageJson(healthy, "0.1.0"), {
    name: "minimax-codex",
    version: "0.1.0",
    description: "A Codex-style interactive CLI shell for MiniMax.",
    license: "MIT OR Apache-2.0",
    type: "module",
    bin: {"minimax-codex": "bin/minimax-codex.cjs"},
    engines: {node: ">=20"},
    repository: {type: "git", url: "git+https://github.com/niuniu122/wiki-coding.git"},
    homepage: "https://github.com/niuniu122/wiki-coding#readme",
    bugs: {url: "https://github.com/niuniu122/wiki-coding/issues"},
    publishConfig: {access: "public"}
  });

  for (const lifecycle of ["preinstall", "install", "postinstall"]) {
    const candidate = structuredClone(healthy);
    candidate.scripts[lifecycle] = "node install.js";
    assertContractError(
      () => createPublishedPackageJson(candidate, "0.1.0"),
      "PACKAGE_METADATA_FORBIDDEN",
      lifecycle
    );
  }
  for (const field of [
    "dependencies",
    "devDependencies",
    "optionalDependencies",
    "peerDependencies",
    "bundledDependencies"
  ]) {
    const candidate = structuredClone(healthy);
    candidate[field] = field === "bundledDependencies" ? [] : {};
    assertContractError(
      () => createPublishedPackageJson(candidate, "0.1.0"),
      "PACKAGE_METADATA_FORBIDDEN",
      field
    );
  }
  for (const [label, mutate] of [
    ["version drift", (value) => value.version = "0.1.1"],
    ["license drift", (value) => value.license = "MIT"],
    ["repository drift", (value) => value.repository.url = "https://example.invalid/repo.git"],
    ["private access", (value) => value.publishConfig.access = "restricted"]
  ]) {
    const candidate = structuredClone(healthy);
    mutate(candidate);
    assertContractError(
      () => createPublishedPackageJson(candidate, "0.1.0"),
      "PACKAGE_METADATA_IDENTITY",
      label
    );
  }
});

test("source package carries exact npm publication metadata", () => {
  const sourcePackage = JSON.parse(readFileSync(resolve(root, "package.json"), "utf8"));
  const published = createPublishedPackageJson(sourcePackage, sourcePackage.version);
  assert.equal(published.license, "MIT OR Apache-2.0");
  assert.deepEqual(published.publishConfig, {access: "public"});
  assert.equal(published.repository.url, "git+https://github.com/niuniu122/wiki-coding.git");
});

test("universal npm manifest and candidate bind both hosted binaries", () => {
  const candidate = healthyUniversalCandidate();
  assert.equal(
    validateUniversalNpmManifest(candidate.manifest, candidate.contract, candidate.thresholds),
    candidate.manifest
  );
  assert.equal(validateUniversalNpmCandidate(candidate.input), candidate.manifest);
  assert.equal(contractSha256(candidate.archiveBytes), candidate.manifest.npmPackage.sha256);
  assert.deepEqual(
    parseTarGzip(candidate.archiveBytes, candidate.manifest.npmPackage.name).map((entry) => entry.name),
    candidate.entries.map((entry) => entry.name).sort((left, right) => left.localeCompare(right, "en"))
  );
});

test("universal npm manifest and candidate reject drift by stable category", () => {
  for (const [label, mutate, code] of [
    ["unknown manifest field", (value) => value.manifest.unexpected = true, "UNIVERSAL_MANIFEST_UNKNOWN_FIELD"],
    ["binary target order", (value) => value.manifest.binaries.reverse(), "UNIVERSAL_TARGET_SET"],
    ["development target", (value) => value.manifest.binaries[1].targetId = "windows-x86_64-gnullvm-dev", "UNIVERSAL_TARGET_SET"],
    ["product fingerprint", (value) => value.expectedProduct.fingerprint = "0".repeat(64), "UNIVERSAL_PRODUCT_FINGERPRINT_MISMATCH"],
    ["launcher mode", (value) => value.manifest.launcher.mode = 0o644, "UNIVERSAL_LAUNCHER_MISMATCH"],
    ["Windows magic", (value) => mutateUniversalEntry(value, "package/minimax-codex.exe", Buffer.from("not-pe\n", "utf8")), "UNIVERSAL_BINARY_MAGIC"],
    ["Linux magic", (value) => mutateUniversalEntry(value, "package/minimax-codex", Buffer.from("not-elf\n", "utf8")), "UNIVERSAL_BINARY_MAGIC"],
    ["binary mode", (value) => setUniversalEntryMode(value, "package/minimax-codex", 0o644), "UNIVERSAL_BINARY_MODE"],
    ["checksum", (value) => value.checksumBytes = Buffer.from(`${"0".repeat(64)}  ${value.manifest.npmPackage.name}\n`, "utf8"), "UNIVERSAL_CHECKSUM_MISMATCH"],
    ["extra executable", (value) => addUniversalEntry(value, {name: "package/bin/alternate", bytes: Buffer.from("alternate\n", "utf8"), mode: 0o755, type: "0"}), "UNIVERSAL_EXTRA_EXECUTABLE"],
    ["renamed Windows binary", (value) => renameUniversalEntry(value, "package/minimax-codex.exe", "package/renamed.exe"), "UNIVERSAL_BINARY_RENAMED"],
    ["unsafe entry type", (value) => setUniversalEntryType(value, "package/minimax-codex", "2"), "ARTIFACT_UNSAFE_TYPE"],
    ["archive size", (value) => value.thresholds = {...value.thresholds, baseCompressedBytes: value.archiveBytes.length - 1}, "UNIVERSAL_SIZE_LIMIT"]
  ]) {
    const candidate = healthyUniversalCandidate();
    mutate(candidate);
    assertContractError(
      () => validateUniversalNpmCandidate(candidate.input ?? universalInput(candidate)),
      code,
      label
    );
  }
});

test("healthy target and release manifest controls pass", () => {
  const contract = loadTargetContract();
  assert.equal(validateTargetContract(contract), contract);
  assert.deepEqual(
    contract.targets.map((target) => [target.id, target.rustcHost, target.supportTier]),
    [
      ["linux-x86_64-gnu", "x86_64-unknown-linux-gnu", "hosted_release"],
      ["windows-x86_64-gnullvm-dev", "x86_64-pc-windows-gnullvm", "development_only"],
      ["windows-x86_64-msvc", "x86_64-pc-windows-msvc", "hosted_release"]
    ]
  );
  assert.equal(contract.targets.filter((target) => target.supportTier === "hosted_release").length, 2);
  assert.doesNotThrow(() => validateReleaseManifest(healthyManifest(contract), contract));
});

test("target contract links the unchanged release threshold budgets", () => {
  const contract = loadTargetContract();
  const thresholds = loadReleaseThresholds();
  assert.equal(validateReleaseThresholds(thresholds, contract), thresholds);
  assert.equal(thresholds.schemaVersion, contract.thresholdSchemaVersion);
  assert.equal(thresholds.targetContractSchemaVersion, contract.schemaVersion);
  assert.deepEqual(
    {
      coldStartMs: thresholds.coldStartMs,
      idleRssBytes: thresholds.idleRssBytes,
      baseCompressedBytes: thresholds.baseCompressedBytes,
      wikiBm25P95Ms: thresholds.wikiBm25P95Ms
    },
    {coldStartMs: 500, idleRssBytes: 157286400, baseCompressedBytes: 52428800, wikiBm25P95Ms: 100}
  );

  for (const [label, mutate, code] of [
    ["unknown field", (value) => value.unexpected = true, "THRESHOLD_SCHEMA_UNKNOWN_FIELD"],
    ["schema drift", (value) => value.schemaVersion = 2, "THRESHOLD_SCHEMA_INVALID"],
    ["target schema drift", (value) => value.targetContractSchemaVersion = 2, "THRESHOLD_CONTRACT_MISMATCH"],
    ["zero package budget", (value) => value.baseCompressedBytes = 0, "THRESHOLD_SCHEMA_INVALID"],
    ["fractional package budget", (value) => value.baseCompressedBytes = 1.5, "THRESHOLD_SCHEMA_INVALID"]
  ]) {
    const candidate = structuredClone(thresholds);
    mutate(candidate);
    assertContractError(() => validateReleaseThresholds(candidate, contract), code, label);
  }
});

test("target contract rejects malformed and tier-confused identities by category", () => {
  const rows = [
    ["unknown root field", (value) => value.unexpected = true, "TARGET_SCHEMA_UNKNOWN_FIELD"],
    ["unknown target field", (value) => value.targets[0].unexpected = true, "TARGET_SCHEMA_UNKNOWN_FIELD"],
    ["unsafe binary name", (value) => value.targets[0].binaryName = "../minimax-codex", "TARGET_UNSAFE_NAME"],
    ["duplicate target", (value) => value.targets[1] = structuredClone(value.targets[0]), "TARGET_DUPLICATE"],
    ["hosted development confusion", (value) => value.targets[0].supportTier = "development_only", "TARGET_TIER_MISMATCH"],
    ["development hosted confusion", (value) => value.targets[1].supportTier = "hosted_release", "TARGET_TIER_MISMATCH"],
    ["unsupported platform", (value) => value.targets[0].id = "darwin-aarch64", "TARGET_IDENTITY_UNSUPPORTED"],
    ["unsupported host", (value) => value.targets[0].rustcHost = "aarch64-unknown-linux-gnu", "TARGET_IDENTITY_UNSUPPORTED"]
  ];
  for (const [label, mutate, code] of rows) {
    const candidate = structuredClone(loadTargetContract());
    mutate(candidate);
    assertContractError(() => validateTargetContract(candidate), code, label);
  }
});

test("release manifest rejects schema path duplicate tier hash platform and embedding drift", () => {
  const contract = loadTargetContract();
  const rows = [
    ["unknown field", (value) => value.unexpected = true, "MANIFEST_SCHEMA_UNKNOWN_FIELD"],
    ["unsafe native name", (value) => value.nativeArchive.name = "../escape.tar.gz", "MANIFEST_UNSAFE_NAME"],
    ["unsafe entry path", (value) => value.nativeArchive.entries[0].path = "../escape", "MANIFEST_UNSAFE_NAME"],
    ["duplicate entry", (value) => value.nativeArchive.entries.push(structuredClone(value.nativeArchive.entries[0])), "MANIFEST_DUPLICATE_ENTRY"],
    ["hosted development confusion", (value) => value.target.supportTier = "development_only", "MANIFEST_TIER_MISMATCH"],
    ["binary hash", (value) => value.binary.sha256 = "bad", "MANIFEST_HASH_INVALID"],
    ["launcher hash", (value) => value.launcher.sha256 = "bad", "MANIFEST_HASH_INVALID"],
    ["npm hash", (value) => value.npmPackage.sha256 = "bad", "MANIFEST_HASH_INVALID"],
    ["archive hash", (value) => value.nativeArchive.sha256 = "bad", "MANIFEST_HASH_INVALID"],
    ["product fingerprint", (value) => value.product.fingerprint = "bad", "MANIFEST_HASH_INVALID"],
    ["unsupported platform", (value) => value.target.id = "darwin-aarch64", "MANIFEST_IDENTITY_UNSUPPORTED"],
    ["unsupported host", (value) => value.target.rustcHost = "aarch64-unknown-linux-gnu", "MANIFEST_IDENTITY_UNSUPPORTED"],
    ["embedding included", (value) => value.embeddingIncluded = true, "MANIFEST_EMBEDDING_INCLUDED"],
    ["missing canonical entry", (value) => value.npmPackage.entries.pop(), "MANIFEST_ENTRY_SET"]
  ];
  for (const [label, mutate, code] of rows) {
    const candidate = healthyManifest(contract);
    mutate(candidate);
    assertContractError(() => validateReleaseManifest(candidate, contract), code, label);
  }
});

test("package assembly is byte-identical and emits one strict external manifest", () => {
  mkdirSync(resolve(root, "target"), {recursive: true});
  const workspace = mkdtempSync(resolve(root, "target/package-contract-test-"));
  try {
    const binary = resolve(workspace, process.platform === "win32" ? "minimax-cli.exe" : "minimax-cli");
    writeFileSync(binary, Buffer.from("synthetic-rust-binary-v1\n", "utf8"));
    if (process.platform !== "win32") chmodSync(binary, 0o755);
    const first = resolve(workspace, "first");
    const second = resolve(workspace, "second");
    const fingerprintFile = resolve(workspace, "fingerprint.json");
    writeFileSync(fingerprintFile, `${JSON.stringify(computeProductFingerprint(root))}\n`, "utf8");
    runPackage(binary, first, fingerprintFile);
    runPackage(binary, second, fingerprintFile);

    const contract = loadTargetContract();
    const rustc = spawnSync("rustc", ["-vV"], {cwd: root, encoding: "utf8", shell: false, windowsHide: true});
    assert.equal(rustc.status, 0, rustc.stderr);
    const host = /^host:\s*(.+)$/mu.exec(rustc.stdout)?.[1]?.trim();
    const target = contract.targets.find((candidate) => candidate.rustcHost === host);
    assert.ok(target, `unsupported test rustc host: ${host}`);
    const base = `minimax-codex-v0.1.0-${target.id}`;
    const manifestName = `${base}-RELEASE-MANIFEST.json`;
    const expectedFiles = [
      `${base}.tar.gz`,
      `${base}.tar.gz.sha256`,
      `${base}-npm.tgz`,
      `${base}-npm.tgz.sha256`,
      manifestName
    ].sort();
    assert.deepEqual(readdirSync(first).sort(), expectedFiles);
    assert.deepEqual(readdirSync(second).sort(), expectedFiles);
    for (const name of expectedFiles) {
      assert.deepEqual(readFileSync(resolve(first, name)), readFileSync(resolve(second, name)), name);
    }
    validateReleaseManifest(
      JSON.parse(readFileSync(resolve(first, manifestName), "utf8")),
      contract
    );
  } finally {
    rmSync(workspace, {recursive: true, force: true});
  }
});

test("universal packager consumes exactly one verified hosted artifact per OS", () => {
  mkdirSync(resolve(root, "target"), {recursive: true});
  const workspace = mkdtempSync(resolve(root, "target/universal-packager-test-"));
  try {
    const currentProduct = computeProductFingerprint(root);
    const linuxArtifacts = resolve(workspace, "linux");
    const windowsArtifacts = resolve(workspace, "windows");
    const firstOutput = resolve(workspace, "first");
    const secondOutput = resolve(workspace, "second");
    const fingerprintFile = resolve(workspace, "fingerprint.json");
    writeFileSync(fingerprintFile, `${JSON.stringify(currentProduct)}\n`, "utf8");
    writeArtifactCandidateDirectory(
      linuxArtifacts,
      healthyArtifactCandidate("linux-x86_64-gnu", currentProduct)
    );
    writeArtifactCandidateDirectory(
      windowsArtifacts,
      healthyArtifactCandidate("windows-x86_64-msvc", currentProduct)
    );

    const first = runUniversalPackage(
      windowsArtifacts,
      linuxArtifacts,
      firstOutput,
      fingerprintFile
    );
    const second = runUniversalPackage(
      windowsArtifacts,
      linuxArtifacts,
      secondOutput,
      fingerprintFile
    );
    const firstRecord = JSON.parse(first.stdout);
    assert.deepEqual(
      {
        schemaVersion: firstRecord.schemaVersion,
        mode: firstRecord.mode,
        productFingerprint: firstRecord.productFingerprint,
        version: firstRecord.version
      },
      {
        schemaVersion: 1,
        mode: "universal-npm",
        productFingerprint: currentProduct.fingerprint,
        version: "0.1.0"
      }
    );

    const expectedFiles = [
      "minimax-codex-0.1.0.tgz",
      "minimax-codex-0.1.0.tgz.sha256",
      "minimax-codex-v0.1.0-NPM-MANIFEST.json"
    ];
    assert.deepEqual(readdirSync(firstOutput).sort(), expectedFiles);
    assert.deepEqual(readdirSync(secondOutput).sort(), expectedFiles);
    for (const name of expectedFiles) {
      assert.deepEqual(
        readFileSync(resolve(firstOutput, name)),
        readFileSync(resolve(secondOutput, name)),
        name
      );
    }
    const manifest = JSON.parse(
      readFileSync(resolve(firstOutput, "minimax-codex-v0.1.0-NPM-MANIFEST.json"), "utf8")
    );
    validateUniversalNpmCandidate({
      manifest,
      contract: loadTargetContract(),
      thresholds: loadReleaseThresholds(),
      expectedProduct: currentProduct,
      archiveBytes: readFileSync(resolve(firstOutput, "minimax-codex-0.1.0.tgz")),
      checksumBytes: readFileSync(resolve(firstOutput, "minimax-codex-0.1.0.tgz.sha256"))
    });
  } finally {
    rmSync(workspace, {recursive: true, force: true});
  }
});

test("universal packager rejects unsafe and mismatched inputs before output", async (t) => {
  for (const [label, mutate, expected] of [
    ["swapped platform directories", (fixture) => {
      const windows = fixture.args[2];
      fixture.args[2] = fixture.args[4];
      fixture.args[4] = windows;
    }, "ARTIFACT_TARGET_MISMATCH"],
    ["development-only Windows target", (fixture) => {
      rmSync(fixture.windowsArtifacts, {recursive: true, force: true});
      writeArtifactCandidateDirectory(
        fixture.windowsArtifacts,
        healthyArtifactCandidate("windows-x86_64-gnullvm-dev", fixture.currentProduct)
      );
    }, "ARTIFACT_TARGET_MISMATCH"],
    ["second release manifest", (fixture) => {
      writeFileSync(
        resolve(fixture.windowsArtifacts, "extra-RELEASE-MANIFEST.json"),
        "{}\n",
        "utf8"
      );
    }, "UNIVERSAL_INPUT_FILE_SET"],
    ["missing checksum sidecar", (fixture) => {
      const sidecarName = readdirSync(fixture.linuxArtifacts).find((name) => name.endsWith(".sha256"));
      assert.ok(sidecarName);
      rmSync(resolve(fixture.linuxArtifacts, sidecarName));
    }, "ARTIFACT_FILE_SET"],
    ["stale fingerprint", (fixture) => {
      writeFileSync(
        fixture.fingerprintFile,
        `${JSON.stringify({...fixture.currentProduct, fingerprint: "0".repeat(64)})}\n`,
        "utf8"
      );
    }, "E_FINGERPRINT_STALE"],
    ["version mismatch", (fixture) => fixture.args[fixture.args.length - 1] = "0.1.1", "UNIVERSAL_INPUT_VERSION"],
    ["wrong Windows binary magic", (fixture) => {
      rmSync(fixture.windowsArtifacts, {recursive: true, force: true});
      writeArtifactCandidateDirectory(
        fixture.windowsArtifacts,
        healthyArtifactCandidate(
          "windows-x86_64-msvc",
          fixture.currentProduct,
          Buffer.from("not-pe\n", "utf8")
        )
      );
    }, "UNIVERSAL_BINARY_MAGIC"],
    ["launcher drift", (fixture) => {
      rmSync(fixture.windowsArtifacts, {recursive: true, force: true});
      const candidate = healthyArtifactCandidate("windows-x86_64-msvc", fixture.currentProduct);
      mutatePlatformSharedContent(
        candidate,
        "bin/minimax-codex.cjs",
        Buffer.from("#!/usr/bin/env node\nlauncher drift\n", "utf8")
      );
      writeArtifactCandidateDirectory(fixture.windowsArtifacts, candidate);
    }, "UNIVERSAL_INPUT_DRIFT"],
    ["missing artifact directory", (fixture) => fixture.args[2] = resolve(fixture.workspace, "missing-windows"), "UNIVERSAL_INPUT_UNSAFE"],
    ["output outside target", (fixture) => fixture.args[6] = resolve(root, "outside-universal-output-test"), "must stay inside"],
    ["non-empty output directory", (fixture) => {
      mkdirSync(fixture.output, {recursive: true});
      writeFileSync(resolve(fixture.output, "unexpected.txt"), "occupied\n", "utf8");
    }, "UNIVERSAL_OUTPUT_NOT_EMPTY"],
    ["pre-existing archive output", (fixture) => {
      mkdirSync(fixture.output, {recursive: true});
      writeFileSync(resolve(fixture.output, "minimax-codex-0.1.0.tgz"), "occupied\n", "utf8");
    }, "UNIVERSAL_OUTPUT_EXISTS"],
    ["unknown argument", (fixture) => fixture.args.push("--surprise", "value"), "invalid package argument"]
  ]) {
    await t.test(label, () => {
      const fixture = setupUniversalPackageInputs();
      try {
        mutate(fixture);
        const result = spawnReleaseScript("package-rust.mjs", fixture.args);
        assert.notEqual(result.status, 0, label);
        assert.match(result.stderr, new RegExp(expected, "u"), label);
        if (!expected.startsWith("UNIVERSAL_OUTPUT")) {
          assert.equal(existsSync(fixture.output), false, `${label}: output must not be created`);
        }
      } finally {
        rmSync(fixture.workspace, {recursive: true, force: true});
      }
    });
  }
});

test("corrupt package candidates fail closed by stable category before any alternate child", async (t) => {
  mkdirSync(resolve(root, "target"), {recursive: true});
  const workspace = mkdtempSync(resolve(root, "target/package-corruption-test-"));
  const alternateChildMarker = resolve(workspace, "alternate-child.marker");
  try {
    const rows = [
      ["absent binary", (candidate) => removeNativeBinary(candidate), "ARTIFACT_BINARY_MISSING"],
      ["wrong target", (candidate) => candidate.input.expectedTargetId = "windows-x86_64-msvc", "ARTIFACT_TARGET_MISMATCH"],
      ["renamed binary", (candidate) => renameNativeBinary(candidate), "ARTIFACT_BINARY_RENAMED"],
      ["Linux executable bit lost", (candidate) => setNativeBinaryMode(candidate, 0o644), "ARTIFACT_BINARY_NOT_EXECUTABLE"],
      ["symlink or unsafe entry type", (candidate) => setNativeBinaryType(candidate, "2"), "ARTIFACT_UNSAFE_TYPE"],
      ["checksum mismatch", (candidate) => mutateNativeReadme(candidate), "ARTIFACT_CHECKSUM_MISMATCH"],
      ["product fingerprint drift", (candidate) => candidate.input.expectedProduct.fingerprint = "0".repeat(64), "ARTIFACT_PRODUCT_FINGERPRINT_MISMATCH"],
      ["launcher mismatch", (candidate) => mutateNativeLauncher(candidate), "ARTIFACT_LAUNCHER_MISMATCH"],
      ["extra executable", (candidate) => addNativeExecutable(candidate), "ARTIFACT_EXTRA_EXECUTABLE"],
      ["truncated or corrupt archive", (candidate) => truncateNativeArchive(candidate), "ARTIFACT_ARCHIVE_CORRUPT"],
      ["invalid checksum sidecar", (candidate) => invalidateNativeSidecar(candidate), "ARTIFACT_SIDECAR_INVALID"]
    ];

    for (const [label, mutate, code] of rows) {
      await t.test(label, () => {
        const candidate = healthyArtifactCandidate();
        mutate(candidate);
        assertContractError(() => validateArtifactCandidate(candidate.input), code, label);
        assert.equal(existsSync(alternateChildMarker), false, `${label}: no alternate child may run`);
      });
    }
  } finally {
    rmSync(workspace, {recursive: true, force: true});
  }
});

test("package.json exposes the package-only corruption gate", () => {
  const packageJson = JSON.parse(readFileSync(resolve(root, "package.json"), "utf8"));
  assert.equal(packageJson.scripts["test:package"], "node --test scripts/release/package-contract.test.mjs");
});

test("npm release workflow is tag-only ordered and secret-isolated", () => {
  const source = readFileSync(resolve(root, ".github/workflows/npm-release.yml"), "utf8")
    .replaceAll("\r\n", "\n");
  assert.match(source, /on:\n  push:\n    tags:\n      - "v\*"/u);
  assert.doesNotMatch(source, /pull_request:|workflow_dispatch:|release:/u);
  assert.match(source, /permissions:\n  contents: read/u);
  const preflight = source.indexOf("  preflight:");
  const build = source.indexOf("  build:");
  const assemble = source.indexOf("  assemble:");
  const smoke = source.indexOf("  smoke:");
  const publish = source.indexOf("  publish:");
  assert.ok(preflight < build && build < assemble && assemble < smoke && smoke < publish);
  const smokeText = source.slice(smoke, publish);
  const buildText = source.slice(build, assemble);
  assert.match(buildText, /npm run verify:rust-contracts\n/u);
  assert.match(smokeText, /--global --ignore-scripts/u);
  assert.match(smokeText, /npx --no-install minimax-codex --version/u);
  assert.doesNotMatch(smokeText, /cargo|rustup|rustc/u);
  assert.equal(source.match(/secrets\.NPM_TOKEN/gu)?.length, 1);
  assert.match(source.slice(publish), /environment: npm-production/u);
  assert.match(source.slice(publish), /id-token: write/u);
  assert.match(source.slice(publish), /npm install --global npm@11\.5\.1/u);
  assert.match(source.slice(publish), /manifest\.npmPackage\.sha256/u);
  assert.match(source.slice(publish), /npm publish "\$ARCHIVE" --dry-run --json --access public/u);
  assert.match(
    source.slice(publish),
    /const actual = dryRun\.files\s+\.filter\(\(entry\) => entry\.path !== "" && !entry\.path\.endsWith\("\/"\)\)\s+\.map\(\(entry\) => entry\.path\)\.sort\(\);/u
  );
  assert.match(source.slice(publish), /npm publish "\$ARCHIVE" --access public --provenance/u);
});

test("consumer docs lead with registry installs and keep Rust in source development", () => {
  const readme = readFileSync(resolve(root, "README.md"), "utf8");
  const installGuide = readFileSync(
    resolve(root, "docs/release/install-upgrade-rollback.md"),
    "utf8"
  );
  const cutover = readFileSync(resolve(root, "docs/release/cutover.md"), "utf8");
  const consumerText = `${readme}\n${installGuide}\n${cutover}`;

  assert.match(readme, /npm install --global minimax-codex/u);
  assert.match(readme, /npm install --save-dev minimax-codex/u);
  assert.match(readme, /npx minimax-codex --version/u);
  assert.match(readme, /Node\.js 20 or newer/u);
  assert.match(installGuide, /npm install --global minimax-codex@latest/u);
  assert.match(installGuide, /npm install --global minimax-codex@<previous-version>/u);
  assert.match(installGuide, /npm uninstall --global minimax-codex/u);
  assert.match(cutover, /one universal npm package/u);
  assert.match(cutover, /verify npm ownership[\s\S]*npm-production[\s\S]*granular token[\s\S]*trusted publisher[\s\S]*remove the `NPM_TOKEN` secret/iu);
  assert.match(consumerText, /Windows x64/u);
  assert.match(consumerText, /Linux x64/u);
  assert.match(consumerText, /E_UNSUPPORTED_HOST/u);
  assert.match(readme, /## Source development and release verification[\s\S]*Rust 1\.97\.0/u);
  assert.doesNotMatch(
    readme.slice(readme.indexOf("## Install and run"), readme.indexOf("## Permission and subprocess boundaries")),
    /cargo|rustup|Rust 1\.97\.0|<target>-npm\.tgz|github\.com\/.+\.git|preinstall|postinstall/u
  );
});

test("release CLIs require one explicit current fingerprint and bind artifacts to it", () => {
  mkdirSync(resolve(root, "target"), {recursive: true});
  const workspace = mkdtempSync(resolve(root, "target/fingerprint-contract-test-"));
  try {
    const binary = resolve(workspace, process.platform === "win32" ? "minimax-cli.exe" : "minimax-cli");
    const artifacts = resolve(workspace, "artifacts");
    const evidence = resolve(workspace, "evidence");
    const fingerprintFile = resolve(workspace, "fingerprint.json");
    const malformedFile = resolve(workspace, "malformed.json");
    const staleFile = resolve(workspace, "stale.json");
    const current = computeProductFingerprint(root);
    writeFileSync(binary, Buffer.from("synthetic-rust-binary-v2\n", "utf8"));
    if (process.platform !== "win32") chmodSync(binary, 0o755);
    writeFileSync(fingerprintFile, `${JSON.stringify(current)}\n`, "utf8");
    writeFileSync(malformedFile, "{\n", "utf8");
    writeFileSync(staleFile, `${JSON.stringify({...current, fingerprint: "0".repeat(64)})}\n`, "utf8");

    const packaged = runPackage(binary, artifacts, fingerprintFile);
    const packageOutput = JSON.parse(packaged.stdout);
    assert.equal(packageOutput.productFingerprint, current.fingerprint);
    assert.equal(packageOutput.fingerprintFile.replaceAll("\\", "/"), fingerprintFile.replaceAll("\\", "/"));

    for (const [label, values, code] of [
      ["missing package fingerprint", ["--binary", binary, "--output", resolve(workspace, "missing")], "E_FINGERPRINT_REQUIRED"],
      ["malformed package fingerprint", ["--binary", binary, "--output", resolve(workspace, "malformed"), "--fingerprint-file", malformedFile], "E_FINGERPRINT_INVALID"],
      ["stale package fingerprint", ["--binary", binary, "--output", resolve(workspace, "stale"), "--fingerprint-file", staleFile], "E_FINGERPRINT_STALE"]
    ]) {
      const result = spawnReleaseScript("package-rust.mjs", values);
      assert.notEqual(result.status, 0, label);
      assert.match(result.stderr, new RegExp(code, "u"), label);
    }

    const missingMilestone = spawnReleaseScript("verify-milestone-flow.mjs", [
      "--artifacts", artifacts,
      "--evidence-dir", evidence
    ]);
    assert.notEqual(missingMilestone.status, 0);
    assert.match(missingMilestone.stderr, /E_FINGERPRINT_REQUIRED/u);

    const manifestName = readdirSync(artifacts).find((name) => name.endsWith("-RELEASE-MANIFEST.json"));
    assert.ok(manifestName);
    const manifestPath = resolve(artifacts, manifestName);
    const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
    manifest.product.fingerprint = "1".repeat(64);
    writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`, "utf8");
    const mismatch = spawnReleaseScript("verify-milestone-flow.mjs", [
      "--artifacts", artifacts,
      "--evidence-dir", evidence,
      "--fingerprint-file", fingerprintFile
    ]);
    assert.notEqual(mismatch.status, 0);
    assert.match(mismatch.stderr, /E_FINGERPRINT_ARTIFACT_MISMATCH/u);
  } finally {
    rmSync(workspace, {recursive: true, force: true});
  }
});

function healthySourcePackage() {
  return {
    name: "minimax-codex",
    version: "0.1.0",
    description: "A Codex-style interactive CLI shell for MiniMax.",
    license: "MIT OR Apache-2.0",
    type: "module",
    bin: {"minimax-codex": "bin/minimax-codex.cjs"},
    engines: {node: ">=20"},
    repository: {type: "git", url: "git+https://github.com/niuniu122/wiki-coding.git"},
    homepage: "https://github.com/niuniu122/wiki-coding#readme",
    bugs: {url: "https://github.com/niuniu122/wiki-coding/issues"},
    publishConfig: {access: "public"},
    files: [],
    scripts: {"test:package": "node --test scripts/release/package-contract.test.mjs"}
  };
}

function healthyUniversalCandidate() {
  const contract = loadTargetContract();
  const thresholds = loadReleaseThresholds();
  const version = "0.1.0";
  const linuxBytes = Buffer.concat([
    Buffer.from([0x7f, 0x45, 0x4c, 0x46]),
    Buffer.from("synthetic-linux-binary\n", "utf8")
  ]);
  const windowsBytes = Buffer.concat([
    Buffer.from([0x4d, 0x5a]),
    Buffer.from("synthetic-windows-binary\n", "utf8")
  ]);
  const launcherBytes = Buffer.from("#!/usr/bin/env node\nfixture launcher\n", "utf8");
  const packageJsonBytes = Buffer.from(
    `${JSON.stringify(createPublishedPackageJson(healthySourcePackage(), version), null, 2)}\n`,
    "utf8"
  );
  const sourceBytes = new Map([
    ["package/bin/minimax-codex.cjs", launcherBytes],
    ["package/minimax-codex", linuxBytes],
    ["package/minimax-codex.exe", windowsBytes],
    ["package/package.json", packageJsonBytes]
  ]);
  const entries = expectedUniversalNpmEntries(version).map((descriptor) => descriptor.type === "directory"
    ? {name: descriptor.path, bytes: Buffer.alloc(0), mode: descriptor.mode, type: "5"}
    : {
        name: descriptor.path,
        bytes: sourceBytes.get(descriptor.path) ?? fixtureBytes(descriptor.path.slice("package/".length)),
        mode: descriptor.mode,
        type: "0"
      });
  const archiveBytes = createDeterministicTarGzip(entries);
  const archiveName = `minimax-codex-${version}.tgz`;
  const product = {fingerprint: sha256(Buffer.from("universal-product", "utf8")), fileCount: 440};
  const manifest = {
    schemaVersion: 1,
    name: "minimax-codex",
    version,
    product: {...product},
    launcher: {
      path: "bin/minimax-codex.cjs",
      mode: 0o755,
      bytes: launcherBytes.length,
      sha256: sha256(launcherBytes)
    },
    binaries: [
      {
        targetId: "linux-x86_64-gnu",
        path: "minimax-codex",
        mode: 0o755,
        bytes: linuxBytes.length,
        sha256: sha256(linuxBytes)
      },
      {
        targetId: "windows-x86_64-msvc",
        path: "minimax-codex.exe",
        mode: 0o755,
        bytes: windowsBytes.length,
        sha256: sha256(windowsBytes)
      }
    ],
    npmPackage: {
      name: archiveName,
      bytes: archiveBytes.length,
      sha256: sha256(archiveBytes),
      entries: testEntryEvidence(entries)
    }
  };
  const candidate = {
    manifest,
    contract,
    thresholds,
    expectedProduct: {...product},
    entries,
    archiveBytes,
    checksumBytes: sidecar(archiveName, archiveBytes)
  };
  Object.defineProperty(candidate, "input", {
    enumerable: true,
    get() {
      return universalInput(candidate);
    }
  });
  return candidate;
}

function universalInput(candidate) {
  return {
    manifest: candidate.manifest,
    contract: candidate.contract,
    thresholds: candidate.thresholds,
    expectedProduct: candidate.expectedProduct,
    archiveBytes: candidate.archiveBytes,
    checksumBytes: candidate.checksumBytes
  };
}

function rebuildUniversalArchive(candidate) {
  candidate.archiveBytes = createDeterministicTarGzip(candidate.entries);
  candidate.manifest.npmPackage.bytes = candidate.archiveBytes.length;
  candidate.manifest.npmPackage.sha256 = sha256(candidate.archiveBytes);
  candidate.checksumBytes = sidecar(candidate.manifest.npmPackage.name, candidate.archiveBytes);
}

function universalEntry(candidate, path) {
  const entry = candidate.entries.find((value) => value.name === path);
  assert.ok(entry, `expected universal entry ${path}`);
  return entry;
}

function mutateUniversalEntry(candidate, path, bytes) {
  universalEntry(candidate, path).bytes = bytes;
  rebuildUniversalArchive(candidate);
}

function setUniversalEntryMode(candidate, path, mode) {
  universalEntry(candidate, path).mode = mode;
  rebuildUniversalArchive(candidate);
}

function setUniversalEntryType(candidate, path, type) {
  universalEntry(candidate, path).type = type;
  rebuildUniversalArchive(candidate);
}

function addUniversalEntry(candidate, entry) {
  candidate.entries.push(entry);
  rebuildUniversalArchive(candidate);
}

function renameUniversalEntry(candidate, path, renamed) {
  universalEntry(candidate, path).name = renamed;
  rebuildUniversalArchive(candidate);
}

function healthyManifest(contract) {
  const target = contract.targets[0];
  const version = "0.1.0";
  const nativeEntries = evidenceEntries(expectedArchiveEntries(target, version, "native"));
  const npmEntries = evidenceEntries(expectedArchiveEntries(target, version, "npm"));
  setContentEvidence(nativeEntries, `minimax-codex-v${version}-${target.id}/${target.binaryName}`, HASHES.binary, 1024);
  setContentEvidence(nativeEntries, `minimax-codex-v${version}-${target.id}/bin/minimax-codex.cjs`, HASHES.launcher, 512);
  setContentEvidence(npmEntries, `package/${target.binaryName}`, HASHES.binary, 1024);
  setContentEvidence(npmEntries, "package/bin/minimax-codex.cjs", HASHES.launcher, 512);
  for (const [index, relative] of [
    "README.md",
    "LICENSE-APACHE",
    "LICENSE-MIT",
    "docs/release/cutover.md",
    "docs/release/embedding-package.md",
    "docs/release/install-upgrade-rollback.md",
    "docs/release/subprocess-sandbox.md"
  ].entries()) {
    setContentEvidence(nativeEntries, `minimax-codex-v${version}-${target.id}/${relative}`, HASHES.content, 100 + index);
    setContentEvidence(npmEntries, `package/${relative}`, HASHES.content, 100 + index);
  }
  return {
    schemaVersion: 2,
    name: "minimax-codex",
    version,
    target: {
      id: target.id,
      rustcHost: target.rustcHost,
      os: target.os,
      arch: target.arch,
      supportTier: target.supportTier
    },
    embeddingIncluded: false,
    product: {fingerprint: HASHES.product, fileCount: 420},
    binary: {path: target.binaryName, mode: target.binaryMode, bytes: 1024, sha256: HASHES.binary},
    launcher: {path: "bin/minimax-codex.cjs", mode: 493, bytes: 512, sha256: HASHES.launcher},
    nativeArchive: {
      name: `minimax-codex-v${version}-${target.id}${target.archiveSuffix}`,
      bytes: 2048,
      sha256: HASHES.archive,
      entries: nativeEntries
    },
    npmPackage: {
      name: `minimax-codex-v${version}-${target.id}-npm.tgz`,
      bytes: 1900,
      sha256: HASHES.npm,
      entries: npmEntries
    }
  };
}

function evidenceEntries(entries) {
  return entries.map((entry, index) => entry.type === "directory"
    ? {...entry}
    : {...entry, bytes: index + 1, sha256: HASHES.content});
}

function setContentEvidence(entries, path, hash, bytes) {
  const entry = entries.find((candidate) => candidate.path === path);
  assert.ok(entry, `expected canonical entry ${path}`);
  entry.sha256 = hash;
  entry.bytes = bytes;
}

function healthyArtifactCandidate(
  targetId = "linux-x86_64-gnu",
  expectedProduct = {fingerprint: sha256(Buffer.from("current-product", "utf8")), fileCount: 438},
  binaryOverride
) {
  const contract = loadTargetContract();
  const target = contract.targets.find((candidate) => candidate.id === targetId);
  assert.ok(target, `unknown artifact test target ${targetId}`);
  const version = "0.1.0";
  const nativePrefix = `minimax-codex-v${version}-${target.id}/`;
  const overrides = binaryOverride ? new Map([[target.binaryName, binaryOverride]]) : new Map();
  const nativeEntries = materializeTestEntries(
    expectedArchiveEntries(target, version, "native"),
    nativePrefix,
    overrides
  );
  const npmEntries = materializeTestEntries(
    expectedArchiveEntries(target, version, "npm"),
    "package/",
    overrides
  );
  const nativeBytes = createDeterministicTarGzip(nativeEntries);
  const npmBytes = createDeterministicTarGzip(npmEntries);
  const binaryBytes = overrides.get(target.binaryName) ?? fixtureBytes(target.binaryName);
  const launcherBytes = fixtureBytes("bin/minimax-codex.cjs");
  const nativeName = `minimax-codex-v${version}-${target.id}.tar.gz`;
  const npmName = `minimax-codex-v${version}-${target.id}-npm.tgz`;
  const product = {
    fingerprint: expectedProduct.fingerprint,
    fileCount: expectedProduct.fileCount
  };
  const manifest = {
    schemaVersion: contract.manifestSchemaVersion,
    name: "minimax-codex",
    version,
    target: {
      id: target.id,
      rustcHost: target.rustcHost,
      os: target.os,
      arch: target.arch,
      supportTier: target.supportTier
    },
    embeddingIncluded: false,
    product: {...product},
    binary: {path: target.binaryName, mode: target.binaryMode, bytes: binaryBytes.length, sha256: sha256(binaryBytes)},
    launcher: {path: "bin/minimax-codex.cjs", mode: 0o755, bytes: launcherBytes.length, sha256: sha256(launcherBytes)},
    nativeArchive: {
      name: nativeName,
      bytes: nativeBytes.length,
      sha256: sha256(nativeBytes),
      entries: testEntryEvidence(nativeEntries)
    },
    npmPackage: {
      name: npmName,
      bytes: npmBytes.length,
      sha256: sha256(npmBytes),
      entries: testEntryEvidence(npmEntries)
    }
  };
  validateReleaseManifest(manifest, contract);
  const artifacts = new Map([
    [nativeName, {kind: "file", bytes: nativeBytes}],
    [`${nativeName}.sha256`, {kind: "file", bytes: sidecar(nativeName, nativeBytes)}],
    [npmName, {kind: "file", bytes: npmBytes}],
    [`${npmName}.sha256`, {kind: "file", bytes: sidecar(npmName, npmBytes)}]
  ]);
  return {
    input: {
      manifest,
      contract,
      expectedTargetId: target.id,
      expectedProduct: {...product},
      artifacts
    },
    nativeEntries,
    npmEntries
  };
}

function materializeTestEntries(descriptors, prefix, overrides = new Map()) {
  return descriptors.map((descriptor) => descriptor.type === "directory"
    ? {name: descriptor.path, bytes: Buffer.alloc(0), mode: descriptor.mode, type: "5"}
    : {
        name: descriptor.path,
        bytes: overrides.get(descriptor.path.slice(prefix.length))
          ?? fixtureBytes(descriptor.path.slice(prefix.length)),
        mode: descriptor.mode,
        type: "0"
      });
}

function testEntryEvidence(entries) {
  return entries.map((entry) => entry.type === "5"
    ? {path: entry.name, type: "directory", mode: entry.mode}
    : {path: entry.name, type: "file", mode: entry.mode, bytes: entry.bytes.length, sha256: sha256(entry.bytes)});
}

function fixtureBytes(relativePath) {
  if (relativePath === "minimax-codex") {
    return Buffer.concat([Buffer.from([0x7f, 0x45, 0x4c, 0x46]), Buffer.from("fixture-linux\n", "utf8")]);
  }
  if (relativePath === "minimax-codex.exe") {
    return Buffer.concat([Buffer.from([0x4d, 0x5a]), Buffer.from("fixture-windows\n", "utf8")]);
  }
  if ([
    "bin/minimax-codex.cjs",
    "package.json",
    "README.md",
    "LICENSE-APACHE",
    "LICENSE-MIT",
    "docs/release/cutover.md",
    "docs/release/embedding-package.md",
    "docs/release/install-upgrade-rollback.md",
    "docs/release/subprocess-sandbox.md"
  ].includes(relativePath)) {
    return readFileSync(resolve(root, relativePath));
  }
  return Buffer.from(`fixture:${relativePath}\n`, "utf8");
}

function sidecar(name, bytes) {
  return Buffer.from(`${sha256(bytes)}  ${name}\n`, "utf8");
}

function nativeBinaryEntry(candidate) {
  const manifest = candidate.input.manifest;
  const path = `minimax-codex-v${manifest.version}-${manifest.target.id}/${manifest.binary.path}`;
  return candidate.nativeEntries.find((entry) => entry.name === path);
}

function nativeEntry(candidate, suffix) {
  return candidate.nativeEntries.find((entry) => entry.name.endsWith(suffix));
}

function rebuildNative(candidate) {
  const bytes = createDeterministicTarGzip(candidate.nativeEntries);
  const name = candidate.input.manifest.nativeArchive.name;
  candidate.input.artifacts.set(name, {kind: "file", bytes});
  candidate.input.artifacts.set(`${name}.sha256`, {kind: "file", bytes: sidecar(name, bytes)});
}

function mutatePlatformSharedContent(candidate, relativePath, bytes) {
  const manifest = candidate.input.manifest;
  const nativePath = `minimax-codex-v${manifest.version}-${manifest.target.id}/${relativePath}`;
  const npmPath = `package/${relativePath}`;
  const native = candidate.nativeEntries.find((entry) => entry.name === nativePath);
  const npm = candidate.npmEntries.find((entry) => entry.name === npmPath);
  assert.ok(native, `expected platform native entry ${nativePath}`);
  assert.ok(npm, `expected platform npm entry ${npmPath}`);
  native.bytes = bytes;
  npm.bytes = bytes;
  if (relativePath === "bin/minimax-codex.cjs") {
    manifest.launcher.bytes = bytes.length;
    manifest.launcher.sha256 = sha256(bytes);
  }
  const nativeBytes = createDeterministicTarGzip(candidate.nativeEntries);
  const npmBytes = createDeterministicTarGzip(candidate.npmEntries);
  manifest.nativeArchive.bytes = nativeBytes.length;
  manifest.nativeArchive.sha256 = sha256(nativeBytes);
  manifest.nativeArchive.entries = testEntryEvidence(candidate.nativeEntries);
  manifest.npmPackage.bytes = npmBytes.length;
  manifest.npmPackage.sha256 = sha256(npmBytes);
  manifest.npmPackage.entries = testEntryEvidence(candidate.npmEntries);
  candidate.input.artifacts.set(manifest.nativeArchive.name, {kind: "file", bytes: nativeBytes});
  candidate.input.artifacts.set(
    `${manifest.nativeArchive.name}.sha256`,
    {kind: "file", bytes: sidecar(manifest.nativeArchive.name, nativeBytes)}
  );
  candidate.input.artifacts.set(manifest.npmPackage.name, {kind: "file", bytes: npmBytes});
  candidate.input.artifacts.set(
    `${manifest.npmPackage.name}.sha256`,
    {kind: "file", bytes: sidecar(manifest.npmPackage.name, npmBytes)}
  );
  validateReleaseManifest(manifest, candidate.input.contract);
}

function removeNativeBinary(candidate) {
  const entry = nativeBinaryEntry(candidate);
  candidate.nativeEntries.splice(candidate.nativeEntries.indexOf(entry), 1);
  rebuildNative(candidate);
}

function renameNativeBinary(candidate) {
  nativeBinaryEntry(candidate).name = nativeBinaryEntry(candidate).name.replace(/minimax-codex$/u, "renamed-minimax-codex");
  rebuildNative(candidate);
}

function setNativeBinaryMode(candidate, mode) {
  nativeBinaryEntry(candidate).mode = mode;
  rebuildNative(candidate);
}

function setNativeBinaryType(candidate, type) {
  nativeBinaryEntry(candidate).type = type;
  rebuildNative(candidate);
}

function mutateNativeReadme(candidate) {
  nativeEntry(candidate, "/README.md").bytes = Buffer.from("tampered README\n", "utf8");
  rebuildNative(candidate);
}

function mutateNativeLauncher(candidate) {
  nativeEntry(candidate, "/bin/minimax-codex.cjs").bytes = Buffer.from("tampered launcher\n", "utf8");
  rebuildNative(candidate);
}

function addNativeExecutable(candidate) {
  const rootName = `minimax-codex-v${candidate.input.manifest.version}-${candidate.input.manifest.target.id}`;
  candidate.nativeEntries.push({name: `${rootName}/bin/alternate-runtime`, bytes: Buffer.from("alternate\n", "utf8"), mode: 0o755, type: "0"});
  rebuildNative(candidate);
}

function truncateNativeArchive(candidate) {
  const name = candidate.input.manifest.nativeArchive.name;
  const artifact = candidate.input.artifacts.get(name);
  candidate.input.artifacts.set(name, {...artifact, bytes: artifact.bytes.subarray(0, 12)});
}

function invalidateNativeSidecar(candidate) {
  const name = candidate.input.manifest.nativeArchive.name;
  candidate.input.artifacts.set(`${name}.sha256`, {kind: "file", bytes: Buffer.from("not-a-sidecar\n", "utf8")});
}

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function assertContractError(operation, expectedCode, label) {
  assert.throws(operation, (error) => {
    assert.equal(error?.code, expectedCode, `${label}: ${error?.message ?? error}`);
    return true;
  });
}

function runPackage(binary, output, fingerprintFile) {
  const result = spawnReleaseScript("package-rust.mjs", [
    "--binary", binary,
    "--output", output,
    "--fingerprint-file", fingerprintFile
  ]);
  assert.equal(result.status, 0, result.stderr || result.stdout);
  return result;
}

function writeArtifactCandidateDirectory(directory, candidate) {
  mkdirSync(directory, {recursive: true});
  for (const [name, artifact] of candidate.input.artifacts) {
    writeFileSync(resolve(directory, name), artifact.bytes);
  }
  const manifest = candidate.input.manifest;
  const manifestName = `minimax-codex-v${manifest.version}-${manifest.target.id}-RELEASE-MANIFEST.json`;
  writeFileSync(resolve(directory, manifestName), `${JSON.stringify(manifest, null, 2)}\n`, "utf8");
}

function runUniversalPackage(windowsArtifacts, linuxArtifacts, output, fingerprintFile) {
  const result = spawnReleaseScript("package-rust.mjs", [
    "--universal-npm",
    "--windows-artifacts", windowsArtifacts,
    "--linux-artifacts", linuxArtifacts,
    "--output", output,
    "--fingerprint-file", fingerprintFile,
    "--version", "0.1.0"
  ]);
  assert.equal(result.status, 0, result.stderr || result.stdout);
  return result;
}

function setupUniversalPackageInputs() {
  mkdirSync(resolve(root, "target"), {recursive: true});
  const workspace = mkdtempSync(resolve(root, "target/universal-packager-corruption-test-"));
  const currentProduct = computeProductFingerprint(root);
  const windowsArtifacts = resolve(workspace, "windows");
  const linuxArtifacts = resolve(workspace, "linux");
  const output = resolve(workspace, "output");
  const fingerprintFile = resolve(workspace, "fingerprint.json");
  writeFileSync(fingerprintFile, `${JSON.stringify(currentProduct)}\n`, "utf8");
  writeArtifactCandidateDirectory(
    windowsArtifacts,
    healthyArtifactCandidate("windows-x86_64-msvc", currentProduct)
  );
  writeArtifactCandidateDirectory(
    linuxArtifacts,
    healthyArtifactCandidate("linux-x86_64-gnu", currentProduct)
  );
  return {
    workspace,
    currentProduct,
    windowsArtifacts,
    linuxArtifacts,
    output,
    fingerprintFile,
    args: [
      "--universal-npm",
      "--windows-artifacts", windowsArtifacts,
      "--linux-artifacts", linuxArtifacts,
      "--output", output,
      "--fingerprint-file", fingerprintFile,
      "--version", "0.1.0"
    ]
  };
}

function spawnReleaseScript(name, values) {
  return spawnSync(process.execPath, [resolve(root, "scripts/release", name), ...values], {
    cwd: root,
    encoding: "utf8",
    env: {...process.env, CARGO_NET_OFFLINE: "true"},
    shell: false,
    windowsHide: true,
    timeout: 30_000
  });
}
