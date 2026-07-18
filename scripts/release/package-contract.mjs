import {readFileSync} from "node:fs";
import {dirname, resolve} from "node:path";
import {fileURLToPath} from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const defaultTargetContract = resolve(root, "fixtures/compat/release/targets.v1.json");
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
