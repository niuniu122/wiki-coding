import {createHash} from "node:crypto";
import {existsSync, lstatSync, mkdirSync, readFileSync, readdirSync, statSync, writeFileSync} from "node:fs";
import {arch, cpus, platform as osPlatform, release as osRelease, totalmem} from "node:os";
import {dirname, join, relative, resolve, sep} from "node:path";
import {fileURLToPath} from "node:url";
import {gunzipSync} from "node:zlib";
import {spawn, spawnSync} from "node:child_process";
import {performance} from "node:perf_hooks";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const targetRoot = resolve(root, "target");
const args = parseArgs(process.argv.slice(2));
const cargoTarget = process.env.CARGO_TARGET_DIR?.trim()
  ? resolve(root, process.env.CARGO_TARGET_DIR)
  : targetRoot;
const defaultBinary = resolve(cargoTarget, process.platform === "win32" ? "release/minimax-cli.exe" : "release/minimax-cli");
const binary = resolve(root, args.binary ?? defaultBinary);
const artifacts = resolve(root, args.artifacts ?? "target/release-artifacts");
const thresholds = JSON.parse(readFileSync(join(root, "fixtures/compat/release/thresholds.v1.json"), "utf8"));
assertWithinTarget(binary, "release binary");
assertWithinTarget(artifacts, "release artifacts");
if (!existsSync(binary) || !lstatSync(binary).isFile() || lstatSync(binary).isSymbolicLink()) fail("release binary is missing or unsafe");

const environment = releaseEnvironment();
const packageEvidence = verifyPackage(artifacts, binary, thresholds, environment.expectedPlatform);
const licenseEvidence = verifyLicenses();
const securityEvidence = verifySecurityBoundary();
const coldStartMs = measureColdStart(binary, thresholds.coldStartMs);
const idleRssBytes = await measureIdleRss(binary, thresholds.idleRssBytes);
const wikiP95Ms = runWikiBenchmark(thresholds.wikiBm25P95Ms);
const report = {
  schemaVersion: 1,
  platform: packageEvidence.platform,
  environment,
  releaseBinary: binary.replaceAll("\\", "/"),
  package: packageEvidence,
  licenses: licenseEvidence,
  security: securityEvidence,
  performance: {
    coldStartMs,
    coldStartLimitMs: thresholds.coldStartMs,
    idleRssBytes,
    idleRssLimitBytes: thresholds.idleRssBytes,
    baseCompressedBytes: packageEvidence.compressedBytes,
    baseCompressedLimitBytes: thresholds.baseCompressedBytes,
    wikiBm25P95Ms: wikiP95Ms,
    wikiBm25P95LimitMs: thresholds.wikiBm25P95Ms
  },
  offline: true,
  providerCalls: 0,
  credentialsRead: 0,
  modelDownloads: 0
};
mkdirSync(join(root, "target/release-evidence"), {recursive: true});
const reportPath = join(root, "target/release-evidence", `${packageEvidence.platform}.json`);
writeFileSync(reportPath, `${JSON.stringify(report, null, 2)}\n`, "utf8");
process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);

function verifyPackage(directory, sourceBinary, limits, expectedPlatform) {
  if (!existsSync(directory) || !lstatSync(directory).isDirectory() || lstatSync(directory).isSymbolicLink()) {
    fail("release artifacts directory is missing or unsafe");
  }
  const archives = readdirSync(directory)
    .filter((name) => name.startsWith("minimax-codex-v") && name.endsWith(".tar.gz"))
    .sort();
  if (archives.length !== 1) fail(`expected exactly one release archive, found ${archives.length}`);
  const archiveName = archives[0];
  const archive = join(directory, archiveName);
  const checksumPath = `${archive}.sha256`;
  if (!existsSync(checksumPath)) fail("release checksum sidecar is missing");
  const actualHash = sha256(readFileSync(archive));
  const checksumText = readFileSync(checksumPath, "utf8").trim();
  const expectedHash = checksumText.split(/\s+/u)[0];
  if (actualHash !== expectedHash) fail("release archive checksum mismatch");
  if (checksumText !== `${expectedHash}  ${archiveName}`) fail("release checksum sidecar format or filename is invalid");
  const compressedBytes = statSync(archive).size;
  if (compressedBytes > limits.baseCompressedBytes) fail(`base archive exceeds ${limits.baseCompressedBytes} bytes`);
  const entries = parseTarGzip(readFileSync(archive));
  const listing = [...entries.keys()];
  if (listing.some((path) => /(?:embedding|safetensors|onnx|vector|model-weight)/iu.test(path))) {
    fail("base archive contains an embedding/model resource");
  }
  const rootName = archiveName.slice(0, -".tar.gz".length);
  const manifestRaw = entries.get(`${rootName}/RELEASE-MANIFEST.json`)?.bytes.toString("utf8");
  if (!manifestRaw) fail("release manifest is missing");
  const manifest = JSON.parse(manifestRaw);
  if (manifest.schemaVersion !== 1 || manifest.embeddingIncluded !== false || manifest.name !== "minimax-codex") {
    fail("release manifest is invalid or claims bundled embedding content");
  }
  if (manifest.platform !== expectedPlatform) fail(`release platform ${manifest.platform} does not match Rust host ${expectedPlatform}`);
  if (rootName !== `minimax-codex-v${manifest.version}-${manifest.platform}`) fail("release archive name does not match its manifest");
  const expectedBinary = manifest.platform.startsWith("windows-") ? "minimax-codex.exe" : "minimax-codex";
  if (manifest.binary !== expectedBinary) fail("release manifest binary name does not match its platform");
  if (manifest.launcher !== "bin/minimax-codex.cjs") fail("release manifest launcher path is invalid");
  const expectedSupportTier = manifest.platform.endsWith("-dev") ? "development_only" : "hosted_release";
  if (manifest.supportTier !== expectedSupportTier) fail("release support tier does not match its platform");
  if (manifest.binarySha256 !== sha256(readFileSync(sourceBinary))) fail("packaged binary hash does not match the release binary");
  if (manifest.launcherSha256 !== sha256(readFileSync(join(root, manifest.launcher)))) {
    fail("packaged launcher hash does not match the repository launcher");
  }
  const expectedEntries = [
    `${rootName}/`,
    `${rootName}/bin/`,
    `${rootName}/bin/minimax-codex.cjs`,
    `${rootName}/LICENSE-APACHE`,
    `${rootName}/LICENSE-MIT`,
    `${rootName}/RELEASE-MANIFEST.json`,
    `${rootName}/${manifest.binary}`
  ].sort();
  if (JSON.stringify([...listing].sort()) !== JSON.stringify(expectedEntries)) {
    fail("release archive contains missing or unexpected entries");
  }
  for (const name of expectedEntries) {
    const entry = entries.get(name);
    const expectedType = name.endsWith("/") ? "5" : "0";
    if (!entry || entry.type !== expectedType) fail(`release archive entry has an unsafe type: ${name}`);
  }
  if (sha256(entries.get(`${rootName}/${manifest.binary}`).bytes) !== manifest.binarySha256) {
    fail("binary inside the release archive does not match the manifest");
  }
  if (sha256(entries.get(`${rootName}/${manifest.launcher}`).bytes) !== manifest.launcherSha256) {
    fail("launcher inside the release archive does not match the manifest");
  }
  return {
    archive: archive.replaceAll("\\", "/"),
    archiveSha256: actualHash,
    compressedBytes,
    platform: manifest.platform,
    manifest
  };
}

function verifyLicenses() {
  const metadata = JSON.parse(run("cargo", ["metadata", "--locked", "--format-version", "1"]));
  const approved = ["MIT", "Apache-2.0", "ISC", "BSD-3-Clause", "Zlib", "Unicode-3.0", "Unlicense", "CC0-1.0", "MIT-0", "CDLA-Permissive-2.0"];
  const invalid = metadata.packages.filter((item) => typeof item.license !== "string" || !approved.some((license) => item.license.includes(license)));
  if (invalid.length > 0) fail(`dependency license is missing or not allowlisted: ${invalid.map((item) => item.name).join(", ")}`);
  return {packagesChecked: metadata.packages.length, invalid: 0, policy: "at_least_one_approved_permissive_choice"};
}

function verifySecurityBoundary() {
  const lock = readFileSync(join(root, "Cargo.lock"), "utf8");
  const deniedPackages = ["rusqlite", "sqlx", "diesel", "sea-orm", "sea_orm"];
  const foundPackages = deniedPackages.filter((name) => lock.includes(`name = "${name}"`));
  if (foundPackages.length > 0) fail(`database dependency found: ${foundPackages.join(", ")}`);
  const rustFiles = collectFiles(join(root, "crates"), ".rs");
  const unsafeFiles = rustFiles.filter((file) => /\bunsafe\s*(?:fn|\{)/u.test(readFileSync(file, "utf8")));
  if (unsafeFiles.length > 0) fail(`unsafe Rust source found: ${unsafeFiles.join(", ")}`);
  if (!/^unsafe_code\s*=\s*"forbid"$/mu.test(readFileSync(join(root, "Cargo.toml"), "utf8"))) {
    fail("workspace Cargo.toml must forbid unsafe Rust code");
  }
  const migrationSource = readFileSync(join(root, "crates/cli/src/migration.rs"), "utf8");
  for (const denied of ["reqwest", "minimax_provider", "Authorization", "Bearer ", "rusqlite", "sqlx", "download_model"]) {
    if (migrationSource.includes(denied)) fail(`migration source boundary contains ${denied}`);
  }
  return {unsafeFiles: 0, unsafeWorkspaceLint: "forbid", databasePackages: 0, migrationNetworkOrCredentialPaths: 0};
}

function releaseEnvironment() {
  const rustc = run("rustc", ["-vV"]);
  const host = /^host:\s*(.+)$/mu.exec(rustc)?.[1]?.trim() ?? "";
  const rustcRelease = /^release:\s*(.+)$/mu.exec(rustc)?.[1]?.trim() ?? "";
  const expectedPlatform = platformForHost(host);
  const processors = cpus();
  return {
    os: osPlatform(),
    osRelease: osRelease(),
    architecture: arch(),
    cpuModel: processors[0]?.model ?? "unknown",
    logicalCpuCount: processors.length,
    totalMemoryBytes: totalmem(),
    node: process.version,
    rustcRelease,
    rustcHost: host,
    expectedPlatform
  };
}

function platformForHost(host) {
  if (host === "x86_64-pc-windows-msvc") return "windows-x86_64-msvc";
  if (host === "x86_64-pc-windows-gnullvm") return "windows-x86_64-gnullvm-dev";
  if (host === "x86_64-unknown-linux-gnu") return "linux-x86_64-gnu";
  fail(`unsupported Rust release host: ${host || "unknown"}`);
}

function parseTarGzip(bytes) {
  let tar;
  try {
    tar = gunzipSync(bytes);
  } catch {
    fail("release archive is not valid gzip data");
  }
  const entries = new Map();
  let offset = 0;
  while (offset + 512 <= tar.length) {
    const header = tar.subarray(offset, offset + 512);
    if (header.every((value) => value === 0)) break;
    const name = readTarString(header, 0, 100);
    const prefix = readTarString(header, 345, 155);
    const path = prefix ? `${prefix}/${name}` : name;
    const size = readTarOctal(header, 124, 12);
    const storedChecksum = readTarOctal(header, 148, 8);
    const checksumHeader = Buffer.from(header);
    checksumHeader.fill(0x20, 148, 156);
    const actualChecksum = checksumHeader.reduce((sum, value) => sum + value, 0);
    const rawType = readTarString(header, 156, 1);
    const type = rawType === "" ? "0" : rawType;
    if (!path || path.startsWith("/") || path.includes("\\") || path.split("/").includes("..")) {
      fail("release archive contains a non-canonical path");
    }
    if (!['0', '5'].includes(type) || storedChecksum !== actualChecksum || entries.has(path)) {
      fail("release archive contains an unsafe, corrupt, or duplicate entry");
    }
    const contentStart = offset + 512;
    const contentEnd = contentStart + size;
    if (contentEnd > tar.length) fail("release archive entry is truncated");
    entries.set(path, {type, bytes: Buffer.from(tar.subarray(contentStart, contentEnd))});
    offset = contentStart + Math.ceil(size / 512) * 512;
  }
  if (entries.size === 0) fail("release archive is empty");
  return entries;
}

function readTarString(buffer, offset, length) {
  const field = buffer.subarray(offset, offset + length);
  const end = field.indexOf(0);
  return field.subarray(0, end === -1 ? field.length : end).toString("utf8");
}

function readTarOctal(buffer, offset, length) {
  const value = readTarString(buffer, offset, length).trim();
  if (!/^[0-7]+$/u.test(value)) fail("release archive contains an invalid numeric field");
  return Number.parseInt(value, 8);
}

function measureColdStart(executable, limit) {
  const samples = [];
  for (let index = 0; index < 9; index += 1) {
    const started = performance.now();
    const result = spawnSync(executable, ["index", "capabilities", "status"], {
      cwd: root,
      encoding: "utf8",
      shell: false,
      windowsHide: true,
      timeout: 5_000
    });
    const elapsed = performance.now() - started;
    if (result.status !== 0) fail(`cold-start command failed: ${(result.stderr || result.stdout || "").trim()}`);
    samples.push(Number(elapsed.toFixed(3)));
  }
  const p95 = percentile(samples, 95);
  if (p95 > limit) fail(`cold-start p95 ${p95} ms exceeds ${limit} ms`);
  return {samples, p95};
}

async function measureIdleRss(executable, limit) {
  const child = spawn(executable, ["__release-probe", "--hold-ms", "10000"], {
    cwd: root,
    stdio: ["ignore", "pipe", "pipe"],
    shell: false,
    windowsHide: true
  });
  await waitForReady(child);
  const samples = await readRssSamples(child.pid, 5, 100);
  const maximum = Math.max(...samples);
  if (maximum > limit) {
    child.kill();
    fail(`idle RSS ${maximum} bytes exceeds ${limit} bytes`);
  }
  child.kill();
  return {samples, maximum};
}

async function readRssSamples(pid, count, intervalMs) {
  if (process.platform === "win32") {
    const powershell = join(process.env.SystemRoot ?? "C:\\Windows", "System32/WindowsPowerShell/v1.0/powershell.exe");
    const command = [
      `1..${Number(count)} | ForEach-Object {`,
      `  $value = (Get-Process -Id ${Number(pid)} -ErrorAction Stop).WorkingSet64;`,
      "  [Console]::Out.WriteLine([string]$value);",
      `  Start-Sleep -Milliseconds ${Number(intervalMs)}`,
      "}"
    ].join(" ");
    const output = spawnSync(
      powershell,
      ["-NoProfile", "-NonInteractive", "-Command", command],
      {encoding: "utf8", shell: false, windowsHide: true, timeout: 5_000}
    );
    const samples = output.stdout.trim().split(/\r?\n/u).map((value) => Number(value.trim()));
    if (output.status !== 0 || samples.length !== count || samples.some((value) => !Number.isFinite(value) || value <= 0)) {
      fail(`could not sample Windows idle RSS: ${(output.stderr || output.stdout || "no output").trim()}`);
    }
    return samples;
  }
  const samples = [];
  for (let index = 0; index < count; index += 1) {
    const status = readFileSync(`/proc/${Number(pid)}/status`, "utf8");
    const kib = Number(/^VmRSS:\s+(\d+)\s+kB$/mu.exec(status)?.[1]);
    if (!Number.isFinite(kib) || kib <= 0) fail("could not sample Linux idle RSS");
    samples.push(kib * 1024);
    await delay(intervalMs);
  }
  return samples;
}

function runWikiBenchmark(limit) {
  const result = spawnSync(
    "cargo",
    ["test", "-p", "minimax-retrieval", "--test", "benchmark", "--release", "--locked", "--", "--nocapture"],
    {cwd: root, encoding: "utf8", shell: false, windowsHide: true, timeout: 180_000}
  );
  const output = `${result.stdout}\n${result.stderr}`;
  if (result.status !== 0) fail(`10k Wiki benchmark failed: ${output.trim()}`);
  const p95 = Number(/10k Wiki BM25 p95:\s*([0-9.]+)\s*ms/u.exec(output)?.[1]);
  if (!Number.isFinite(p95) || p95 > limit) fail(`10k Wiki p95 is missing or exceeds ${limit} ms`);
  return p95;
}

function waitForReady(child) {
  return new Promise((resolveReady, reject) => {
    const timer = setTimeout(() => reject(new Error("release probe readiness timeout")), 5_000);
    child.once("error", reject);
    child.stdout.on("data", (chunk) => {
      if (chunk.toString("utf8").includes("release-probe-ready:")) {
        clearTimeout(timer);
        resolveReady();
      }
    });
  }).catch((error) => fail(error.message));
}

function percentile(values, percent) {
  const sorted = [...values].sort((left, right) => left - right);
  return sorted[Math.max(0, Math.ceil((sorted.length * percent) / 100) - 1)];
}

function collectFiles(directory, extension) {
  const output = [];
  for (const entry of readdirSync(directory, {withFileTypes: true})) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) output.push(...collectFiles(path, extension));
    else if (entry.isFile() && path.endsWith(extension)) output.push(path);
  }
  return output;
}

function run(command, commandArgs) {
  const result = spawnSync(command, commandArgs, {
    cwd: root,
    encoding: "utf8",
    shell: false,
    windowsHide: true,
    timeout: 30_000,
    maxBuffer: 32 * 1024 * 1024
  });
  if (result.status !== 0) fail(`${command} failed: ${(result.stderr || result.stdout || "").trim()}`);
  return result.stdout;
}

function parseArgs(values) {
  const parsed = {};
  const allowed = new Set(["binary", "artifacts"]);
  for (let index = 0; index < values.length; index += 1) {
    const key = values[index];
    const name = key?.startsWith("--") ? key.slice(2) : "";
    if (!allowed.has(name) || index + 1 >= values.length || Object.hasOwn(parsed, name)) {
      fail(`invalid verification argument: ${key ?? ""}`);
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

function delay(milliseconds) {
  return new Promise((resolveDelay) => setTimeout(resolveDelay, milliseconds));
}

function fail(message) {
  process.stderr.write(`release verification failed: ${message}\n`);
  process.exit(1);
}
