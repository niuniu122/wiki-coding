import {createHash} from "node:crypto";
import {
  existsSync,
  lstatSync,
  mkdirSync,
  readFileSync,
  writeFileSync
} from "node:fs";
import {basename, dirname, relative, resolve, sep} from "node:path";
import {spawnSync} from "node:child_process";
import {fileURLToPath} from "node:url";
import {gzipSync} from "node:zlib";

import {
  expectedArchiveEntries,
  loadTargetContract,
  validateReleaseManifest
} from "./package-contract.mjs";
import {computeProductFingerprint} from "./product-fingerprint.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const targetRoot = resolve(root, "target");
const args = parseArgs(process.argv.slice(2));
const packageJson = JSON.parse(readFileSync(resolve(root, "package.json"), "utf8"));
const version = args.version ?? packageJson.version;
const binary = resolve(root, args.binary ?? defaultBinary());
const output = resolve(root, args.output ?? "target/release-artifacts");
const targetContract = loadTargetContract();
const rustcHost = detectRustcHost();
const target = targetContract.targets.find((candidate) => candidate.rustcHost === rustcHost);

if (!target) fail(`unsupported release host: ${rustcHost || "unknown"}`);
if (!/^[0-9A-Za-z][0-9A-Za-z.+-]{0,63}$/u.test(version)) fail("release version is invalid");
assertWithinTarget(binary, "release binary");
assertWithinTarget(output, "release output");
const binaryBytes = readRegularFile(binary, "release binary");
if (binaryBytes.length === 0 || binaryBytes.length > 50 * 1024 * 1024) {
  fail("release binary must be non-empty and at most 50 MiB before packaging");
}
if (process.platform !== "win32" && (lstatSync(binary).mode & 0o111) === 0) {
  fail("release binary must be executable");
}

const sourceBytes = new Map([
  ["bin/minimax-codex.cjs", readRegularFile(resolve(root, "bin/minimax-codex.cjs"), "release launcher")],
  [target.binaryName, binaryBytes],
  ["package.json", readRegularFile(resolve(root, "package.json"), "package metadata")],
  ["README.md", readRegularFile(resolve(root, "README.md"), "README")],
  ["LICENSE-APACHE", readRegularFile(resolve(root, "LICENSE-APACHE"), "Apache license")],
  ["LICENSE-MIT", readRegularFile(resolve(root, "LICENSE-MIT"), "MIT license")],
  ["docs/release/cutover.md", readRegularFile(resolve(root, "docs/release/cutover.md"), "cutover documentation")],
  ["docs/release/embedding-package.md", readRegularFile(resolve(root, "docs/release/embedding-package.md"), "embedding documentation")],
  ["docs/release/install-upgrade-rollback.md", readRegularFile(resolve(root, "docs/release/install-upgrade-rollback.md"), "installation documentation")],
  ["docs/release/subprocess-sandbox.md", readRegularFile(resolve(root, "docs/release/subprocess-sandbox.md"), "sandbox documentation")]
]);

const nativeEntries = materializeEntries(expectedArchiveEntries(target, version, "native"), "native");
const npmEntries = materializeEntries(expectedArchiveEntries(target, version, "npm"), "npm");
const nativeBytes = deterministicTarGzip(nativeEntries);
const npmBytes = deterministicTarGzip(npmEntries);
const baseName = `minimax-codex-v${version}-${target.id}`;
const archiveName = `${baseName}${target.archiveSuffix}`;
const npmName = `${baseName}-npm.tgz`;
const manifestName = `${baseName}-RELEASE-MANIFEST.json`;
const product = computeProductFingerprint(root);
const launcherBytes = sourceBytes.get("bin/minimax-codex.cjs");
const manifest = {
  schemaVersion: targetContract.manifestSchemaVersion,
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
  product: {fingerprint: product.fingerprint, fileCount: product.fileCount},
  binary: {path: target.binaryName, mode: target.binaryMode, bytes: binaryBytes.length, sha256: sha256(binaryBytes)},
  launcher: {path: "bin/minimax-codex.cjs", mode: 0o755, bytes: launcherBytes.length, sha256: sha256(launcherBytes)},
  nativeArchive: {
    name: archiveName,
    bytes: nativeBytes.length,
    sha256: sha256(nativeBytes),
    entries: entryEvidence(nativeEntries)
  },
  npmPackage: {
    name: npmName,
    bytes: npmBytes.length,
    sha256: sha256(npmBytes),
    entries: entryEvidence(npmEntries)
  }
};
validateReleaseManifest(manifest, targetContract);

mkdirSync(output, {recursive: true});
writeArtifact(output, archiveName, nativeBytes, manifest.nativeArchive.sha256);
writeArtifact(output, npmName, npmBytes, manifest.npmPackage.sha256);
const manifestPath = resolve(output, manifestName);
writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`, "utf8");
process.stdout.write(`${JSON.stringify({
  schemaVersion: 2,
  manifest: manifestPath,
  archive: resolve(output, archiveName),
  sha256: manifest.nativeArchive.sha256,
  npmArchive: resolve(output, npmName),
  npmSha256: manifest.npmPackage.sha256,
  platform: target.id,
  rustcHost,
  supportTier: target.supportTier,
  version
})}\n`);

function materializeEntries(descriptors, channel) {
  const prefix = channel === "native" ? `minimax-codex-v${version}-${target.id}/` : "package/";
  return descriptors.map((descriptor) => {
    if (descriptor.type === "directory") {
      return {name: descriptor.path, bytes: Buffer.alloc(0), mode: descriptor.mode, type: "5"};
    }
    const relativePath = descriptor.path.slice(prefix.length);
    const bytes = sourceBytes.get(relativePath);
    if (!bytes) fail(`canonical ${channel} entry has no source bytes: ${relativePath}`);
    return {name: descriptor.path, bytes, mode: descriptor.mode, type: "0"};
  });
}

function entryEvidence(entries) {
  return entries.map((entry) => entry.type === "5"
    ? {path: entry.name, type: "directory", mode: entry.mode}
    : {path: entry.name, type: "file", mode: entry.mode, bytes: entry.bytes.length, sha256: sha256(entry.bytes)});
}

function writeArtifact(directory, name, bytes, hash) {
  const path = resolve(directory, name);
  writeFileSync(path, bytes);
  writeFileSync(`${path}.sha256`, `${hash}  ${basename(path)}\n`, "utf8");
}

function deterministicTarGzip(entries) {
  const blocks = [];
  for (const entry of [...entries].sort((left, right) => left.name.localeCompare(right.name, "en"))) {
    blocks.push(tarHeader(entry), entry.bytes);
    const padding = (512 - (entry.bytes.length % 512)) % 512;
    if (padding > 0) blocks.push(Buffer.alloc(padding));
  }
  blocks.push(Buffer.alloc(1024));
  const archive = gzipSync(Buffer.concat(blocks), {level: 9, mtime: 0});
  archive.fill(0, 4, 8);
  archive[9] = 255;
  return archive;
}

function tarHeader(entry) {
  if (Buffer.byteLength(entry.name, "utf8") > 100) fail(`archive path is too long: ${entry.name}`);
  const header = Buffer.alloc(512);
  writeString(header, 0, 100, entry.name);
  writeOctal(header, 100, 8, entry.mode);
  writeOctal(header, 108, 8, 0);
  writeOctal(header, 116, 8, 0);
  writeOctal(header, 124, 12, entry.bytes.length);
  writeOctal(header, 136, 12, 0);
  header.fill(0x20, 148, 156);
  writeString(header, 156, 1, entry.type);
  writeString(header, 257, 6, "ustar\0");
  writeString(header, 263, 2, "00");
  writeString(header, 265, 32, "root");
  writeString(header, 297, 32, "root");
  writeOctal(header, 329, 8, 0);
  writeOctal(header, 337, 8, 0);
  const checksum = header.reduce((sum, value) => sum + value, 0);
  writeString(header, 148, 6, checksum.toString(8).padStart(6, "0"));
  header[154] = 0;
  header[155] = 0x20;
  return header;
}

function writeString(buffer, offset, length, value) {
  const bytes = Buffer.from(value, "utf8");
  if (bytes.length > length) fail(`archive metadata exceeds ${length} bytes`);
  bytes.copy(buffer, offset);
}

function writeOctal(buffer, offset, length, value) {
  const encoded = value.toString(8).padStart(length - 1, "0");
  if (encoded.length >= length) fail("archive numeric field overflow");
  writeString(buffer, offset, length - 1, encoded);
  buffer[offset + length - 1] = 0;
}

function defaultBinary() {
  const cargoTarget = process.env.CARGO_TARGET_DIR?.trim()
    ? resolve(root, process.env.CARGO_TARGET_DIR)
    : targetRoot;
  return resolve(cargoTarget, process.platform === "win32" ? "release/minimax-cli.exe" : "release/minimax-cli");
}

function detectRustcHost() {
  const output = spawnSync("rustc", ["-vV"], {cwd: root, encoding: "utf8", shell: false, windowsHide: true});
  if (output.status !== 0) fail("rustc -vV failed while detecting the release target");
  return /^host:\s*(.+)$/mu.exec(output.stdout)?.[1]?.trim() ?? "";
}

function parseArgs(values) {
  const parsed = {};
  const allowed = new Set(["binary", "output", "version"]);
  for (let index = 0; index < values.length; index += 1) {
    const key = values[index];
    const name = key?.startsWith("--") ? key.slice(2) : "";
    if (!allowed.has(name) || index + 1 >= values.length || Object.hasOwn(parsed, name)) {
      fail(`invalid package argument: ${key ?? ""}`);
    }
    parsed[name] = values[index + 1];
    index += 1;
  }
  return parsed;
}

function readRegularFile(path, label) {
  if (!existsSync(path)) fail(`${label} is missing: ${path}`);
  const status = lstatSync(path);
  if (!status.isFile() || status.isSymbolicLink()) fail(`${label} is not a safe regular file: ${path}`);
  return readFileSync(path);
}

function assertWithinTarget(path, label) {
  const local = relative(targetRoot, path);
  if (local === "" || local === ".." || local.startsWith(`..${sep}`) || resolve(targetRoot, local) !== path) {
    fail(`${label} must stay inside the repository target directory`);
  }
}

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function fail(message) {
  process.stderr.write(`release packaging failed: ${message}\n`);
  process.exit(1);
}
