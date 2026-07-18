import assert from "node:assert/strict";
import {
  chmodSync,
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
  expectedArchiveEntries,
  loadTargetContract,
  validateReleaseManifest,
  validateTargetContract
} from "./package-contract.mjs";

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
    runPackage(binary, first);
    runPackage(binary, second);

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

function assertContractError(operation, expectedCode, label) {
  assert.throws(operation, (error) => {
    assert.equal(error?.code, expectedCode, `${label}: ${error?.message ?? error}`);
    return true;
  });
}

function runPackage(binary, output) {
  const result = spawnSync(
    process.execPath,
    [resolve(root, "scripts/release/package-rust.mjs"), "--binary", binary, "--output", output],
    {
      cwd: root,
      encoding: "utf8",
      env: {...process.env, CARGO_NET_OFFLINE: "true"},
      shell: false,
      windowsHide: true,
      timeout: 30_000
    }
  );
  assert.equal(result.status, 0, result.stderr || result.stdout);
}
