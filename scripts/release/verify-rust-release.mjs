import {createHash} from "node:crypto";
import {chmodSync, copyFileSync, existsSync, lstatSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync} from "node:fs";
import {arch, cpus, platform as osPlatform, release as osRelease, totalmem} from "node:os";
import {dirname, join, relative, resolve, sep} from "node:path";
import {fileURLToPath} from "node:url";
import {gunzipSync} from "node:zlib";
import {spawn, spawnSync} from "node:child_process";
import {performance} from "node:perf_hooks";
import {loadTargetContract, validateReleaseManifest} from "./package-contract.mjs";
import {computeProductFingerprint} from "./product-fingerprint.mjs";

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
const targetContract = loadTargetContract();
if (thresholds.schemaVersion !== targetContract.thresholdSchemaVersion
    || thresholds.targetContractSchemaVersion !== targetContract.schemaVersion) {
  fail("release threshold and target contract schema versions do not match");
}
assertWithinTarget(binary, "release binary");
assertWithinTarget(artifacts, "release artifacts");
if (!existsSync(binary) || !lstatSync(binary).isFile() || lstatSync(binary).isSymbolicLink()) fail("release binary is missing or unsafe");

const environment = releaseEnvironment(targetContract);
const packageEvidence = verifyPackage(artifacts, binary, thresholds, environment.expectedTarget, targetContract);
const licenseEvidence = verifyLicenses();
const securityEvidence = verifySecurityBoundary();
const coldStartMs = measureColdStart(binary, thresholds.coldStartMs);
const idleRssBytes = await measureIdleRss(binary, thresholds.idleRssBytes);
const wikiP95Ms = runWikiBenchmark(thresholds.wikiBm25P95Ms);
const productInputs = computeProductFingerprint(root);
const report = {
  schemaVersion: 1,
  platform: packageEvidence.platform,
  productFingerprint: productInputs.fingerprint,
  productFileCount: productInputs.fileCount,
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

function verifyPackage(directory, sourceBinary, limits, expectedTarget, contract) {
  if (!existsSync(directory) || !lstatSync(directory).isDirectory() || lstatSync(directory).isSymbolicLink()) {
    fail("release artifacts directory is missing or unsafe");
  }
  const manifests = readdirSync(directory)
    .filter((name) => name.startsWith("minimax-codex-v") && name.endsWith("-RELEASE-MANIFEST.json"))
    .sort();
  if (manifests.length !== 1) fail(`expected exactly one release manifest, found ${manifests.length}`);
  let manifest;
  try {
    manifest = JSON.parse(readFileSync(join(directory, manifests[0]), "utf8"));
    validateReleaseManifest(manifest, contract);
  } catch (error) {
    fail(`release manifest is invalid: ${error instanceof Error ? error.message : String(error)}`);
  }
  const expectedManifestName = `minimax-codex-v${manifest.version}-${manifest.target.id}-RELEASE-MANIFEST.json`;
  if (manifests[0] !== expectedManifestName) fail("release manifest filename does not match its target");
  if (manifest.target.id !== expectedTarget.id
      || manifest.target.rustcHost !== expectedTarget.rustcHost
      || manifest.target.supportTier !== expectedTarget.supportTier) {
    fail(`release target ${manifest.target.id}/${manifest.target.rustcHost} does not match Rust host ${expectedTarget.id}/${expectedTarget.rustcHost}`);
  }
  const sourceBinaryBytes = readFileSync(sourceBinary);
  if (manifest.binary.sha256 !== sha256(sourceBinaryBytes) || manifest.binary.bytes !== sourceBinaryBytes.length) {
    fail("packaged binary evidence does not match the release binary");
  }
  const launcherBytes = readFileSync(join(root, manifest.launcher.path));
  if (manifest.launcher.sha256 !== sha256(launcherBytes) || manifest.launcher.bytes !== launcherBytes.length) {
    fail("packaged launcher evidence does not match the repository launcher");
  }
  const product = computeProductFingerprint(root);
  if (manifest.product.fingerprint !== product.fingerprint || manifest.product.fileCount !== product.fileCount) {
    fail("release manifest product fingerprint does not match the repository");
  }
  const expectedFiles = [
    manifest.nativeArchive.name,
    `${manifest.nativeArchive.name}.sha256`,
    manifest.npmPackage.name,
    `${manifest.npmPackage.name}.sha256`,
    expectedManifestName
  ].sort();
  if (JSON.stringify(readdirSync(directory).sort()) !== JSON.stringify(expectedFiles)) {
    fail("release artifact directory contains missing or unexpected files");
  }
  const native = verifyArchive(directory, manifest.nativeArchive, limits, "native");
  const rootName = `minimax-codex-v${manifest.version}-${manifest.target.id}`;
  assertOnlyProductExecutables(
    native.entries,
    new Set([`${rootName}/${manifest.binary.path}`, `${rootName}/${manifest.launcher.path}`])
  );
  const npmPackage = verifyNpmPackage(directory, manifest, limits, sourceBinary);
  return {
    archive: native.path.replaceAll("\\", "/"),
    archiveSha256: native.hash,
    compressedBytes: native.bytes.length,
    platform: manifest.target.id,
    manifest,
    npmPackage
  };
}

function verifyArchive(directory, evidence, limits, label) {
  const path = join(directory, evidence.name);
  const checksumPath = `${path}.sha256`;
  if (!existsSync(path) || !lstatSync(path).isFile() || lstatSync(path).isSymbolicLink()) {
    fail(`${label} archive is missing or unsafe`);
  }
  if (!existsSync(checksumPath) || !lstatSync(checksumPath).isFile() || lstatSync(checksumPath).isSymbolicLink()) {
    fail(`${label} checksum sidecar is missing or unsafe`);
  }
  const bytes = readFileSync(path);
  const hash = sha256(bytes);
  if (hash !== evidence.sha256 || bytes.length !== evidence.bytes) {
    fail(`${label} archive bytes do not match the release manifest`);
  }
  if (readFileSync(checksumPath, "utf8") !== `${hash}  ${evidence.name}\n`) {
    fail(`${label} checksum sidecar format, hash, or filename is invalid`);
  }
  if (bytes.length > limits.baseCompressedBytes) {
    fail(`${label} archive exceeds ${limits.baseCompressedBytes} bytes`);
  }
  const entries = parseTarGzip(bytes);
  const paths = [...entries.keys()];
  const expectedPaths = evidence.entries.map((entry) => entry.path);
  if (JSON.stringify(paths) !== JSON.stringify(expectedPaths)) {
    fail(`${label} archive contains missing, reordered, or unexpected entries`);
  }
  if (paths.some(containsBundledModelPath)) {
    fail(`${label} archive contains an embedding/model resource`);
  }
  for (const expected of evidence.entries) {
    const actual = entries.get(expected.path);
    const expectedType = expected.type === "directory" ? "5" : "0";
    if (!actual || actual.type !== expectedType || actual.mode !== expected.mode) {
      fail(`${label} archive entry type or mode drifted: ${expected.path}`);
    }
    if (expected.type === "file"
        && (actual.bytes.length !== expected.bytes || sha256(actual.bytes) !== expected.sha256)) {
      fail(`${label} archive entry bytes drifted: ${expected.path}`);
    }
  }
  return {path, bytes, hash, entries};
}

function containsBundledModelPath(path) {
  return /\.(?:onnx|safetensors|gguf)$/iu.test(path)
    || /(?:^|\/)(?:models?|model-weights?|embedding-resources?)(?:\/|$)/iu.test(path);
}

function verifyNpmPackage(directory, manifest, limits, sourceBinary) {
  const verified = verifyArchive(directory, manifest.npmPackage, limits, "npm");
  const {entries, bytes, hash, path: archive} = verified;
  const packageJson = JSON.parse(entries.get("package/package.json").bytes.toString("utf8"));
  if (JSON.stringify(packageJson.bin) !== JSON.stringify({"minimax-codex": "bin/minimax-codex.cjs"})) {
    fail("npm package must expose exactly the Rust launcher bin");
  }
  for (const dependencyClass of ["dependencies", "devDependencies", "optionalDependencies"]) {
    if (Object.hasOwn(packageJson, dependencyClass)) fail(`npm package contains ${dependencyClass}`);
  }
  assertOnlyProductExecutables(entries, new Set(["package/bin/minimax-codex.cjs", `package/${manifest.binary.path}`]));
  if (sha256(entries.get(`package/${manifest.binary.path}`).bytes) !== manifest.binary.sha256
      || sha256(entries.get("package/bin/minimax-codex.cjs").bytes) !== manifest.launcher.sha256) {
    fail("npm package launcher or native binary hash is invalid");
  }
  const smokeRoot = mkdtempSync(join(targetRoot, "release-npm-smoke-"));
  const directRoot = mkdtempSync(join(targetRoot, "release-direct-smoke-"));
  try {
    for (const [name, entry] of entries) {
      const destination = join(smokeRoot, name);
      if (entry.type === "5") {
        mkdirSync(destination, {recursive: true});
      } else {
        mkdirSync(dirname(destination), {recursive: true});
        writeFileSync(destination, entry.bytes);
      }
    }
    const packageRoot = join(smokeRoot, "package");
    const native = join(packageRoot, manifest.binary.path);
    const launcher = join(packageRoot, "bin/minimax-codex.cjs");
    if (process.platform !== "win32") chmodSync(native, 0o755);
    const packagedBinarySha256 = sha256(readFileSync(native));
    if (packagedBinarySha256 !== manifest.binary.sha256) {
      fail("installed npm binary hash does not match the release manifest");
    }

    const directBinary = join(directRoot, manifest.binary.path);
    copyFileSync(sourceBinary, directBinary);
    if (process.platform !== "win32") chmodSync(directBinary, 0o755);
    const directBinarySha256 = sha256(readFileSync(directBinary));
    if (directBinarySha256 !== packagedBinarySha256) {
      fail("direct and installed Rust binaries do not have the same SHA-256");
    }

    const developmentRuntimeAugmented = installDevelopmentRuntime(manifest, directRoot)
      | installDevelopmentRuntime(manifest, packageRoot);
    const sourceVersionOutput = productIdentity(
      directBinary,
      [],
      directRoot,
      releaseSmokeEnvironment(directRoot),
      "direct Rust binary"
    );
    const installedVersionOutput = productIdentity(
      process.execPath,
      [launcher],
      packageRoot,
      releaseSmokeEnvironment(packageRoot),
      "installed npm launcher"
    );
    if (installedVersionOutput !== sourceVersionOutput) {
      fail("installed npm launcher identity differs from the direct Rust binary");
    }
    if (sourceVersionOutput !== `minimax-codex-rust ${manifest.version}`) {
      fail("Rust product identity does not match the package version");
    }

    const smoke = spawnSync(
      process.execPath,
      [launcher, "index", "capabilities", "status"],
      {
        cwd: packageRoot,
        encoding: "utf8",
        env: releaseSmokeEnvironment(packageRoot),
        shell: false,
        windowsHide: true,
        timeout: 10_000
      }
    );
    if (smoke.status !== 0 || !smoke.stdout.includes("domain=capability")) {
      fail(`installed npm Rust launcher smoke test failed: ${(smoke.stderr || smoke.stdout || "no output").trim()}`);
    }
    const siblingEvidence = verifyRejectedSiblings(entries.get("package/bin/minimax-codex.cjs").bytes, manifest, smokeRoot);
    return {
      archive: archive.replaceAll("\\", "/"),
      archiveSha256: hash,
      compressedBytes: bytes.length,
      installedRustIdentity: {
        sourceVersionOutput,
        installedVersionOutput,
        packagedBinarySha256,
        capabilityStatusSmoke: true,
        credentialsExcluded: true,
        pathLookupExcluded: true,
        developmentRuntimeAugmented: developmentRuntimeAugmented === 1,
        ...siblingEvidence
      }
    };
  } finally {
    rmSync(smokeRoot, {recursive: true, force: true});
    rmSync(directRoot, {recursive: true, force: true});
  }
}

function productIdentity(command, prefixArgs, cwd, env, label) {
  const result = spawnSync(command, [...prefixArgs, "--version"], {
    cwd,
    encoding: "utf8",
    env,
    shell: false,
    windowsHide: true,
    timeout: 10_000
  });
  if (result.status !== 0) {
    fail(`${label} identity failed: ${(result.stderr || result.stdout || "no output").trim()}`);
  }
  return result.stdout.trim();
}

function verifyRejectedSiblings(launcherBytes, manifest, smokeRoot) {
  const missingRoot = join(smokeRoot, "missing-sibling");
  const unsafeRoot = join(smokeRoot, "unsafe-sibling");
  for (const rootPath of [missingRoot, unsafeRoot]) {
    mkdirSync(join(rootPath, "bin"), {recursive: true});
    writeFileSync(join(rootPath, "bin/minimax-codex.cjs"), launcherBytes);
  }
  mkdirSync(join(unsafeRoot, manifest.binary.path));

  const missing = spawnSync(process.execPath, [join(missingRoot, "bin/minimax-codex.cjs"), "--version"], {
    cwd: missingRoot,
    encoding: "utf8",
    env: releaseSmokeEnvironment(missingRoot),
    shell: false,
    windowsHide: true,
    timeout: 10_000
  });
  const unsafe = spawnSync(process.execPath, [join(unsafeRoot, "bin/minimax-codex.cjs"), "--version"], {
    cwd: unsafeRoot,
    encoding: "utf8",
    env: releaseSmokeEnvironment(unsafeRoot),
    shell: false,
    windowsHide: true,
    timeout: 10_000
  });
  if (missing.status === 0 || !missing.stderr.includes("packaged Rust binary is missing")) {
    fail("installed launcher did not reject a missing sibling binary");
  }
  if (unsafe.status === 0 || !unsafe.stderr.includes("not a safe regular file")) {
    fail("installed launcher did not reject an unsafe sibling binary");
  }
  return {missingSiblingRejected: true, unsafeSiblingRejected: true};
}

function releaseSmokeEnvironment(isolatedRoot) {
  const environment = {
    HOME: isolatedRoot,
    PATH: "",
    TEMP: isolatedRoot,
    TMP: isolatedRoot,
    USERPROFILE: isolatedRoot
  };
  for (const name of ["ComSpec", "PATHEXT", "SystemRoot", "WINDIR"]) {
    if (typeof process.env[name] === "string") environment[name] = process.env[name];
  }
  return environment;
}

function installDevelopmentRuntime(manifest, destination) {
  if (manifest.target.id !== "windows-x86_64-gnullvm-dev") return 0;
  const sysroot = run("rustc", ["--print", "sysroot"]).trim();
  const runtime = join(sysroot, "lib/rustlib/x86_64-pc-windows-gnullvm/bin/libunwind.dll");
  if (!existsSync(runtime) || !lstatSync(runtime).isFile() || lstatSync(runtime).isSymbolicLink()) {
    fail("GNU-LLVM development runtime is missing or unsafe");
  }
  copyFileSync(runtime, join(destination, "libunwind.dll"));
  return 1;
}

function assertOnlyProductExecutables(entries, expectedExecutablePaths) {
  for (const [name, entry] of entries) {
    if (entry.type !== "0") continue;
    const executable = (entry.mode & 0o111) !== 0;
    if (expectedExecutablePaths.has(name) && !executable) {
      fail(`product entry is not executable: ${name}`);
    }
    if (!expectedExecutablePaths.has(name) && executable) {
      fail(`npm package contains an extra executable entry: ${name}`);
    }
  }
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

function releaseEnvironment(contract) {
  const rustc = run("rustc", ["-vV"]);
  const host = /^host:\s*(.+)$/mu.exec(rustc)?.[1]?.trim() ?? "";
  const rustcRelease = /^release:\s*(.+)$/mu.exec(rustc)?.[1]?.trim() ?? "";
  const expectedTarget = contract.targets.find((target) => target.rustcHost === host);
  if (!expectedTarget) fail(`unsupported Rust release host: ${host || "unknown"}`);
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
    expectedPlatform: expectedTarget.id,
    expectedSupportTier: expectedTarget.supportTier,
    expectedTarget
  };
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
    const mode = readTarOctal(header, 100, 8);
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
    entries.set(path, {type, mode, bytes: Buffer.from(tar.subarray(contentStart, contentEnd))});
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
