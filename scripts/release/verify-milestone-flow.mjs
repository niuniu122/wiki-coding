import {existsSync, lstatSync, mkdirSync, readFileSync, readdirSync, writeFileSync} from "node:fs";
import {dirname, join, relative, resolve, sep} from "node:path";
import {spawnSync} from "node:child_process";
import {fileURLToPath} from "node:url";

import {
  loadExplicitFingerprint,
  loadTargetContract,
  validateFingerprintArtifactBinding,
  validateReleaseManifest
} from "./package-contract.mjs";
import {computeProductFingerprint} from "./product-fingerprint.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const targetRoot = resolve(root, "target");
const args = parseArgs(process.argv.slice(2));
if (!args["fingerprint-file"]) fail("E_FINGERPRINT_REQUIRED: --fingerprint-file is required");
if (!args.artifacts || !args["evidence-dir"]) {
  fail("E_ARGUMENT_REQUIRED: explicit --artifacts and --evidence-dir are required");
}
const artifacts = resolve(root, args.artifacts);
const evidenceDir = resolve(root, args["evidence-dir"]);
const fingerprintFile = resolve(root, args["fingerprint-file"]);
for (const [path, label] of [
  [artifacts, "release artifacts"],
  [evidenceDir, "release evidence"],
  [fingerprintFile, "product fingerprint file"]
]) assertWithinTarget(path, label);

const currentProduct = computeProductFingerprint(root);
const fingerprint = fingerprintInput(fingerprintFile, currentProduct);
const targetContract = loadTargetContract();
const {rustHost, platform, target} = releasePlatform(targetContract);
const manifest = artifactManifest(artifacts, targetContract);
try {
  validateFingerprintArtifactBinding(fingerprint, manifest.product);
} catch (error) {
  fail(error instanceof Error ? error.message : String(error));
}
if (manifest.target.id !== platform || manifest.target.rustcHost !== rustHost) {
  fail(`E_ARTIFACT_TARGET_MISMATCH: ${manifest.target.id}/${manifest.target.rustcHost} is not ${platform}/${rustHost}`);
}

const evidencePath = join(evidenceDir, `${platform}.json`);
if (!existsSync(evidencePath) || !lstatSync(evidencePath).isFile() || lstatSync(evidencePath).isSymbolicLink()) {
  fail(`release evidence is missing or unsafe: ${evidencePath}`);
}
let release;
try {
  release = JSON.parse(readFileSync(evidencePath, "utf8"));
} catch {
  fail("release evidence is not valid JSON");
}
validateCompleteReleaseEvidence(release, manifest, fingerprint, rustHost, platform, target);

const tests = ["tool_loop", "lifecycle_wiki", "discovery_commands", "migration"];
const tested = spawnSync(
  "cargo",
  ["test", "-p", "minimax-cli", "--locked", ...tests.flatMap((name) => ["--test", name])],
  {
    cwd: root,
    stdio: "inherit",
    env: {...process.env, CARGO_NET_OFFLINE: "true"},
    shell: false,
    windowsHide: true
  }
);
if (tested.status !== 0) fail(`cross-phase Rust tests failed with status ${tested.status ?? "unknown"}`);

const report = {
  schemaVersion: 3,
  platform,
  target: {
    id: manifest.target.id,
    rustcHost: manifest.target.rustcHost,
    os: manifest.target.os,
    arch: manifest.target.arch,
    supportTier: manifest.target.supportTier
  },
  productFingerprint: fingerprint.fingerprint,
  productFileCount: fingerprint.fileCount,
  fingerprintFile: fingerprintFile.replaceAll("\\", "/"),
  artifacts: {
    nativeArchiveSha256: manifest.nativeArchive.sha256,
    npmArchiveSha256: manifest.npmPackage.sha256,
    binarySha256: manifest.binary.sha256
  },
  gates: {
    rustEvaluations: {
      provider: "passed-before-milestone",
      retrieval: "passed-before-milestone"
    },
    sourceAuthorityAndCompatibility: "passed-before-milestone",
    migration: "passed:crates/cli/tests/migration.rs",
    packageCorruption: "passed-before-build",
    installedNative: "passed",
    installedNpm: "passed"
  },
  licenses: release.licenses,
  security: release.security,
  performance: release.performance,
  flows: {
    promptAndToolCompletion: "tool_loop",
    runtimeFinalizationWikiAndCurrentRetrieval: "lifecycle_wiki",
    bm25FirstProjectDiscovery: "discovery_commands",
    sourcePreservingMigrationAndNarrowRollback: "migration",
    nativeInstalledRustIdentity: release.package.nativeInstalledRustIdentity,
    npmInstalledRustIdentity: release.package.npmPackage.installedRustIdentity
  },
  offline: true,
  providerCalls: 0,
  credentialsRead: 0,
  modelDownloads: 0
};
mkdirSync(evidenceDir, {recursive: true});
writeFileSync(
  join(evidenceDir, `milestone-flow-${platform}.json`),
  `${JSON.stringify(report, null, 2)}\n`,
  "utf8"
);
process.stdout.write(`${JSON.stringify(report)}\n`);

function fingerprintInput(path, currentProduct) {
  try {
    return loadExplicitFingerprint(path, currentProduct);
  } catch (error) {
    fail(error instanceof Error ? error.message : String(error));
  }
}

function artifactManifest(directory, contract) {
  if (!existsSync(directory) || !lstatSync(directory).isDirectory() || lstatSync(directory).isSymbolicLink()) {
    fail("release artifacts directory is missing or unsafe");
  }
  const manifests = readdirSync(directory)
    .filter((name) => name.startsWith("minimax-codex-v") && name.endsWith("-RELEASE-MANIFEST.json"));
  if (manifests.length !== 1) fail(`expected exactly one release manifest, found ${manifests.length}`);
  try {
    return validateReleaseManifest(JSON.parse(readFileSync(join(directory, manifests[0]), "utf8")), contract);
  } catch (error) {
    fail(`release manifest is invalid: ${error instanceof Error ? error.message : String(error)}`);
  }
}

function releasePlatform(contract) {
  const rustc = spawnSync("rustc", ["-vV"], {cwd: root, encoding: "utf8", shell: false, windowsHide: true});
  if (rustc.status !== 0) fail("rustc -vV failed while selecting milestone evidence");
  const rustHost = /^host:\s*(.+)$/mu.exec(rustc.stdout)?.[1]?.trim() ?? "";
  const target = contract.targets.find((candidate) => candidate.rustcHost === rustHost);
  if (!target || (target.os === "win32" && process.platform !== "win32") || (target.os === "linux" && process.platform !== "linux")) {
    fail(`unsupported milestone-flow Rust host: ${rustHost || "unknown"}`);
  }
  return {rustHost, platform: target.id, target};
}

function validateCompleteReleaseEvidence(release, manifest, fingerprint, rustHost, platform, target) {
  const nativeIdentity = release.package?.nativeInstalledRustIdentity;
  const npmIdentity = release.package?.npmPackage?.installedRustIdentity;
  const identities = [nativeIdentity, npmIdentity];
  const expectedVersion = `minimax-codex-rust ${manifest.version}`;
  const identityInvalid = identities.some((identity) => !identity
    || identity.installedVersionOutput !== expectedVersion
    || identity.packagedBinarySha256 !== manifest.binary.sha256
    || identity.capabilityStatusSmoke !== true
    || identity.productFingerprint !== fingerprint.fingerprint
    || identity.offline !== true
    || identity.providerCalls !== 0
    || identity.credentialsRead !== 0
    || identity.modelDownloads !== 0);
  const targetInvalid = target.id !== manifest.target.id
    || target.rustcHost !== manifest.target.rustcHost
    || target.os !== manifest.target.os
    || target.arch !== manifest.target.arch
    || target.supportTier !== manifest.target.supportTier
    || release.environment?.expectedPlatform !== target.id
    || release.environment?.expectedSupportTier !== target.supportTier
    || JSON.stringify(release.environment?.expectedTarget) !== JSON.stringify(target);
  const licensesInvalid = !release.licenses
    || !Number.isSafeInteger(release.licenses.packagesChecked)
    || release.licenses.packagesChecked <= 0
    || release.licenses.invalid !== 0
    || release.licenses.policy !== "at_least_one_approved_permissive_choice";
  const securityInvalid = !release.security
    || release.security.unsafeFiles !== 0
    || release.security.unsafeWorkspaceLint !== "forbid"
    || release.security.databasePackages !== 0
    || release.security.migrationNetworkOrCredentialPaths !== 0;
  const performance = release.performance;
  const coldStart = performance?.coldStartMs;
  const idleRss = performance?.idleRssBytes;
  const performanceInvalid = !performance
    || !Array.isArray(coldStart?.samples)
    || coldStart.samples.length !== 9
    || !coldStart.samples.every((value) => Number.isFinite(value) && value > 0)
    || coldStart.p95 !== Math.max(...coldStart.samples)
    || !Array.isArray(idleRss?.samples)
    || idleRss.samples.length !== 5
    || !idleRss.samples.every((value) => Number.isSafeInteger(value) && value > 0)
    || idleRss.maximum !== Math.max(...idleRss.samples)
    || ![performance.baseCompressedBytes, performance.wikiBm25P95Ms]
      .every((value) => Number.isFinite(value) && value > 0)
    || coldStart.p95 > performance.coldStartLimitMs
    || idleRss.maximum > performance.idleRssLimitBytes
    || performance.baseCompressedBytes > performance.baseCompressedLimitBytes
    || performance.wikiBm25P95Ms > performance.wikiBm25P95LimitMs
    || performance.baseCompressedBytes !== release.package?.compressedBytes;
  if (release.schemaVersion !== 2
      || release.platform !== platform
      || release.environment?.rustcHost !== rustHost
      || targetInvalid
      || release.productFingerprint !== fingerprint.fingerprint
      || release.productFileCount !== fingerprint.fileCount
      || JSON.stringify(release.package?.manifest) !== JSON.stringify(manifest)
      || release.package?.archiveSha256 !== manifest.nativeArchive.sha256
      || release.package?.npmPackage?.archiveSha256 !== manifest.npmPackage.sha256
      || identityInvalid
      || nativeIdentity.installedVersionOutput !== npmIdentity.installedVersionOutput
      || nativeIdentity.packagedBinarySha256 !== npmIdentity.packagedBinarySha256
      || licensesInvalid
      || securityInvalid
      || performanceInvalid
      || release.offline !== true
      || release.providerCalls !== 0
      || release.credentialsRead !== 0
      || release.modelDownloads !== 0) {
    fail("release evidence does not prove both current installed artifact paths");
  }
}

function parseArgs(values) {
  const parsed = {};
  const allowed = new Set(["artifacts", "evidence-dir", "fingerprint-file"]);
  for (let index = 0; index < values.length; index += 1) {
    const key = values[index];
    const name = key?.startsWith("--") ? key.slice(2) : "";
    if (!allowed.has(name) || index + 1 >= values.length || Object.hasOwn(parsed, name)) {
      fail(`invalid milestone argument: ${key ?? ""}`);
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

function fail(message) {
  process.stderr.write(`milestone flow verification failed: ${message}\n`);
  process.exit(1);
}
