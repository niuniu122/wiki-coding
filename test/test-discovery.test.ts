import assert from "node:assert/strict";
import {spawn} from "node:child_process";
import {once} from "node:events";
import {access, mkdtemp, mkdir, readFile, rm, symlink, writeFile} from "node:fs/promises";
import {tmpdir} from "node:os";
import {dirname, extname, join, relative, resolve} from "node:path";
import {fileURLToPath} from "node:url";
import test from "node:test";
import {
  discoverTestFiles,
  importTestFiles,
  testModuleUrl
} from "./test-discovery.js";

const LOCAL_DEPENDENCY_PATTERN = /(?:\b(?:import|export)\s+(?:[^"'`]*?\s+from\s+)?|\bimport\s*\(\s*)["']([^"']+)["']/g;

async function auditEvaluatorReachability(
  repositoryRoot: string,
  testFiles: readonly string[]
): Promise<string[]> {
  const root = resolve(repositoryRoot);
  const evaluatorRoot = resolve(root, "src/eval");
  const queue = testFiles.map((path) => ({path: resolve(path), trace: [resolve(path)]}));
  const visited = new Set<string>();
  const violations = new Set<string>();

  while (queue.length > 0) {
    const current = queue.shift();
    if (!current || visited.has(current.path)) continue;
    visited.add(current.path);
    const source = await readFile(current.path, "utf8");
    for (const match of source.matchAll(LOCAL_DEPENDENCY_PATTERN)) {
      const specifier = match[1];
      if (!specifier?.startsWith(".")) continue;
      const dependency = await resolveTestDependency(current.path, specifier);
      if (!dependency) continue;
      const trace = [...current.trace, dependency];
      if (dependency === evaluatorRoot || dependency.startsWith(`${evaluatorRoot}\\`) || dependency.startsWith(`${evaluatorRoot}/`)) {
        violations.add(trace.map((path) => relative(root, path).replaceAll("\\", "/")).join(" -> "));
      } else {
        queue.push({path: dependency, trace});
      }
    }
  }
  return [...violations].sort();
}

async function resolveTestDependency(importer: string, specifier: string): Promise<string | undefined> {
  const normalized = specifier.replaceAll("\\", "/");
  const raw = resolve(dirname(importer), normalized);
  const extension = extname(raw);
  const candidates = extension === ".js" || extension === ".jsx"
    ? [raw.slice(0, -extension.length) + ".ts", raw.slice(0, -extension.length) + ".tsx"]
    : extension
      ? [raw]
      : [raw + ".ts", raw + ".tsx", join(raw, "index.ts"), join(raw, "index.tsx")];
  for (const candidate of candidates) {
    try {
      await access(candidate);
      return resolve(candidate);
    } catch {
      // Continue through the deterministic candidate list.
    }
  }
  return undefined;
}

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

test("discovered tests cannot reach TypeScript evaluators directly or transitively", async () => {
  const testRoot = dirname(fileURLToPath(import.meta.url));
  const repositoryRoot = resolve(testRoot, "..");
  const evaluatorTests = (await discoverTestFiles(testRoot)).filter((path) =>
    path.endsWith("provider-conformance.test.ts") ||
    path.endsWith("capability-retrieval-report.test.ts")
  );
  const violations = await auditEvaluatorReachability(repositoryRoot, evaluatorTests);

  assert.deepEqual(
    violations,
    [],
    `discovered TypeScript evaluator reachability must be empty:\n${violations.join("\n")}`
  );
});

test("dependency audit normalizes indirect re-export dynamic and platform paths", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-test-dependency-red-"));
  try {
    const files = new Map<string, string>([
      ["test/direct.test.ts", "import '../src/eval/provider-conformance.js';\n"],
      ["test/indirect.test.ts", "import './support/./bridge.js';\n"],
      ["test/support/bridge.ts", "import '../../src/other/../eval/capability-retrieval-report.js';\n"],
      ["test/reexport.test.ts", "export {providerReport} from '../src/eval/provider-conformance.js';\n"],
      ["test/dynamic.test.ts", "await import('./cycle-a.js');\n"],
      ["test/cycle-a.ts", "export * from './cycle-b.js';\n"],
      ["test/cycle-b.ts", "import './cycle-a.js';\nawait import('../src/eval/provider-conformance.js');\n"],
      ["test/windows.test.ts", String.raw`import '..\src\eval\provider-conformance.js';` + "\n"],
      ["src/eval/provider-conformance.ts", "export const providerReport = true;\n"],
      ["src/eval/capability-retrieval-report.ts", "export const retrievalReport = true;\n"]
    ]);
    await Promise.all([...files].map(async ([path, source]) => {
      const absolute = join(root, path);
      await mkdir(dirname(absolute), {recursive: true});
      await writeFile(absolute, source, "utf8");
    }));

    const discovered = await discoverTestFiles(join(root, "test"));
    const violations = await auditEvaluatorReachability(root, discovered);
    assert.deepEqual(
      violations,
      [],
      `normalized direct and transitive evaluator paths must fail closed:\n${violations.join("\n")}`
    );
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});
