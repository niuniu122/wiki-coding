import assert from "node:assert/strict";
import {createHash} from "node:crypto";
import {mkdir, mkdtemp, readFile, rm, writeFile} from "node:fs/promises";
import {resolve} from "node:path";
import {spawnSync} from "node:child_process";
import test from "node:test";

test("release packaging is deterministic and confines all writes to target", async () => {
  await mkdir(resolve("target"), {recursive: true});
  const temporary = await mkdtemp(resolve("target/release-package-test-"));
  const binary = resolve(temporary, "fixture.exe");
  const output = resolve(temporary, "artifacts");
  const archive = resolve(output, "minimax-codex-v9.8.7-windows-x86_64-msvc.tar.gz");
  try {
    await writeFile(binary, Buffer.from("deterministic-rust-binary-fixture", "utf8"));
    const args = [
      "scripts/release/package-rust.mjs",
      "--binary", binary,
      "--output", output,
      "--platform", "windows-x86_64-msvc",
      "--version", "9.8.7"
    ];
    const first = spawnSync(process.execPath, args, {cwd: resolve("."), encoding: "utf8", shell: false});
    assert.equal(first.status, 0, first.stderr);
    const firstHash = sha256(await readFile(archive));

    const second = spawnSync(process.execPath, args, {cwd: resolve("."), encoding: "utf8", shell: false});
    assert.equal(second.status, 0, second.stderr);
    assert.equal(sha256(await readFile(archive)), firstHash);

    const escapedVersion = spawnSync(
      process.execPath,
      [...args.slice(0, -2), "--version", "../../escape"],
      {cwd: resolve("."), encoding: "utf8", shell: false}
    );
    assert.notEqual(escapedVersion.status, 0);
    assert.match(escapedVersion.stderr, /version is invalid/i);

    const escapedOutput = spawnSync(
      process.execPath,
      [
        "scripts/release/package-rust.mjs",
        "--binary", binary,
        "--output", resolve("release-escape"),
        "--platform", "windows-x86_64-msvc",
        "--version", "9.8.7"
      ],
      {cwd: resolve("."), encoding: "utf8", shell: false}
    );
    assert.notEqual(escapedOutput.status, 0);
    assert.match(escapedOutput.stderr, /inside the repository target directory/i);
  } finally {
    await rm(temporary, {recursive: true, force: true});
  }
});

test("committed local release evidence is explicitly development-only and below every budget", async () => {
  const evidence = JSON.parse(
    await readFile(resolve("fixtures/compat/release/local-gnullvm-evidence.v1.json"), "utf8")
  ) as {
    evidenceClass: string;
    platform: string;
    package: {compressedBytes: number; embeddingIncluded: boolean; supportTier: string};
    performance: {coldStartP95Ms: number; idleRssMaximumBytes: number; wikiBm25P95Ms: number};
    offline: boolean;
    providerCalls: number;
    credentialsRead: number;
    modelDownloads: number;
  };
  const thresholds = JSON.parse(
    await readFile(resolve("fixtures/compat/release/thresholds.v1.json"), "utf8")
  ) as {coldStartMs: number; idleRssBytes: number; baseCompressedBytes: number; wikiBm25P95Ms: number};

  assert.equal(evidence.evidenceClass, "development_only");
  assert.equal(evidence.platform, "windows-x86_64-gnullvm-dev");
  assert.equal(evidence.package.supportTier, "development_only");
  assert.equal(evidence.package.embeddingIncluded, false);
  assert.ok(evidence.performance.coldStartP95Ms <= thresholds.coldStartMs);
  assert.ok(evidence.performance.idleRssMaximumBytes <= thresholds.idleRssBytes);
  assert.ok(evidence.package.compressedBytes <= thresholds.baseCompressedBytes);
  assert.ok(evidence.performance.wikiBm25P95Ms <= thresholds.wikiBm25P95Ms);
  assert.deepEqual(
    [evidence.offline, evidence.providerCalls, evidence.credentialsRead, evidence.modelDownloads],
    [true, 0, 0, 0]
  );
});

function sha256(bytes: Buffer): string {
  return createHash("sha256").update(bytes).digest("hex");
}
