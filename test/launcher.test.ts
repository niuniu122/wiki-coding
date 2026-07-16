import assert from "node:assert/strict";
import {copyFile, chmod, mkdir, mkdtemp, readFile, rm, writeFile} from "node:fs/promises";
import {dirname, join, resolve} from "node:path";
import {spawnSync} from "node:child_process";
import test from "node:test";

const launcherSource = resolve("bin/minimax-codex.cjs");
const packagedBinary = process.platform === "win32" ? "minimax-codex.exe" : "minimax-codex";

test("fixed launcher forwards argv without a shell and preserves the Rust exit code", async () => {
  const fixture = await launcherFixture(true);
  try {
    const argumentProbe = join(fixture.root, "argument-probe.cjs");
    await writeFile(
      argumentProbe,
      "process.stdout.write(JSON.stringify(process.argv.slice(2)));\n",
      "utf8"
    );
    const forwarded = spawnSync(
      process.execPath,
      [fixture.launcher, argumentProbe, "中文 request", "$(not-a-shell)", "--flag=value"],
      {encoding: "utf8", shell: false, windowsHide: true}
    );
    assert.equal(forwarded.status, 0, forwarded.stderr);
    assert.deepEqual(JSON.parse(forwarded.stdout), ["中文 request", "$(not-a-shell)", "--flag=value"]);

    const exitProbe = join(fixture.root, "exit-probe.cjs");
    await writeFile(exitProbe, "process.exit(7);\n", "utf8");
    const exited = spawnSync(process.execPath, [fixture.launcher, exitProbe], {
      encoding: "utf8",
      shell: false,
      windowsHide: true
    });
    assert.equal(exited.status, 7);
  } finally {
    await rm(fixture.root, {recursive: true, force: true});
  }
});

test("missing Rust artifact fails clearly and never falls back to TypeScript", async () => {
  const fixture = await launcherFixture(false);
  try {
    const result = spawnSync(process.execPath, [fixture.launcher, "--version"], {
      encoding: "utf8",
      shell: false,
      windowsHide: true
    });
    assert.equal(result.status, 1);
    assert.match(result.stderr, /packaged Rust binary is missing/i);
    assert.match(result.stderr, /minimax-codex-legacy/);
    assert.equal(result.stdout, "");
  } finally {
    await rm(fixture.root, {recursive: true, force: true});
  }
});

test("launcher source has one fixed local binary map and no download or shell escape", async () => {
  const source = await readFile(launcherSource, "utf8");
  assert.match(source, /"win32:x64": "minimax-codex\.exe"/u);
  assert.match(source, /"linux:x64": "minimax-codex"/u);
  assert.match(source, /spawnSync\(binaryPath, process\.argv\.slice\(2\)/u);
  assert.match(source, /shell: false/u);
  assert.match(source, /isSymbolicLink/u);
  assert.doesNotMatch(source, /https?:|fetch\(|execSync|process\.env|dist\/cli/u);
});

async function launcherFixture(withBinary: boolean): Promise<{root: string; launcher: string}> {
  await mkdir(resolve("target"), {recursive: true});
  const root = await mkdtemp(resolve("target/launcher-test-"));
  const launcher = join(root, "bin/minimax-codex.cjs");
  await mkdir(dirname(launcher), {recursive: true});
  await copyFile(launcherSource, launcher);
  if (withBinary) {
    const binary = join(root, packagedBinary);
    await copyFile(process.execPath, binary);
    if (process.platform !== "win32") await chmod(binary, 0o755);
  }
  return {root, launcher};
}
