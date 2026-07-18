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
  createDeterministicTarGzip,
  expectedArchiveEntries,
  loadTargetContract,
  validateArtifactCandidate,
  validateReleaseManifest,
  validateTargetContract
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
  const thresholds = JSON.parse(readFileSync(resolve(root, "fixtures/compat/release/thresholds.v1.json"), "utf8"));
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

function healthyArtifactCandidate() {
  const contract = loadTargetContract();
  const target = contract.targets[0];
  const version = "0.1.0";
  const nativePrefix = `minimax-codex-v${version}-${target.id}/`;
  const nativeEntries = materializeTestEntries(expectedArchiveEntries(target, version, "native"), nativePrefix);
  const npmEntries = materializeTestEntries(expectedArchiveEntries(target, version, "npm"), "package/");
  const nativeBytes = createDeterministicTarGzip(nativeEntries);
  const npmBytes = createDeterministicTarGzip(npmEntries);
  const binaryBytes = fixtureBytes(target.binaryName);
  const launcherBytes = fixtureBytes("bin/minimax-codex.cjs");
  const nativeName = `minimax-codex-v${version}-${target.id}.tar.gz`;
  const npmName = `minimax-codex-v${version}-${target.id}-npm.tgz`;
  const product = {fingerprint: sha256(Buffer.from("current-product", "utf8")), fileCount: 438};
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

function materializeTestEntries(descriptors, prefix) {
  return descriptors.map((descriptor) => descriptor.type === "directory"
    ? {name: descriptor.path, bytes: Buffer.alloc(0), mode: descriptor.mode, type: "5"}
    : {
        name: descriptor.path,
        bytes: fixtureBytes(descriptor.path.slice(prefix.length)),
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
