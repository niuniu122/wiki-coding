import {
  existsSync,
  lstatSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  writeFileSync
} from "node:fs";
import {basename, dirname, relative, resolve, sep} from "node:path";
import {spawnSync} from "node:child_process";
import {fileURLToPath} from "node:url";

import {
  createPublishedPackageJson,
  createDeterministicTarGzip,
  expectedArchiveEntries,
  expectedUniversalNpmEntries,
  loadExplicitFingerprint,
  loadReleaseThresholds,
  loadTargetContract,
  parseTarGzip,
  sha256,
  validateArtifactCandidate,
  validateReleaseManifest,
  validateUniversalNpmCandidate,
  validateUniversalNpmManifest
} from "./package-contract.mjs";
import {computeProductFingerprint} from "./product-fingerprint.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const targetRoot = resolve(root, "target");
const args = parseArgs(process.argv.slice(2));
const packageJson = JSON.parse(readFileSync(resolve(root, "package.json"), "utf8"));
if (args.mode === "universal-npm") {
  packageUniversalNpm(args, packageJson);
} else {
  packagePlatform(args, packageJson);
}

function packagePlatform(options, sourcePackage) {
  const version = options.version ?? sourcePackage.version;
  for (const required of ["binary", "output", "fingerprint-file"]) {
    if (!options[required]) fail(`E_FINGERPRINT_REQUIRED: explicit --binary, --output, and --fingerprint-file are required`);
  }
  const binary = resolve(root, options.binary);
  const output = resolve(root, options.output);
  const fingerprintFile = resolve(root, options["fingerprint-file"]);
  const targetContract = loadTargetContract();
  const rustcHost = detectRustcHost();
  const target = targetContract.targets.find((candidate) => candidate.rustcHost === rustcHost);

  if (!target) fail(`unsupported release host: ${rustcHost || "unknown"}`);
  if (!/^[0-9A-Za-z][0-9A-Za-z.+-]{0,63}$/u.test(version)) fail("release version is invalid");
  assertWithinTarget(binary, "release binary");
  assertWithinTarget(output, "release output");
  assertWithinTarget(fingerprintFile, "product fingerprint file");
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
  const materializeEntries = (descriptors, channel) => {
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
  };

  const nativeEntries = materializeEntries(expectedArchiveEntries(target, version, "native"), "native");
  const npmEntries = materializeEntries(expectedArchiveEntries(target, version, "npm"), "npm");
  const nativeBytes = createDeterministicTarGzip(nativeEntries);
  const npmBytes = createDeterministicTarGzip(npmEntries);
  const baseName = `minimax-codex-v${version}-${target.id}`;
  const archiveName = `${baseName}${target.archiveSuffix}`;
  const npmName = `${baseName}-npm.tgz`;
  const manifestName = `${baseName}-RELEASE-MANIFEST.json`;
  const currentProduct = computeProductFingerprint(root);
  const product = loadFingerprint(fingerprintFile, currentProduct);
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
    fingerprintFile,
    productFingerprint: product.fingerprint,
    version
  })}\n`);
}

function packageUniversalNpm(options, sourcePackage) {
  const version = options.version;
  const output = resolve(root, options.output);
  const fingerprintFile = resolve(root, options["fingerprint-file"]);
  const windowsArtifacts = resolve(root, options["windows-artifacts"]);
  const linuxArtifacts = resolve(root, options["linux-artifacts"]);
  for (const [path, label] of [
    [output, "universal npm output"],
    [fingerprintFile, "product fingerprint file"],
    [windowsArtifacts, "Windows release artifacts"],
    [linuxArtifacts, "Linux release artifacts"]
  ]) {
    assertWithinTarget(path, label);
  }
  if (!/^[0-9A-Za-z][0-9A-Za-z.+-]{0,63}$/u.test(version)) {
    fail("E_UNIVERSAL_ARGUMENTS: universal npm release version is invalid");
  }

  const contract = loadTargetContract();
  const thresholds = loadReleaseThresholds();
  const currentProduct = computeProductFingerprint(root);
  const product = loadFingerprint(fingerprintFile, currentProduct);
  const linux = loadPlatformCandidate(
    linuxArtifacts,
    "linux-x86_64-gnu",
    version,
    product,
    contract
  );
  const windows = loadPlatformCandidate(
    windowsArtifacts,
    "windows-x86_64-msvc",
    version,
    product,
    contract
  );
  assertSharedPlatformInputs(linux, windows);

  const publishedPackage = createPublishedPackageJson(sourcePackage, version);
  const publishedPackageBytes = Buffer.from(`${JSON.stringify(publishedPackage, null, 2)}\n`, "utf8");
  const linuxEntries = new Map(linux.npmEntries.map((entry) => [entry.name, entry]));
  const windowsEntries = new Map(windows.npmEntries.map((entry) => [entry.name, entry]));
  const sourceBytes = new Map();
  for (const path of sharedUniversalPaths()) {
    if (path === "package/package.json") continue;
    const entry = linuxEntries.get(path);
    if (!entry || entry.type !== "0") fail(`UNIVERSAL_INPUT_DRIFT: missing shared input ${path}`);
    sourceBytes.set(path, entry.bytes);
  }
  sourceBytes.set("package/package.json", publishedPackageBytes);
  sourceBytes.set("package/minimax-codex", requiredArchiveEntry(linuxEntries, "package/minimax-codex").bytes);
  sourceBytes.set("package/minimax-codex.exe", requiredArchiveEntry(windowsEntries, "package/minimax-codex.exe").bytes);

  const universalEntries = expectedUniversalNpmEntries(version).map((descriptor) => {
    if (descriptor.type === "directory") {
      return {name: descriptor.path, bytes: Buffer.alloc(0), mode: descriptor.mode, type: "5"};
    }
    const bytes = sourceBytes.get(descriptor.path);
    if (!bytes) fail(`UNIVERSAL_INPUT_DRIFT: canonical universal entry has no verified bytes: ${descriptor.path}`);
    return {name: descriptor.path, bytes, mode: descriptor.mode, type: "0"};
  });
  const archiveBytes = createDeterministicTarGzip(universalEntries);
  const archiveName = `minimax-codex-${version}.tgz`;
  const manifestName = `minimax-codex-v${version}-NPM-MANIFEST.json`;
  const launcherBytes = sourceBytes.get("package/bin/minimax-codex.cjs");
  const linuxBinary = sourceBytes.get("package/minimax-codex");
  const windowsBinary = sourceBytes.get("package/minimax-codex.exe");
  const manifest = {
    schemaVersion: 1,
    name: "minimax-codex",
    version,
    product: {fingerprint: product.fingerprint, fileCount: product.fileCount},
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
        bytes: linuxBinary.length,
        sha256: sha256(linuxBinary)
      },
      {
        targetId: "windows-x86_64-msvc",
        path: "minimax-codex.exe",
        mode: 0o755,
        bytes: windowsBinary.length,
        sha256: sha256(windowsBinary)
      }
    ],
    npmPackage: {
      name: archiveName,
      bytes: archiveBytes.length,
      sha256: sha256(archiveBytes),
      entries: entryEvidence(universalEntries)
    }
  };
  const checksumBytes = Buffer.from(`${manifest.npmPackage.sha256}  ${archiveName}\n`, "utf8");
  requireContract(() => validateUniversalNpmManifest(manifest, contract, thresholds));
  requireContract(() => validateUniversalNpmCandidate({
    manifest,
    contract,
    thresholds,
    expectedProduct: product,
    archiveBytes,
    checksumBytes
  }));

  const archivePath = resolve(output, archiveName);
  const checksumPath = `${archivePath}.sha256`;
  const manifestPath = resolve(output, manifestName);
  for (const path of [archivePath, checksumPath, manifestPath]) {
    if (existsSync(path)) fail(`UNIVERSAL_OUTPUT_EXISTS: release output already exists: ${path}`);
  }
  prepareOutputDirectory(output);
  writeFileSync(archivePath, archiveBytes, {flag: "wx"});
  writeFileSync(checksumPath, checksumBytes, {flag: "wx"});
  writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`, {encoding: "utf8", flag: "wx"});
  process.stdout.write(`${JSON.stringify({
    schemaVersion: 1,
    mode: "universal-npm",
    manifest: manifestPath,
    npmArchive: archivePath,
    npmSha256: manifest.npmPackage.sha256,
    productFingerprint: product.fingerprint,
    version
  })}\n`);
}

function loadPlatformCandidate(directory, expectedTargetId, version, expectedProduct, contract) {
  if (!existsSync(directory)) {
    fail(`UNIVERSAL_INPUT_UNSAFE: artifact input is missing: ${directory}`);
  }
  const status = lstatSync(directory);
  if (!status.isDirectory() || status.isSymbolicLink()) {
    fail(`UNIVERSAL_INPUT_UNSAFE: artifact input is not a safe directory: ${directory}`);
  }
  const records = readdirSync(directory, {withFileTypes: true});
  if (records.some((entry) => !entry.isFile() || entry.isSymbolicLink())) {
    fail(`UNIVERSAL_INPUT_UNSAFE: artifact directory contains a non-regular file: ${directory}`);
  }
  const names = records.map((entry) => entry.name).sort();
  const manifestNames = names.filter((name) => name.endsWith("-RELEASE-MANIFEST.json"));
  if (manifestNames.length !== 1) {
    fail(`UNIVERSAL_INPUT_FILE_SET: ${expectedTargetId} must contain exactly one release manifest`);
  }
  let manifest;
  try {
    manifest = JSON.parse(readRegularFile(resolve(directory, manifestNames[0]), `${expectedTargetId} manifest`).toString("utf8"));
  } catch (error) {
    fail(`UNIVERSAL_INPUT_INVALID: cannot parse ${expectedTargetId} manifest: ${error instanceof Error ? error.message : String(error)}`);
  }
  const expectedManifestName = `minimax-codex-v${version}-${expectedTargetId}-RELEASE-MANIFEST.json`;
  if (manifest.version !== version) {
    fail(`UNIVERSAL_INPUT_VERSION: ${expectedTargetId} manifest version does not match ${version}`);
  }
  const artifacts = new Map(names
    .filter((name) => name !== manifestNames[0])
    .map((name) => [name, {kind: "file", bytes: readRegularFile(resolve(directory, name), `${expectedTargetId} artifact`)}]));
  requireContract(() => validateArtifactCandidate({
    manifest,
    contract,
    expectedTargetId,
    expectedProduct,
    artifacts
  }));
  if (manifestNames[0] !== expectedManifestName) {
    fail(`UNIVERSAL_INPUT_FILE_SET: ${expectedTargetId} manifest name is not canonical`);
  }
  const npmBytes = artifacts.get(manifest.npmPackage.name)?.bytes;
  if (!npmBytes) fail(`UNIVERSAL_INPUT_FILE_SET: ${expectedTargetId} npm package is missing`);
  const npmEntries = requireContract(() => parseTarGzip(npmBytes, manifest.npmPackage.name));
  return {manifest, npmEntries};
}

function assertSharedPlatformInputs(linux, windows) {
  if (linux.manifest.version !== windows.manifest.version
      || JSON.stringify(linux.manifest.product) !== JSON.stringify(windows.manifest.product)
      || JSON.stringify(linux.manifest.launcher) !== JSON.stringify(windows.manifest.launcher)) {
    fail("UNIVERSAL_INPUT_DRIFT: platform manifests do not bind one version, product, and launcher");
  }
  const linuxEntries = new Map(linux.npmEntries.map((entry) => [entry.name, entry]));
  const windowsEntries = new Map(windows.npmEntries.map((entry) => [entry.name, entry]));
  for (const path of sharedUniversalPaths()) {
    const left = linuxEntries.get(path);
    const right = windowsEntries.get(path);
    if (!left
        || !right
        || left.type !== "0"
        || right.type !== "0"
        || left.mode !== right.mode
        || !left.bytes.equals(right.bytes)) {
      fail(`UNIVERSAL_INPUT_DRIFT: platform shared content differs at ${path}`);
    }
    const relativePath = path.slice("package/".length);
    const currentBytes = readRegularFile(resolve(root, relativePath), `current ${relativePath}`);
    if (!left.bytes.equals(currentBytes)) {
      fail(`UNIVERSAL_INPUT_DRIFT: verified shared content is stale at ${path}`);
    }
  }
}

function sharedUniversalPaths() {
  return [
    "package/bin/minimax-codex.cjs",
    "package/README.md",
    "package/LICENSE-APACHE",
    "package/LICENSE-MIT",
    "package/docs/release/cutover.md",
    "package/docs/release/embedding-package.md",
    "package/docs/release/install-upgrade-rollback.md",
    "package/docs/release/subprocess-sandbox.md",
    "package/package.json"
  ];
}

function requiredArchiveEntry(entries, path) {
  const entry = entries.get(path);
  if (!entry || entry.type !== "0") fail(`UNIVERSAL_INPUT_DRIFT: verified package is missing ${path}`);
  return entry;
}

function prepareOutputDirectory(output) {
  if (existsSync(output)) {
    const status = lstatSync(output);
    if (!status.isDirectory() || status.isSymbolicLink()) {
      fail(`UNIVERSAL_OUTPUT_UNSAFE: output is not a safe directory: ${output}`);
    }
    if (readdirSync(output).length !== 0) {
      fail(`UNIVERSAL_OUTPUT_NOT_EMPTY: output directory must be empty: ${output}`);
    }
  } else {
    mkdirSync(output, {recursive: true});
  }
}

function loadFingerprint(path, currentProduct) {
  try {
    return loadExplicitFingerprint(path, currentProduct);
  } catch (error) {
    fail(error instanceof Error ? error.message : String(error));
  }
}

function requireContract(operation) {
  try {
    return operation();
  } catch (error) {
    fail(error instanceof Error ? error.message : String(error));
  }
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

function detectRustcHost() {
  const output = spawnSync("rustc", ["-vV"], {cwd: root, encoding: "utf8", shell: false, windowsHide: true});
  if (output.status !== 0) fail("rustc -vV failed while detecting the release target");
  return /^host:\s*(.+)$/mu.exec(output.stdout)?.[1]?.trim() ?? "";
}

function parseArgs(values) {
  const universal = values.includes("--universal-npm");
  const parsed = {mode: universal ? "universal-npm" : "platform"};
  const allowed = universal
    ? new Set(["fingerprint-file", "linux-artifacts", "output", "universal-npm", "version", "windows-artifacts"])
    : new Set(["binary", "fingerprint-file", "output", "version"]);
  for (let index = 0; index < values.length; index += 1) {
    const key = values[index];
    const name = key?.startsWith("--") ? key.slice(2) : "";
    if (!allowed.has(name) || Object.hasOwn(parsed, name)) {
      fail(`invalid package argument: ${key ?? ""}`);
    }
    if (name === "universal-npm") {
      parsed[name] = true;
      continue;
    }
    if (index + 1 >= values.length || values[index + 1]?.startsWith("--")) {
      fail(`invalid package argument: ${key ?? ""}`);
    }
    parsed[name] = values[index + 1];
    index += 1;
  }
  if (universal) {
    for (const required of ["fingerprint-file", "linux-artifacts", "output", "universal-npm", "version", "windows-artifacts"]) {
      if (!Object.hasOwn(parsed, required)) {
        fail(`E_UNIVERSAL_ARGUMENTS: missing --${required}`);
      }
    }
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

function fail(message) {
  process.stderr.write(`release packaging failed: ${message}\n`);
  process.exit(1);
}
