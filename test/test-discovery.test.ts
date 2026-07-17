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
  testModuleUrl,
  validateDiscoveredTestGraph
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
  assert.match(runner, /validateDiscoveredTestGraph/);
  assert.match(runner, /importTestFiles/);
  assert.doesNotMatch(runner, /\.test\.(?:js|jsx|ts|tsx)/);
  assert.ok(
    runner.indexOf("await validateDiscoveredTestGraph") < runner.indexOf("await importTestFiles"),
    "the graph must be validated before the first discovered module import"
  );
});

test("discovered tests cannot reach TypeScript evaluators directly or transitively", async () => {
  const testRoot = dirname(fileURLToPath(import.meta.url));
  const repositoryRoot = resolve(testRoot, "..");
  await validateDiscoveredTestGraph(repositoryRoot, await discoverTestFiles(testRoot));
});

test("dependency audit normalizes indirect re-export dynamic and platform paths", async () => {
  const variants: readonly (readonly [string, ReadonlyMap<string, string>])[] = [
    ["direct", new Map([
      ["test/direct.test.ts", "import '../src/eval/provider-conformance.js';\n"],
      ["src/eval/provider-conformance.ts", "export const providerReport = true;\n"]
    ])],
    ["indirect normalized path", new Map([
      ["test/indirect.test.ts", "import './support/./bridge.js';\n"],
      ["test/support/bridge.ts", "import '../../src/other/../eval/capability-retrieval-report.js';\n"],
      ["src/eval/capability-retrieval-report.ts", "export const retrievalReport = true;\n"]
    ])],
    ["re-export", new Map([
      ["test/reexport.test.ts", "export {providerReport} from '../src/eval/provider-conformance.js';\n"],
      ["src/eval/provider-conformance.ts", "export const providerReport = true;\n"]
    ])],
    ["literal dynamic import through cycle", new Map([
      ["test/dynamic.test.ts", "await import('./cycle-a.js');\n"],
      ["test/cycle-a.ts", "export * from './cycle-b.js';\n"],
      ["test/cycle-b.ts", "import './cycle-a.js';\nawait import('../src/eval/provider-conformance.js');\n"],
      ["src/eval/provider-conformance.ts", "export const providerReport = true;\n"]
    ])],
    ["Windows separator and JS-to-TSX mapping", new Map([
      ["test/windows.test.ts", "import '..\\\\src\\\\eval\\\\provider-conformance.js';\n"],
      ["src/eval/provider-conformance.tsx", "export const providerReport = true;\n"]
    ])]
  ];
  for (const [label, files] of variants) {
    await withSyntheticGraph(files, async (root, discovered) => {
      await assert.rejects(
        () => validateDiscoveredTestGraph(root, discovered),
        new RegExp(`TypeScript evaluator.*${label === "indirect normalized path" ? "indirect" : ""}`, "is")
      );
    });
  }
});

test("dependency audit handles cycles and fails closed on unsafe ambiguous unresolved and symlinked paths", async () => {
  await withSyntheticGraph(new Map([
    ["test/cycle.test.ts", "import './cycle-a.js';\nimport './regex-fixture.js';\n"],
    ["test/cycle-a.ts", "export * from './cycle-b.js';\n"],
    ["test/cycle-b.ts", "import './cycle-a.js';\n"],
    ["test/regex-fixture.ts", String.raw`const importLike = /import\s+['"]\.\.\/src\/eval/g;` + "\nexport {importLike};\n"]
  ]), async (root, discovered) => validateDiscoveredTestGraph(root, discovered));

  for (const [label, files, expected] of [
    ["unresolved", new Map([["test/unresolved.test.ts", "import './missing.js';\n"]]), /Unresolved local TypeScript dependency/],
    ["ambiguous", new Map([
      ["test/ambiguous.test.ts", "import './helper.js';\n"],
      ["test/helper.ts", "export {};\n"],
      ["test/helper.tsx", "export {};\n"]
    ]), /Ambiguous local TypeScript dependency/],
    ["unsafe", new Map([["test/unsafe.test.ts", "import '../../outside.js';\n"]]), /Unsafe local dependency.*escapes repository/]
  ] as const) {
    await withSyntheticGraph(files, async (root, discovered) => {
      await assert.rejects(() => validateDiscoveredTestGraph(root, discovered), expected, label);
    });
  }

  const root = await mkdtemp(join(tmpdir(), "minimax-test-dependency-symlink-"));
  const external = await mkdtemp(join(tmpdir(), "minimax-test-dependency-external-"));
  try {
    await writeSyntheticGraph(root, new Map([
      ["test/symlink.test.ts", "import './linked/helper.js';\n"]
    ]));
    await writeSyntheticGraph(external, new Map([
      ["helper.ts", "export {};\n"]
    ]));
    await symlink(external, join(root, "test/linked"), process.platform === "win32" ? "junction" : "dir");
    await assert.rejects(
      () => validateDiscoveredTestGraph(root, [join(root, "test/symlink.test.ts")]),
      /dependency path is symlinked/
    );
  } finally {
    await rm(root, {recursive: true, force: true});
    await rm(external, {recursive: true, force: true});
  }
});

async function withSyntheticGraph(
  files: ReadonlyMap<string, string>,
  operation: (root: string, discovered: string[]) => Promise<unknown>
): Promise<void> {
  const root = await mkdtemp(join(tmpdir(), "minimax-test-dependency-"));
  try {
    await writeSyntheticGraph(root, files);
    await operation(root, await discoverTestFiles(join(root, "test")));
  } finally {
    await rm(root, {recursive: true, force: true});
  }
}

async function writeSyntheticGraph(root: string, files: ReadonlyMap<string, string>): Promise<void> {
  await Promise.all([...files].map(async ([path, source]) => {
    const absolute = join(root, path);
    await mkdir(dirname(absolute), {recursive: true});
    await writeFile(absolute, source, "utf8");
  }));
}
