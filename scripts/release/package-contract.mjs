import {createHash} from "node:crypto";
import {existsSync, lstatSync, readFileSync} from "node:fs";
import {dirname, resolve} from "node:path";
import {fileURLToPath} from "node:url";
import {gunzipSync, gzipSync} from "node:zlib";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const defaultTargetContract = resolve(root, "fixtures/compat/release/targets.v1.json");
const defaultReleaseThresholds = resolve(root, "fixtures/compat/release/thresholds.v1.json");
const directoryMode = 0o755;
const fileMode = 0o644;
const launcherMode = 0o755;
const releaseDocs = Object.freeze([
  "cutover.md",
  "embedding-package.md",
  "install-upgrade-rollback.md",
  "subprocess-sandbox.md"
]);
const canonicalTargets = Object.freeze([
  Object.freeze({
    id: "linux-x86_64-gnu",
    rustcHost: "x86_64-unknown-linux-gnu",
    os: "linux",
    arch: "x64",
    binaryName: "minimax-codex",
    binaryMode: directoryMode,
    archiveSuffix: ".tar.gz",
    supportTier: "hosted_release"
  }),
  Object.freeze({
    id: "windows-x86_64-gnullvm-dev",
    rustcHost: "x86_64-pc-windows-gnullvm",
    os: "win32",
    arch: "x64",
    binaryName: "minimax-codex.exe",
    binaryMode: directoryMode,
    archiveSuffix: ".tar.gz",
    supportTier: "development_only"
  }),
  Object.freeze({
    id: "windows-x86_64-msvc",
    rustcHost: "x86_64-pc-windows-msvc",
    os: "win32",
    arch: "x64",
    binaryName: "minimax-codex.exe",
    binaryMode: directoryMode,
    archiveSuffix: ".tar.gz",
    supportTier: "hosted_release"
  })
]);

export class PackageContractError extends Error {
  constructor(code, message) {
    super(`${code}: ${message}`);
    this.name = "PackageContractError";
    this.code = code;
  }
}

export function loadTargetContract(path = defaultTargetContract) {
  let value;
  try {
    value = JSON.parse(readFileSync(path, "utf8"));
  } catch (error) {
    contractFail("TARGET_SCHEMA_INVALID", `cannot read target contract: ${error instanceof Error ? error.message : String(error)}`);
  }
  return validateTargetContract(value);
}

export function validateTargetContract(contract) {
  exactObject(contract, ["schemaVersion", "manifestSchemaVersion", "thresholdSchemaVersion", "targets"], "TARGET_SCHEMA_UNKNOWN_FIELD", "target contract");
  if (contract.schemaVersion !== 1 || contract.manifestSchemaVersion !== 2 || contract.thresholdSchemaVersion !== 1) {
    contractFail("TARGET_SCHEMA_INVALID", "target, manifest, and threshold schema versions must remain 1, 2, and 1");
  }
  if (!Array.isArray(contract.targets) || contract.targets.length !== canonicalTargets.length) {
    contractFail("TARGET_IDENTITY_UNSUPPORTED", "target contract must contain exactly three locked identities");
  }
  for (const target of contract.targets) validateTargetFields(target);
  const ids = contract.targets.map((target) => target.id);
  const hosts = contract.targets.map((target) => target.rustcHost);
  if (new Set(ids).size !== ids.length || new Set(hosts).size !== hosts.length) {
    contractFail("TARGET_DUPLICATE", "target ids and rustc hosts must be duplicate-free");
  }
  for (const target of contract.targets) {
    const canonical = canonicalTargets.find((candidate) => candidate.id === target.id);
    if (!canonical) contractFail("TARGET_IDENTITY_UNSUPPORTED", `unsupported target id: ${target.id}`);
    if (target.supportTier !== canonical.supportTier) {
      contractFail("TARGET_TIER_MISMATCH", `${target.id} cannot use ${target.supportTier}`);
    }
    if (!sameJson(target, canonical)) {
      contractFail("TARGET_IDENTITY_UNSUPPORTED", `target identity drifted: ${target.id}`);
    }
  }
  if (!sameJson(contract.targets, canonicalTargets)) {
    contractFail("TARGET_IDENTITY_UNSUPPORTED", "targets must remain in canonical id order");
  }
  return contract;
}

export function loadReleaseThresholds(path = defaultReleaseThresholds) {
  let value;
  try {
    value = JSON.parse(readFileSync(path, "utf8"));
  } catch (error) {
    contractFail("THRESHOLD_SCHEMA_INVALID", `cannot read release thresholds: ${error instanceof Error ? error.message : String(error)}`);
  }
  return validateReleaseThresholds(value);
}

export function validateReleaseThresholds(thresholds, contract = loadTargetContract()) {
  validateTargetContract(contract);
  exactObject(
    thresholds,
    ["schemaVersion", "targetContractSchemaVersion", "coldStartMs", "idleRssBytes", "baseCompressedBytes", "wikiBm25P95Ms"],
    "THRESHOLD_SCHEMA_UNKNOWN_FIELD",
    "release thresholds"
  );
  if (thresholds.schemaVersion !== contract.thresholdSchemaVersion) {
    contractFail("THRESHOLD_SCHEMA_INVALID", "release threshold schema version is invalid");
  }
  if (thresholds.targetContractSchemaVersion !== contract.schemaVersion) {
    contractFail("THRESHOLD_CONTRACT_MISMATCH", "release thresholds do not bind the target contract schema");
  }
  for (const field of ["coldStartMs", "idleRssBytes", "baseCompressedBytes", "wikiBm25P95Ms"]) {
    if (!Number.isSafeInteger(thresholds[field]) || thresholds[field] <= 0) {
      contractFail("THRESHOLD_SCHEMA_INVALID", `${field} must be a positive safe integer`);
    }
  }
  return thresholds;
}

export function expectedArchiveEntries(target, version, channel) {
  const canonical = canonicalTargets.find((candidate) => candidate.id === target?.id);
  if (!canonical || !sameJson(target, canonical)) {
    contractFail("TARGET_IDENTITY_UNSUPPORTED", "archive entries require an exact locked target");
  }
  if (!/^[0-9A-Za-z][0-9A-Za-z.+-]{0,63}$/u.test(version)) {
    contractFail("MANIFEST_UNSAFE_NAME", "release version is not archive-safe");
  }
  if (channel !== "native" && channel !== "npm") {
    contractFail("MANIFEST_SCHEMA_INVALID", `unknown archive channel: ${channel}`);
  }
  const prefix = channel === "native" ? `minimax-codex-v${version}-${target.id}` : "package";
  const entries = [
    directory(`${prefix}/`),
    directory(`${prefix}/bin/`),
    directory(`${prefix}/docs/`),
    directory(`${prefix}/docs/release/`),
    file(`${prefix}/bin/minimax-codex.cjs`, launcherMode),
    file(`${prefix}/${target.binaryName}`, target.binaryMode),
    file(`${prefix}/README.md`, fileMode),
    file(`${prefix}/LICENSE-APACHE`, fileMode),
    file(`${prefix}/LICENSE-MIT`, fileMode),
    ...releaseDocs.map((name) => file(`${prefix}/docs/release/${name}`, fileMode))
  ];
  if (channel === "npm") entries.push(file(`${prefix}/package.json`, fileMode));
  return entries.sort((left, right) => left.path.localeCompare(right.path, "en"));
}

export function expectedUniversalNpmEntries(version) {
  if (!/^[0-9A-Za-z][0-9A-Za-z.+-]{0,63}$/u.test(version)) {
    contractFail("MANIFEST_UNSAFE_NAME", "release version is not archive-safe");
  }
  return [
    directory("package/"),
    directory("package/bin/"),
    directory("package/docs/"),
    directory("package/docs/release/"),
    file("package/bin/minimax-codex.cjs", launcherMode),
    file("package/minimax-codex", launcherMode),
    file("package/minimax-codex.exe", launcherMode),
    file("package/README.md", fileMode),
    file("package/LICENSE-APACHE", fileMode),
    file("package/LICENSE-MIT", fileMode),
    ...releaseDocs.map((name) => file(`package/docs/release/${name}`, fileMode)),
    file("package/package.json", fileMode)
  ].sort((left, right) => left.path.localeCompare(right.path, "en"));
}

export function createPublishedPackageJson(sourcePackage, version) {
  if (!isPlainObject(sourcePackage)
      || !/^[0-9A-Za-z][0-9A-Za-z.+-]{0,63}$/u.test(version)
      || sourcePackage.name !== "minimax-codex"
      || sourcePackage.version !== version
      || typeof sourcePackage.description !== "string"
      || sourcePackage.description.length === 0
      || sourcePackage.license !== "MIT OR Apache-2.0"
      || sourcePackage.type !== "module"
      || !sameJson(sourcePackage.bin, {"minimax-codex": "bin/minimax-codex.cjs"})
      || !sameJson(sourcePackage.engines, {node: ">=20"})
      || !sameJson(sourcePackage.repository, {
        type: "git",
        url: "git+https://github.com/niuniu122/wiki-coding.git"
      })
      || sourcePackage.homepage !== "https://github.com/niuniu122/wiki-coding#readme"
      || !sameJson(sourcePackage.bugs, {url: "https://github.com/niuniu122/wiki-coding/issues"})
      || !sameJson(sourcePackage.publishConfig, {access: "public"})
      || Object.hasOwn(sourcePackage, "private")) {
    contractFail("PACKAGE_METADATA_IDENTITY", "source package publication identity is invalid");
  }

  const dependencyFields = [
    "dependencies",
    "devDependencies",
    "optionalDependencies",
    "peerDependencies",
    "bundledDependencies"
  ];
  if (dependencyFields.some((field) => Object.hasOwn(sourcePackage, field))) {
    contractFail("PACKAGE_METADATA_FORBIDDEN", "source package must not declare dependency classes");
  }
  if (!isPlainObject(sourcePackage.scripts)
      || ["preinstall", "install", "postinstall"].some((name) => Object.hasOwn(sourcePackage.scripts, name))) {
    contractFail("PACKAGE_METADATA_FORBIDDEN", "source package must not declare npm lifecycle scripts");
  }

  return {
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
  };
}

export function validateReleaseManifest(manifest, contract = loadTargetContract()) {
  validateTargetContract(contract);
  exactObject(
    manifest,
    ["schemaVersion", "name", "version", "target", "embeddingIncluded", "product", "binary", "launcher", "nativeArchive", "npmPackage"],
    "MANIFEST_SCHEMA_UNKNOWN_FIELD",
    "release manifest"
  );
  if (manifest.schemaVersion !== contract.manifestSchemaVersion || manifest.name !== "minimax-codex") {
    contractFail("MANIFEST_SCHEMA_INVALID", "release manifest identity or schema version is invalid");
  }
  if (!/^[0-9A-Za-z][0-9A-Za-z.+-]{0,63}$/u.test(manifest.version)) {
    contractFail("MANIFEST_UNSAFE_NAME", "release version is not archive-safe");
  }
  validateManifestTarget(manifest.target, contract);
  if (manifest.embeddingIncluded !== false) {
    contractFail("MANIFEST_EMBEDDING_INCLUDED", "base artifacts must not include embedding resources");
  }
  validateProduct(manifest.product);
  validatePayload(manifest.binary, ["path", "mode", "bytes", "sha256"], "binary");
  validatePayload(manifest.launcher, ["path", "mode", "bytes", "sha256"], "launcher");
  const target = contract.targets.find((candidate) => candidate.id === manifest.target.id);
  if (manifest.binary.path !== target.binaryName || manifest.binary.mode !== target.binaryMode) {
    contractFail("MANIFEST_IDENTITY_UNSUPPORTED", "binary path or mode does not match the target");
  }
  if (manifest.launcher.path !== "bin/minimax-codex.cjs" || manifest.launcher.mode !== launcherMode) {
    contractFail("MANIFEST_IDENTITY_UNSUPPORTED", "launcher path or mode is invalid");
  }

  const nativeName = `minimax-codex-v${manifest.version}-${target.id}${target.archiveSuffix}`;
  const npmName = `minimax-codex-v${manifest.version}-${target.id}-npm.tgz`;
  validateArchive(manifest.nativeArchive, nativeName, expectedArchiveEntries(target, manifest.version, "native"), "native");
  validateArchive(manifest.npmPackage, npmName, expectedArchiveEntries(target, manifest.version, "npm"), "npm");

  const nativeRoot = `minimax-codex-v${manifest.version}-${target.id}`;
  bindPayload(manifest.nativeArchive.entries, `${nativeRoot}/${manifest.binary.path}`, manifest.binary, "binary");
  bindPayload(manifest.nativeArchive.entries, `${nativeRoot}/${manifest.launcher.path}`, manifest.launcher, "launcher");
  bindPayload(manifest.npmPackage.entries, `package/${manifest.binary.path}`, manifest.binary, "binary");
  bindPayload(manifest.npmPackage.entries, `package/${manifest.launcher.path}`, manifest.launcher, "launcher");
  bindSharedContent(manifest.nativeArchive.entries, nativeRoot, manifest.npmPackage.entries);
  return manifest;
}

export function validateUniversalNpmManifest(
  manifest,
  contract = loadTargetContract(),
  thresholds = loadReleaseThresholds()
) {
  validateTargetContract(contract);
  validateReleaseThresholds(thresholds, contract);
  exactObject(
    manifest,
    ["schemaVersion", "name", "version", "product", "launcher", "binaries", "npmPackage"],
    "UNIVERSAL_MANIFEST_UNKNOWN_FIELD",
    "universal npm manifest"
  );
  if (manifest.schemaVersion !== 1
      || manifest.name !== "minimax-codex"
      || !/^[0-9A-Za-z][0-9A-Za-z.+-]{0,63}$/u.test(manifest.version)) {
    contractFail("UNIVERSAL_MANIFEST_INVALID", "universal npm manifest identity is invalid");
  }
  validateProduct(manifest.product);
  validateUniversalPayload(manifest.launcher, ["path", "mode", "bytes", "sha256"], "launcher");
  if (manifest.launcher.path !== "bin/minimax-codex.cjs" || manifest.launcher.mode !== launcherMode) {
    contractFail("UNIVERSAL_LAUNCHER_MISMATCH", "universal npm launcher identity is invalid");
  }

  if (!Array.isArray(manifest.binaries) || manifest.binaries.length !== 2) {
    contractFail("UNIVERSAL_TARGET_SET", "universal npm package requires exactly two hosted binaries");
  }
  const expectedBinaries = [
    {targetId: "linux-x86_64-gnu", path: "minimax-codex"},
    {targetId: "windows-x86_64-msvc", path: "minimax-codex.exe"}
  ];
  for (let index = 0; index < expectedBinaries.length; index += 1) {
    const binary = manifest.binaries[index];
    const expected = expectedBinaries[index];
    validateUniversalPayload(binary, ["targetId", "path", "mode", "bytes", "sha256"], "binary");
    const target = contract.targets.find((candidate) => candidate.id === binary.targetId);
    if (!target
        || target.supportTier !== "hosted_release"
        || binary.targetId !== expected.targetId
        || binary.path !== expected.path
        || binary.mode !== launcherMode) {
      contractFail("UNIVERSAL_TARGET_SET", "universal npm binary targets must remain Linux GNU then Windows MSVC");
    }
    if (binary.bytes > thresholds.baseCompressedBytes) {
      contractFail("UNIVERSAL_SIZE_LIMIT", "universal npm binary exceeds the release size limit");
    }
  }

  validateArchive(
    manifest.npmPackage,
    `minimax-codex-${manifest.version}.tgz`,
    expectedUniversalNpmEntries(manifest.version),
    "universal npm"
  );
  bindPayload(manifest.npmPackage.entries, `package/${manifest.launcher.path}`, manifest.launcher, "universal launcher");
  for (const binary of manifest.binaries) {
    bindPayload(manifest.npmPackage.entries, `package/${binary.path}`, binary, `universal ${binary.targetId} binary`);
  }
  const extractedBytes = manifest.npmPackage.entries
    .filter((entry) => entry.type === "file")
    .reduce((total, entry) => total + entry.bytes, 0);
  if (manifest.npmPackage.bytes > thresholds.baseCompressedBytes
      || extractedBytes > thresholds.baseCompressedBytes) {
    contractFail("UNIVERSAL_SIZE_LIMIT", "universal npm package exceeds the release size limit");
  }
  return manifest;
}

export function validateUniversalNpmCandidate({
  manifest,
  contract = loadTargetContract(),
  thresholds = loadReleaseThresholds(),
  expectedProduct,
  archiveBytes,
  checksumBytes
}) {
  if (!isPlainObject(manifest)
      || !isPlainObject(expectedProduct)
      || !Buffer.isBuffer(archiveBytes)
      || !Buffer.isBuffer(checksumBytes)) {
    contractFail("UNIVERSAL_CANDIDATE_INVALID", "universal npm candidate inputs are malformed");
  }
  if (manifest.product?.fingerprint !== expectedProduct.fingerprint
      || manifest.product?.fileCount !== expectedProduct.fileCount) {
    contractFail("UNIVERSAL_PRODUCT_FINGERPRINT_MISMATCH", "universal npm candidate product fingerprint is stale");
  }
  validateUniversalNpmManifest(manifest, contract, thresholds);

  const entries = parseTarGzip(archiveBytes, manifest.npmPackage.name);
  const expectedPaths = new Set(manifest.npmPackage.entries.map((entry) => entry.path));
  const binaryRecords = manifest.binaries.map((binary) => ({
    binary,
    entry: entries.find((entry) => entry.name === `package/${binary.path}`)
  }));
  for (const record of binaryRecords) {
    if (!record.entry) {
      const renamed = entries.find((entry) => entry.type === "0"
        && sha256(entry.bytes) === record.binary.sha256
        && entry.mode === record.binary.mode);
      if (renamed) {
        contractFail("UNIVERSAL_BINARY_RENAMED", `universal binary was renamed to ${renamed.name}`);
      }
      contractFail("UNIVERSAL_BINARY_MISSING", `universal npm archive is missing package/${record.binary.path}`);
    }
    if (record.entry.type !== "0" || record.entry.mode !== launcherMode) {
      contractFail("UNIVERSAL_BINARY_MODE", `${record.binary.targetId} binary must be a regular executable entry`);
    }
    const expectedMagic = record.binary.targetId === "linux-x86_64-gnu"
      ? Buffer.from([0x7f, 0x45, 0x4c, 0x46])
      : Buffer.from([0x4d, 0x5a]);
    if (record.entry.bytes.length < expectedMagic.length
        || !record.entry.bytes.subarray(0, expectedMagic.length).equals(expectedMagic)) {
      contractFail("UNIVERSAL_BINARY_MAGIC", `${record.binary.targetId} binary magic is invalid`);
    }
  }

  const unexpectedExecutable = entries.find((entry) => !expectedPaths.has(entry.name)
    && entry.type === "0"
    && (entry.mode & 0o111) !== 0);
  if (unexpectedExecutable) {
    contractFail("UNIVERSAL_EXTRA_EXECUTABLE", `universal npm archive contains extra executable ${unexpectedExecutable.name}`);
  }

  const launcher = entries.find((entry) => entry.name === "package/bin/minimax-codex.cjs");
  if (!launcher
      || launcher.type !== "0"
      || launcher.mode !== manifest.launcher.mode
      || launcher.bytes.length !== manifest.launcher.bytes
      || sha256(launcher.bytes) !== manifest.launcher.sha256) {
    contractFail("UNIVERSAL_LAUNCHER_MISMATCH", "universal npm launcher does not match its manifest");
  }
  const packageMetadata = entries.find((entry) => entry.name === "package/package.json");
  validatePublishedPackageEntry(packageMetadata, manifest.version);

  if (entries.length !== manifest.npmPackage.entries.length) {
    contractFail("UNIVERSAL_CHECKSUM_MISMATCH", "universal npm archive entry count does not match its manifest");
  }
  for (let index = 0; index < manifest.npmPackage.entries.length; index += 1) {
    const expected = manifest.npmPackage.entries[index];
    const actual = entries[index];
    const expectedType = expected.type === "directory" ? "5" : "0";
    if (!actual
        || actual.name !== expected.path
        || actual.type !== expectedType
        || actual.mode !== expected.mode) {
      contractFail("UNIVERSAL_CHECKSUM_MISMATCH", `universal npm archive metadata drifted at ${expected.path}`);
    }
    if (expected.type === "file"
        && (actual.bytes.length !== expected.bytes || sha256(actual.bytes) !== expected.sha256)) {
      contractFail("UNIVERSAL_CHECKSUM_MISMATCH", `universal npm archive content drifted at ${expected.path}`);
    }
  }

  const archiveHash = sha256(archiveBytes);
  const sidecarHash = parseUniversalSidecar(checksumBytes, manifest.npmPackage.name);
  if (archiveBytes.length !== manifest.npmPackage.bytes
      || archiveHash !== manifest.npmPackage.sha256
      || sidecarHash !== archiveHash) {
    contractFail("UNIVERSAL_CHECKSUM_MISMATCH", "universal npm archive checksum does not match its manifest and sidecar");
  }
  return manifest;
}

export function createDeterministicTarGzip(entries) {
  if (!Array.isArray(entries) || entries.length === 0) {
    contractFail("ARTIFACT_ARCHIVE_CORRUPT", "archive entries must be a non-empty array");
  }
  const blocks = [];
  for (const entry of [...entries].sort((left, right) => left.name.localeCompare(right.name, "en"))) {
    const bytes = Buffer.from(entry.bytes ?? Buffer.alloc(0));
    blocks.push(tarHeader({...entry, bytes}), bytes);
    const padding = (512 - (bytes.length % 512)) % 512;
    if (padding > 0) blocks.push(Buffer.alloc(padding));
  }
  blocks.push(Buffer.alloc(1024));
  const archive = gzipSync(Buffer.concat(blocks), {level: 9, mtime: 0});
  archive.fill(0, 4, 8);
  archive[9] = 255;
  return archive;
}

export function validateArtifactCandidate({manifest, contract, expectedTargetId, expectedProduct, artifacts}) {
  if (!isPlainObject(manifest) || !isPlainObject(expectedProduct) || !(artifacts instanceof Map)) {
    contractFail("ARTIFACT_CONTRACT_INVALID", "artifact candidate inputs are malformed");
  }
  if (manifest.target?.id !== expectedTargetId) {
    contractFail("ARTIFACT_TARGET_MISMATCH", `candidate target ${manifest.target?.id ?? "unknown"} does not match ${expectedTargetId}`);
  }
  if (manifest.product?.fingerprint !== expectedProduct.fingerprint || manifest.product?.fileCount !== expectedProduct.fileCount) {
    contractFail("ARTIFACT_PRODUCT_FINGERPRINT_MISMATCH", "candidate product fingerprint does not match the current product");
  }
  validateReleaseManifest(manifest, contract);

  const expectedNames = [
    manifest.nativeArchive.name,
    `${manifest.nativeArchive.name}.sha256`,
    manifest.npmPackage.name,
    `${manifest.npmPackage.name}.sha256`
  ];
  if (!sameJson([...artifacts.keys()].sort(), [...expectedNames].sort())) {
    contractFail("ARTIFACT_FILE_SET", "candidate must contain exactly two archives and their checksum sidecars");
  }
  for (const name of expectedNames) {
    const artifact = artifacts.get(name);
    if (!isPlainObject(artifact) || artifact.kind !== "file" || !Buffer.isBuffer(artifact.bytes)) {
      contractFail("ARTIFACT_UNSAFE_TYPE", `candidate artifact is not a safe regular file: ${name}`);
    }
  }

  validateArtifactArchive(manifest.nativeArchive, manifest, artifacts, "native");
  validateArtifactArchive(manifest.npmPackage, manifest, artifacts, "npm");
  return manifest;
}

export function loadExplicitFingerprint(path, currentProduct) {
  if (typeof path !== "string" || path.length === 0) {
    contractFail("E_FINGERPRINT_REQUIRED", "--fingerprint-file is required");
  }
  if (!existsSync(path)) {
    contractFail("E_FINGERPRINT_REQUIRED", `fingerprint file is missing: ${path}`);
  }
  const status = lstatSync(path);
  if (!status.isFile() || status.isSymbolicLink()) {
    contractFail("E_FINGERPRINT_INVALID", `fingerprint file is not a safe regular file: ${path}`);
  }
  let fingerprint;
  try {
    fingerprint = JSON.parse(readFileSync(path, "utf8"));
  } catch {
    contractFail("E_FINGERPRINT_INVALID", "fingerprint file is not valid JSON");
  }
  exactObject(fingerprint, ["schemaVersion", "fingerprint", "fileCount"], "E_FINGERPRINT_INVALID", "fingerprint file");
  if (fingerprint.schemaVersion !== 1
      || typeof fingerprint.fingerprint !== "string"
      || !/^[0-9a-f]{64}$/u.test(fingerprint.fingerprint)
      || !Number.isSafeInteger(fingerprint.fileCount)
      || fingerprint.fileCount <= 0) {
    contractFail("E_FINGERPRINT_INVALID", "fingerprint file fields are invalid");
  }
  if (!isPlainObject(currentProduct)
      || fingerprint.fingerprint !== currentProduct.fingerprint
      || fingerprint.fileCount !== currentProduct.fileCount) {
    contractFail("E_FINGERPRINT_STALE", "fingerprint file does not match the current product");
  }
  return fingerprint;
}

export function validateFingerprintArtifactBinding(fingerprint, artifactProduct) {
  if (!isPlainObject(fingerprint)
      || !isPlainObject(artifactProduct)
      || fingerprint.fingerprint !== artifactProduct.fingerprint
      || fingerprint.fileCount !== artifactProduct.fileCount) {
    contractFail("E_FINGERPRINT_ARTIFACT_MISMATCH", "artifact product does not match the explicit fingerprint file");
  }
  return artifactProduct;
}

function validateArtifactArchive(archive, manifest, artifacts, channel) {
  const archiveBytes = artifacts.get(archive.name).bytes;
  const sidecarBytes = artifacts.get(`${archive.name}.sha256`).bytes;
  const sidecarHash = parseSidecar(sidecarBytes, archive.name);
  const actualEntries = parseTarGzip(archiveBytes, archive.name);
  const rootPath = channel === "native"
    ? `minimax-codex-v${manifest.version}-${manifest.target.id}`
    : "package";
  const binaryPath = `${rootPath}/${manifest.binary.path}`;
  const launcherPath = `${rootPath}/${manifest.launcher.path}`;
  const binary = actualEntries.find((entry) => entry.name === binaryPath);
  if (!binary) {
    const renamed = actualEntries.find((entry) => entry.type === "0"
      && sha256(entry.bytes) === manifest.binary.sha256
      && entry.mode === manifest.binary.mode);
    if (renamed) {
      contractFail("ARTIFACT_BINARY_RENAMED", `${channel} binary was renamed to ${renamed.name}`);
    }
    contractFail("ARTIFACT_BINARY_MISSING", `${channel} archive is missing ${binaryPath}`);
  }
  if (binary.type !== "0") {
    contractFail("ARTIFACT_UNSAFE_TYPE", `${channel} binary must be a regular archive entry`);
  }
  if (binary.mode !== manifest.binary.mode || (binary.mode & 0o111) === 0) {
    contractFail("ARTIFACT_BINARY_NOT_EXECUTABLE", `${channel} binary must retain executable mode 0755`);
  }

  const launcher = actualEntries.find((entry) => entry.name === launcherPath);
  if (!launcher || launcher.type !== "0") {
    contractFail("ARTIFACT_LAUNCHER_MISMATCH", `${channel} launcher is missing or not a regular file`);
  }
  if (launcher.mode !== manifest.launcher.mode
      || launcher.bytes.length !== manifest.launcher.bytes
      || sha256(launcher.bytes) !== manifest.launcher.sha256) {
    contractFail("ARTIFACT_LAUNCHER_MISMATCH", `${channel} launcher does not match the locked launcher`);
  }

  const expectedPaths = new Set(archive.entries.map((entry) => entry.path));
  const unexpectedExecutable = actualEntries.find((entry) => !expectedPaths.has(entry.name)
    && entry.type === "0"
    && (entry.mode & 0o111) !== 0);
  if (unexpectedExecutable) {
    contractFail("ARTIFACT_EXTRA_EXECUTABLE", `${channel} archive contains extra executable ${unexpectedExecutable.name}`);
  }

  if (actualEntries.length !== archive.entries.length) {
    contractFail("ARTIFACT_CHECKSUM_MISMATCH", `${channel} archive entry count does not match its manifest`);
  }
  for (let index = 0; index < archive.entries.length; index += 1) {
    const expected = archive.entries[index];
    const actual = actualEntries[index];
    const expectedType = expected.type === "directory" ? "5" : "0";
    if (!actual || actual.name !== expected.path || actual.type !== expectedType || actual.mode !== expected.mode) {
      contractFail("ARTIFACT_CHECKSUM_MISMATCH", `${channel} archive metadata drifted at ${expected.path}`);
    }
    if (expected.type === "file"
        && (actual.bytes.length !== expected.bytes || sha256(actual.bytes) !== expected.sha256)) {
      contractFail("ARTIFACT_CHECKSUM_MISMATCH", `${channel} archive content drifted at ${expected.path}`);
    }
  }

  const archiveHash = sha256(archiveBytes);
  if (archiveBytes.length !== archive.bytes || archiveHash !== archive.sha256 || sidecarHash !== archiveHash) {
    contractFail("ARTIFACT_CHECKSUM_MISMATCH", `${channel} archive checksum does not match its manifest and sidecar`);
  }
}

function parseSidecar(bytes, expectedName) {
  const text = bytes.toString("utf8");
  const match = /^([0-9a-f]{64})  ([0-9A-Za-z][0-9A-Za-z._+-]{0,95})\n$/u.exec(text);
  if (!match || match[2] !== expectedName) {
    contractFail("ARTIFACT_SIDECAR_INVALID", `checksum sidecar is invalid for ${expectedName}`);
  }
  return match[1];
}

export function parseTarGzip(bytes, label) {
  let tar;
  try {
    tar = gunzipSync(bytes);
  } catch {
    contractFail("ARTIFACT_ARCHIVE_CORRUPT", `${label} is not a valid gzip archive`);
  }
  if (tar.length < 1024 || tar.length % 512 !== 0) {
    contractFail("ARTIFACT_ARCHIVE_CORRUPT", `${label} has an invalid tar length`);
  }
  const entries = [];
  const names = new Set();
  let offset = 0;
  let zeroBlocks = 0;
  while (offset < tar.length) {
    const header = tar.subarray(offset, offset + 512);
    if (header.every((byte) => byte === 0)) {
      zeroBlocks += 1;
      offset += 512;
      if (zeroBlocks === 2) break;
      continue;
    }
    if (zeroBlocks !== 0) contractFail("ARTIFACT_ARCHIVE_CORRUPT", `${label} has data after a zero tar block`);
    const storedChecksum = parseTarOctal(header, 148, 8, label);
    const checksumHeader = Buffer.from(header);
    checksumHeader.fill(0x20, 148, 156);
    const actualChecksum = checksumHeader.reduce((sum, value) => sum + value, 0);
    if (storedChecksum !== actualChecksum) {
      contractFail("ARTIFACT_ARCHIVE_CORRUPT", `${label} contains a tar header checksum mismatch`);
    }
    const name = readTarString(header, 0, 100);
    const mode = parseTarOctal(header, 100, 8, label);
    const size = parseTarOctal(header, 124, 12, label);
    const type = readTarString(header, 156, 1) || "0";
    if (!safeArchivePath(name) || names.has(name)) {
      contractFail("ARTIFACT_UNSAFE_TYPE", `${label} contains an unsafe or duplicate archive path`);
    }
    if (type !== "0" && type !== "5") {
      contractFail("ARTIFACT_UNSAFE_TYPE", `${label} contains unsupported archive entry type ${type}`);
    }
    if ((type === "5" && size !== 0) || offset + 512 + size > tar.length) {
      contractFail("ARTIFACT_ARCHIVE_CORRUPT", `${label} contains an invalid archive entry size`);
    }
    names.add(name);
    const contentStart = offset + 512;
    entries.push({name, mode, type, bytes: Buffer.from(tar.subarray(contentStart, contentStart + size))});
    offset = contentStart + Math.ceil(size / 512) * 512;
  }
  if (zeroBlocks !== 2 || tar.subarray(offset).some((byte) => byte !== 0)) {
    contractFail("ARTIFACT_ARCHIVE_CORRUPT", `${label} lacks a canonical two-block tar terminator`);
  }
  return entries;
}

function tarHeader(entry) {
  if (!safeArchivePath(entry.name) || Buffer.byteLength(entry.name, "utf8") > 100) {
    contractFail("ARTIFACT_UNSAFE_TYPE", `archive path is unsafe or too long: ${entry.name}`);
  }
  if (!Number.isInteger(entry.mode) || entry.mode < 0 || entry.mode > 0o7777 || !["0", "2", "5"].includes(entry.type)) {
    contractFail("ARTIFACT_UNSAFE_TYPE", `archive entry metadata is unsafe: ${entry.name}`);
  }
  const header = Buffer.alloc(512);
  writeTarString(header, 0, 100, entry.name);
  writeTarOctal(header, 100, 8, entry.mode);
  writeTarOctal(header, 108, 8, 0);
  writeTarOctal(header, 116, 8, 0);
  writeTarOctal(header, 124, 12, entry.bytes.length);
  writeTarOctal(header, 136, 12, 0);
  header.fill(0x20, 148, 156);
  writeTarString(header, 156, 1, entry.type);
  writeTarString(header, 257, 6, "ustar\0");
  writeTarString(header, 263, 2, "00");
  writeTarString(header, 265, 32, "root");
  writeTarString(header, 297, 32, "root");
  writeTarOctal(header, 329, 8, 0);
  writeTarOctal(header, 337, 8, 0);
  const checksum = header.reduce((sum, value) => sum + value, 0);
  writeTarString(header, 148, 6, checksum.toString(8).padStart(6, "0"));
  header[154] = 0;
  header[155] = 0x20;
  return header;
}

function writeTarString(buffer, offset, length, value) {
  const bytes = Buffer.from(value, "utf8");
  if (bytes.length > length) contractFail("ARTIFACT_ARCHIVE_CORRUPT", "archive metadata field overflow");
  bytes.copy(buffer, offset);
}

function writeTarOctal(buffer, offset, length, value) {
  const encoded = value.toString(8).padStart(length - 1, "0");
  if (encoded.length >= length) contractFail("ARTIFACT_ARCHIVE_CORRUPT", "archive numeric field overflow");
  writeTarString(buffer, offset, length - 1, encoded);
  buffer[offset + length - 1] = 0;
}

function readTarString(buffer, offset, length) {
  const field = buffer.subarray(offset, offset + length);
  const zero = field.indexOf(0);
  return field.subarray(0, zero === -1 ? field.length : zero).toString("utf8");
}

function parseTarOctal(buffer, offset, length, label) {
  const value = readTarString(buffer, offset, length).trim();
  if (!/^[0-7]+$/u.test(value)) contractFail("ARTIFACT_ARCHIVE_CORRUPT", `${label} contains invalid tar numeric metadata`);
  return Number.parseInt(value, 8);
}

export function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function validateUniversalPayload(payload, fields, label) {
  exactObject(payload, fields, "UNIVERSAL_MANIFEST_UNKNOWN_FIELD", `universal npm ${label}`);
  if (!safeRelativePath(payload.path)
      || !Number.isSafeInteger(payload.mode)
      || !Number.isSafeInteger(payload.bytes)
      || payload.bytes <= 0
      || typeof payload.sha256 !== "string"
      || !/^[0-9a-f]{64}$/u.test(payload.sha256)) {
    contractFail("UNIVERSAL_MANIFEST_INVALID", `universal npm ${label} payload is invalid`);
  }
}

function validatePublishedPackageEntry(entry, version) {
  if (!entry || entry.type !== "0" || entry.mode !== fileMode) {
    contractFail("PACKAGE_METADATA_FORBIDDEN", "published package metadata must be one regular non-executable file");
  }
  let packageMetadata;
  try {
    packageMetadata = JSON.parse(entry.bytes.toString("utf8"));
  } catch {
    contractFail("PACKAGE_METADATA_FORBIDDEN", "published package metadata is not valid JSON");
  }
  exactObject(
    packageMetadata,
    ["name", "version", "description", "license", "type", "bin", "engines", "repository", "homepage", "bugs", "publishConfig"],
    "PACKAGE_METADATA_FORBIDDEN",
    "published package metadata"
  );
  if (packageMetadata.name !== "minimax-codex"
      || packageMetadata.version !== version
      || typeof packageMetadata.description !== "string"
      || packageMetadata.description.length === 0
      || packageMetadata.license !== "MIT OR Apache-2.0"
      || packageMetadata.type !== "module"
      || !sameJson(packageMetadata.bin, {"minimax-codex": "bin/minimax-codex.cjs"})
      || !sameJson(packageMetadata.engines, {node: ">=20"})
      || !sameJson(packageMetadata.repository, {
        type: "git",
        url: "git+https://github.com/niuniu122/wiki-coding.git"
      })
      || packageMetadata.homepage !== "https://github.com/niuniu122/wiki-coding#readme"
      || !sameJson(packageMetadata.bugs, {url: "https://github.com/niuniu122/wiki-coding/issues"})
      || !sameJson(packageMetadata.publishConfig, {access: "public"})) {
    contractFail("PACKAGE_METADATA_IDENTITY", "published package metadata identity is invalid");
  }
}

function parseUniversalSidecar(bytes, expectedName) {
  const match = /^([0-9a-f]{64})  ([0-9A-Za-z][0-9A-Za-z._+-]{0,95})\n$/u.exec(bytes.toString("utf8"));
  if (!match || match[2] !== expectedName) {
    contractFail("UNIVERSAL_CHECKSUM_MISMATCH", "universal npm checksum sidecar is invalid");
  }
  return match[1];
}

function validateTargetFields(target) {
  exactObject(target, ["id", "rustcHost", "os", "arch", "binaryName", "binaryMode", "archiveSuffix", "supportTier"], "TARGET_SCHEMA_UNKNOWN_FIELD", "target");
  if (!safeToken(target.id) || !safeFileName(target.binaryName) || target.archiveSuffix !== ".tar.gz") {
    contractFail("TARGET_UNSAFE_NAME", "target id, binary name, or archive suffix is unsafe");
  }
  if (target.binaryMode !== directoryMode) {
    contractFail("TARGET_IDENTITY_UNSUPPORTED", "target binary mode must be executable 0755");
  }
}

function validateManifestTarget(target, contract) {
  exactObject(target, ["id", "rustcHost", "os", "arch", "supportTier"], "MANIFEST_SCHEMA_UNKNOWN_FIELD", "manifest target");
  if (!safeToken(target.id)) contractFail("MANIFEST_UNSAFE_NAME", "manifest target id is unsafe");
  const canonical = contract.targets.find((candidate) => candidate.id === target.id);
  if (!canonical) contractFail("MANIFEST_IDENTITY_UNSUPPORTED", `unsupported manifest target: ${target.id}`);
  if (target.supportTier !== canonical.supportTier) {
    contractFail("MANIFEST_TIER_MISMATCH", `${target.id} cannot use ${target.supportTier}`);
  }
  for (const key of ["rustcHost", "os", "arch"]) {
    if (target[key] !== canonical[key]) {
      contractFail("MANIFEST_IDENTITY_UNSUPPORTED", `manifest target ${key} does not match ${target.id}`);
    }
  }
}

function validateProduct(product) {
  exactObject(product, ["fingerprint", "fileCount"], "MANIFEST_SCHEMA_UNKNOWN_FIELD", "manifest product");
  validHash(product.fingerprint, "product fingerprint");
  if (!Number.isSafeInteger(product.fileCount) || product.fileCount <= 0) {
    contractFail("MANIFEST_SCHEMA_INVALID", "product file count must be a positive integer");
  }
}

function validatePayload(payload, fields, label) {
  exactObject(payload, fields, "MANIFEST_SCHEMA_UNKNOWN_FIELD", `manifest ${label}`);
  if (!safeRelativePath(payload.path)) contractFail("MANIFEST_UNSAFE_NAME", `${label} path is unsafe`);
  if (![fileMode, launcherMode].includes(payload.mode)) {
    contractFail("MANIFEST_SCHEMA_INVALID", `${label} mode is invalid`);
  }
  positiveBytes(payload.bytes, `${label} bytes`);
  validHash(payload.sha256, `${label} hash`);
}

function validateArchive(archive, expectedName, expectedEntries, label) {
  exactObject(archive, ["name", "bytes", "sha256", "entries"], "MANIFEST_SCHEMA_UNKNOWN_FIELD", `${label} archive`);
  if (!safeFileName(archive.name)) contractFail("MANIFEST_UNSAFE_NAME", `${label} archive name is unsafe`);
  if (archive.name !== expectedName) contractFail("MANIFEST_IDENTITY_UNSUPPORTED", `${label} archive name does not match its target`);
  positiveBytes(archive.bytes, `${label} archive bytes`);
  validHash(archive.sha256, `${label} archive hash`);
  if (!Array.isArray(archive.entries)) contractFail("MANIFEST_SCHEMA_INVALID", `${label} entries must be an array`);
  const paths = archive.entries.map((entry) => entry?.path);
  if (new Set(paths).size !== paths.length) contractFail("MANIFEST_DUPLICATE_ENTRY", `${label} entries contain a duplicate path`);
  for (const entry of archive.entries) validateEntry(entry, label);
  const expectedPaths = expectedEntries.map((entry) => entry.path);
  if (!sameJson(paths, expectedPaths)) contractFail("MANIFEST_ENTRY_SET", `${label} entries are not the canonical ordered set`);
  for (let index = 0; index < expectedEntries.length; index += 1) {
    const actual = archive.entries[index];
    const expected = expectedEntries[index];
    if (actual.type !== expected.type || actual.mode !== expected.mode) {
      contractFail("MANIFEST_ENTRY_SET", `${label} entry type or mode drifted: ${actual.path}`);
    }
  }
}

function validateEntry(entry, label) {
  if (!isPlainObject(entry)) contractFail("MANIFEST_SCHEMA_INVALID", `${label} entry must be an object`);
  if (!safeArchivePath(entry.path)) contractFail("MANIFEST_UNSAFE_NAME", `${label} entry path is unsafe`);
  if (entry.type === "directory") {
    exactObject(entry, ["path", "type", "mode"], "MANIFEST_SCHEMA_UNKNOWN_FIELD", `${label} directory entry`);
    if (!entry.path.endsWith("/") || entry.mode !== directoryMode) {
      contractFail("MANIFEST_ENTRY_SET", `${label} directory entry is not canonical: ${entry.path}`);
    }
  } else if (entry.type === "file") {
    exactObject(entry, ["path", "type", "mode", "bytes", "sha256"], "MANIFEST_SCHEMA_UNKNOWN_FIELD", `${label} file entry`);
    if (entry.path.endsWith("/") || ![fileMode, launcherMode].includes(entry.mode)) {
      contractFail("MANIFEST_ENTRY_SET", `${label} file entry is not canonical: ${entry.path}`);
    }
    positiveBytes(entry.bytes, `${label} entry bytes`);
    validHash(entry.sha256, `${label} entry hash`);
  } else {
    contractFail("MANIFEST_ENTRY_SET", `${label} entry type is unsupported`);
  }
}

function bindPayload(entries, path, payload, label) {
  const entry = entries.find((candidate) => candidate.path === path);
  if (!entry || entry.type !== "file" || entry.mode !== payload.mode || entry.bytes !== payload.bytes || entry.sha256 !== payload.sha256) {
    contractFail("MANIFEST_ENTRY_SET", `${label} evidence does not bind archive entry ${path}`);
  }
}

function bindSharedContent(nativeEntries, nativeRoot, npmEntries) {
  for (const relative of [
    "README.md",
    "LICENSE-APACHE",
    "LICENSE-MIT",
    ...releaseDocs.map((name) => `docs/release/${name}`)
  ]) {
    const native = nativeEntries.find((entry) => entry.path === `${nativeRoot}/${relative}`);
    const npm = npmEntries.find((entry) => entry.path === `package/${relative}`);
    if (!native || !npm || native.bytes !== npm.bytes || native.sha256 !== npm.sha256) {
      contractFail("MANIFEST_ENTRY_SET", `native and npm content drifted: ${relative}`);
    }
  }
}

function directory(path) {
  return Object.freeze({path, type: "directory", mode: directoryMode});
}

function file(path, mode) {
  return Object.freeze({path, type: "file", mode});
}

function exactObject(value, fields, code, label) {
  if (!isPlainObject(value)) contractFail(code.replace("UNKNOWN_FIELD", "INVALID"), `${label} must be an object`);
  const actual = Object.keys(value).sort();
  const expected = [...fields].sort();
  if (!sameJson(actual, expected)) contractFail(code, `${label} fields must be exact`);
}

function isPlainObject(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value) && Object.getPrototypeOf(value) === Object.prototype;
}

function safeToken(value) {
  return typeof value === "string" && /^[0-9A-Za-z][0-9A-Za-z._+-]{0,95}$/u.test(value) && !value.includes("..");
}

function safeFileName(value) {
  return safeToken(value) && !value.includes("/") && !value.includes("\\");
}

function safeRelativePath(value) {
  return typeof value === "string" && value.length > 0 && !value.startsWith("/") && !value.includes("\\")
    && value.split("/").every((part) => part.length > 0 && part !== "." && part !== "..");
}

function safeArchivePath(value) {
  if (typeof value !== "string" || value.length === 0 || value.startsWith("/") || value.includes("\\")) return false;
  const path = value.endsWith("/") ? value.slice(0, -1) : value;
  return path.length > 0 && path.split("/").every((part) => part.length > 0 && part !== "." && part !== "..");
}

function validHash(value, label) {
  if (typeof value !== "string" || !/^[0-9a-f]{64}$/u.test(value)) {
    contractFail("MANIFEST_HASH_INVALID", `${label} must be lowercase SHA-256`);
  }
}

function positiveBytes(value, label) {
  if (!Number.isSafeInteger(value) || value <= 0) contractFail("MANIFEST_SCHEMA_INVALID", `${label} must be a positive integer`);
}

function sameJson(left, right) {
  return JSON.stringify(left) === JSON.stringify(right);
}

function contractFail(code, message) {
  throw new PackageContractError(code, message);
}
