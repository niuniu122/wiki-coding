import {createHash} from "node:crypto";
import {existsSync, lstatSync, mkdirSync, readFileSync, writeFileSync} from "node:fs";
import {fileURLToPath} from "node:url";
import {gzipSync} from "node:zlib";
import {basename, dirname, relative, resolve, sep} from "node:path";
import {spawnSync} from "node:child_process";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const targetRoot = resolve(root, "target");
const args = parseArgs(process.argv.slice(2));
const packageJson = JSON.parse(readFileSync(resolve(root, "package.json"), "utf8"));
const version = args.version ?? packageJson.version;
const platform = args.platform ?? detectPlatform();
const binary = resolve(root, args.binary ?? defaultBinary());
const output = resolve(root, args.output ?? "target/release-artifacts");

if (!/^[0-9A-Za-z][0-9A-Za-z.+-]{0,63}$/u.test(version)) fail("release version is invalid");
if (!["windows-x86_64-msvc", "windows-x86_64-gnullvm-dev", "linux-x86_64-gnu"].includes(platform)) {
  fail(`unsupported release platform: ${platform}`);
}
assertWithinTarget(binary, "release binary");
assertWithinTarget(output, "release output");
if (!existsSync(binary)) fail(`release binary is missing: ${binary}`);
const binaryStatus = lstatSync(binary);
if (!binaryStatus.isFile() || binaryStatus.isSymbolicLink()) {
  fail(`release binary is not a regular file: ${binary}`);
}
const binaryBytes = readFileSync(binary);
if (binaryBytes.length === 0 || binaryBytes.length > 50 * 1024 * 1024) {
  fail("release binary must be non-empty and at most 50 MiB before packaging");
}

const packageName = `minimax-codex-v${version}-${platform}`;
const executable = platform.startsWith("windows-") ? "minimax-codex.exe" : "minimax-codex";
const archive = resolve(output, `${packageName}.tar.gz`);
const manifest = {
  schemaVersion: 1,
  name: "minimax-codex",
  version,
  platform,
  binary: executable,
  binarySha256: sha256(binaryBytes),
  embeddingIncluded: false,
  supportTier: platform.endsWith("-dev") ? "development_only" : "hosted_release",
  rustToolchain: "1.97.0"
};
const entries = [
  {name: `${packageName}/`, bytes: Buffer.alloc(0), mode: 0o755, type: "5"},
  {name: `${packageName}/${executable}`, bytes: binaryBytes, mode: 0o755, type: "0"},
  {name: `${packageName}/LICENSE-APACHE`, bytes: readFileSync(resolve(root, "LICENSE-APACHE")), mode: 0o644, type: "0"},
  {name: `${packageName}/LICENSE-MIT`, bytes: readFileSync(resolve(root, "LICENSE-MIT")), mode: 0o644, type: "0"},
  {
    name: `${packageName}/RELEASE-MANIFEST.json`,
    bytes: Buffer.from(`${JSON.stringify(manifest, null, 2)}\n`, "utf8"),
    mode: 0o644,
    type: "0"
  }
];

mkdirSync(output, {recursive: true});
writeFileSync(archive, deterministicTarGzip(entries));
const archiveHash = sha256(readFileSync(archive));
writeFileSync(`${archive}.sha256`, `${archiveHash}  ${basename(archive)}\n`, "utf8");
process.stdout.write(`${JSON.stringify({schemaVersion: 1, archive, sha256: archiveHash, platform, version})}\n`);

function deterministicTarGzip(entriesToWrite) {
  const blocks = [];
  for (const entry of [...entriesToWrite].sort((left, right) => left.name.localeCompare(right.name, "en"))) {
    const header = tarHeader(entry);
    blocks.push(header, entry.bytes);
    const padding = (512 - (entry.bytes.length % 512)) % 512;
    if (padding > 0) blocks.push(Buffer.alloc(padding));
  }
  blocks.push(Buffer.alloc(1024));
  const archiveBytes = gzipSync(Buffer.concat(blocks), {level: 9, mtime: 0});
  archiveBytes.fill(0, 4, 8);
  archiveBytes[9] = 255;
  return archiveBytes;
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
  const checksumText = checksum.toString(8).padStart(6, "0");
  writeString(header, 148, 6, checksumText);
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

function detectPlatform() {
  const output = spawnSync("rustc", ["-vV"], {cwd: root, encoding: "utf8", shell: false, windowsHide: true});
  if (output.status !== 0) fail("rustc -vV failed while detecting the release platform");
  const host = /^host:\s*(.+)$/mu.exec(output.stdout)?.[1]?.trim() ?? "";
  if (host === "x86_64-pc-windows-msvc") return "windows-x86_64-msvc";
  if (host === "x86_64-pc-windows-gnullvm") return "windows-x86_64-gnullvm-dev";
  if (host === "x86_64-unknown-linux-gnu") return "linux-x86_64-gnu";
  fail(`unsupported release host: ${host || "unknown"}`);
}

function parseArgs(values) {
  const parsed = {};
  const allowed = new Set(["binary", "output", "platform", "version"]);
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
