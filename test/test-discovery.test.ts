import assert from "node:assert/strict";
import {spawn} from "node:child_process";
import {once} from "node:events";
import {mkdtemp, mkdir, readFile, rm, symlink, writeFile} from "node:fs/promises";
import {tmpdir} from "node:os";
import {dirname, join, resolve} from "node:path";
import {fileURLToPath} from "node:url";
import test from "node:test";
import {
  discoverTestFiles,
  importTestFiles,
  testModuleUrl
} from "./test-discovery.js";

test("discovery recursively returns only test modules in stable absolute-path order", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-test-discovery-"));
  const suiteRoot = join(root, "suite");
  const nestedRoot = join(suiteRoot, "nested");

  try {
    await mkdir(nestedRoot, {recursive: true});
    const included = [
      join(suiteRoot, "zeta.test.ts"),
      join(nestedRoot, "alpha.test.tsx"),
      join(nestedRoot, "Beta.test.ts")
    ];
    const ignored = [
      join(suiteRoot, "ordinary.ts"),
      join(suiteRoot, "types.test.d.ts"),
      join(suiteRoot, "similar.test.ts.bak"),
      join(nestedRoot, "almost.tests.ts"),
      join(nestedRoot, "specimen.spec.ts")
    ];
    await Promise.all([...included, ...ignored].map((path) => writeFile(path, "", "utf8")));

    const discovered = await discoverTestFiles(suiteRoot);
    assert.deepEqual(discovered, included.map((path) => resolve(path)).sort());
    assert.equal(discovered.every((path) => path.startsWith(resolve(suiteRoot))), true);
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("discovery does not follow directory symlinks or junctions", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-test-symlink-"));
  const suiteRoot = join(root, "suite");
  const externalRoot = join(root, "external");

  try {
    await mkdir(suiteRoot, {recursive: true});
    await mkdir(externalRoot, {recursive: true});
    await writeFile(join(externalRoot, "linked.test.ts"), "", "utf8");
    await symlink(
      externalRoot,
      join(suiteRoot, "linked"),
      process.platform === "win32" ? "junction" : "dir"
    );

    assert.deepEqual(await discoverTestFiles(suiteRoot), []);
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("test module URLs round-trip an existing platform path safely", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-test-url-"));
  const path = join(root, "spaces and #hash.test.ts");

  try {
    await writeFile(path, "", "utf8");
    const url = testModuleUrl(path);
    assert.equal(url.protocol, "file:");
    assert.equal(fileURLToPath(url), resolve(path));
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("test modules import sequentially in ordinal path order", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-test-import-order-"));
  const stateKey = `__minimax_test_order_${Date.now()}_${Math.random()}`;
  const globalState = globalThis as typeof globalThis & Record<string, string[]>;

  try {
    const first = join(root, "a.test.ts");
    const second = join(root, "b.test.ts");
    await writeFile(first, `globalThis[${JSON.stringify(stateKey)}] = ["a"];`, "utf8");
    await writeFile(
      second,
      `globalThis[${JSON.stringify(stateKey)}].push("b");`,
      "utf8"
    );

    await importTestFiles([second, first]);
    assert.deepEqual(globalState[stateKey], ["a", "b"]);
  } finally {
    delete globalState[stateKey];
    await rm(root, {recursive: true, force: true});
  }
});

test("a test module load failure rejects the runner operation", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-test-import-failure-"));
  const broken = join(root, "broken.test.ts");

  try {
    await writeFile(broken, "throw new Error('fixture-load-failure');", "utf8");
    await assert.rejects(() => importTestFiles([broken]), /fixture-load-failure/);
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("an unhandled test module load failure exits a runner process nonzero", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-test-process-failure-"));
  const broken = join(root, "broken.test.ts");
  const discoveryUrl = new URL("./test-discovery.ts", import.meta.url).href;

  try {
    await writeFile(broken, "throw new Error('process-load-failure');", "utf8");
    const script = [
      `const {importTestFiles} = await import(${JSON.stringify(discoveryUrl)});`,
      `await importTestFiles([${JSON.stringify(broken)}]);`
    ].join("\n");
    const child = spawn(
      process.execPath,
      ["--import", "tsx", "--input-type=module", "--eval", script],
      {stdio: "ignore"}
    );
    const [code, signal] = await once(child, "exit") as [number | null, NodeJS.Signals | null];

    assert.equal(signal, null);
    assert.notEqual(code, 0);
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("the project runner delegates to discovery instead of maintaining a test registry", async () => {
  const runnerPath = join(dirname(fileURLToPath(import.meta.url)), "run-tests.ts");
  const runner = await readFile(runnerPath, "utf8");

  assert.match(runner, /discoverTestFiles/);
  assert.match(runner, /importTestFiles/);
  assert.doesNotMatch(runner, /\.test\.(?:js|jsx|ts|tsx)/);
});
