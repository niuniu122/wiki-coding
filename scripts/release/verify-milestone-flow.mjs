import {existsSync, mkdirSync, readFileSync, writeFileSync} from "node:fs";
import {dirname, join, resolve} from "node:path";
import {spawnSync} from "node:child_process";
import {fileURLToPath} from "node:url";

import {computeProductFingerprint} from "./product-fingerprint.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const platform = process.platform === "win32"
  ? "windows-x86_64-msvc"
  : process.platform === "linux"
    ? "linux-x86_64-gnu"
    : fail(`unsupported milestone-flow platform: ${process.platform}`);

const tests = ["tool_loop", "lifecycle_wiki", "discovery_commands", "migration"];
const tested = spawnSync(
  "cargo",
  ["test", "-p", "minimax-cli", "--locked", ...tests.flatMap((name) => ["--test", name])],
  {cwd: root, stdio: "inherit", shell: false}
);
if (tested.status !== 0) fail(`cross-phase Rust tests failed with status ${tested.status ?? "unknown"}`);

const preferredEvidencePath = join(root, "target/release-evidence", `${platform}.json`);
const developmentPlatform = process.platform === "win32" ? "windows-x86_64-gnullvm-dev" : undefined;
const developmentEvidencePath = developmentPlatform
  ? join(root, "target/release-evidence", `${developmentPlatform}.json`)
  : undefined;
const evidencePath = existsSync(preferredEvidencePath)
  ? preferredEvidencePath
  : developmentEvidencePath && existsSync(developmentEvidencePath)
    ? developmentEvidencePath
    : preferredEvidencePath;
if (!existsSync(evidencePath)) fail(`release evidence is missing: ${evidencePath}`);
const release = JSON.parse(readFileSync(evidencePath, "utf8"));
const evidencePlatform = release.platform;
const product = computeProductFingerprint(root);
if (release.schemaVersion !== 1
    || (evidencePlatform !== platform && evidencePlatform !== developmentPlatform)
    || release.productFingerprint !== product.fingerprint
    || release.productFileCount !== product.fileCount
    || release.package?.npmPackage?.rustDefaultSmoke !== true
    || release.package?.npmPackage?.legacyBin !== "dist/cli.js"
    || release.offline !== true
    || release.providerCalls !== 0
    || release.credentialsRead !== 0
    || release.modelDownloads !== 0) {
  fail("release evidence does not prove the current complete installed product flow");
}

const report = {
  schemaVersion: 1,
  platform: evidencePlatform,
  productFingerprint: product.fingerprint,
  productFileCount: product.fileCount,
  flows: {
    promptAndToolCompletion: "tool_loop",
    runtimeFinalizationWikiAndCurrentRetrieval: "lifecycle_wiki",
    bm25FirstProjectDiscovery: "discovery_commands",
    sourcePreservingMigrationAndNarrowRollback: "migration",
    installedRustDefaultSmoke: true,
    explicitLegacyBin: "dist/cli.js"
  },
  offline: true,
  providerCalls: 0,
  credentialsRead: 0,
  modelDownloads: 0
};
mkdirSync(dirname(evidencePath), {recursive: true});
writeFileSync(
  join(root, "target/release-evidence", `milestone-flow-${evidencePlatform}.json`),
  `${JSON.stringify(report, null, 2)}\n`,
  "utf8"
);
process.stdout.write(`${JSON.stringify(report)}\n`);

function fail(message) {
  process.stderr.write(`milestone flow verification failed: ${message}\n`);
  process.exit(1);
}
