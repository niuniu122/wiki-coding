import {lstat, readFile, readdir} from "node:fs/promises";
import {dirname, extname, isAbsolute, join, relative, resolve} from "node:path";
import {pathToFileURL, type URL} from "node:url";
import ts from "typescript";

const TEST_MODULE_PATTERN = /\.test\.tsx?$/;

export async function discoverTestFiles(testRoot: string): Promise<string[]> {
  const files: string[] = [];
  await collectTestFiles(resolve(testRoot), files);
  return files.sort();
}

export async function importTestFiles(testFiles: readonly string[]): Promise<void> {
  const orderedFiles = testFiles.map((path) => resolve(path)).sort();
  for (const path of orderedFiles) {
    await import(testModuleUrl(path).href);
  }
}

export async function validateDiscoveredTestGraph(
  repositoryRoot: string,
  testFiles: readonly string[]
): Promise<void> {
  const root = resolve(repositoryRoot);
  const testRoot = resolve(root, "test");
  const evaluatorRoot = resolve(root, "src/eval");
  const queue = [...testFiles]
    .map((path) => ({path: resolve(path), trace: [resolve(path)]}))
    .sort((left, right) => left.path.localeCompare(right.path));
  const visited = new Set<string>();

  while (queue.length > 0) {
    const current = queue.shift();
    if (!current) break;
    const key = normalizedPathKey(current.path);
    if (visited.has(key)) continue;
    visited.add(key);
    assertInsideRoot(root, current.path, "dependency importer");
    if (current.trace.length === 1) {
      assertInsideRoot(testRoot, current.path, "discovered test");
      if (!TEST_MODULE_PATTERN.test(current.path)) {
        throw new Error(`Discovered test path has an unsupported extension: ${repositoryPath(root, current.path)}`);
      }
    }
    await requireRegularUnsymbolicPath(root, current.path);
    const source = await readFile(current.path, "utf8");
    for (const specifier of extractLocalModuleSpecifiers(current.path, source)) {
      const dependency = await resolveLocalTypeScriptDependency(root, current.path, specifier);
      const trace = [...current.trace, dependency];
      if (isWithin(evaluatorRoot, dependency)) {
        throw new Error(
          `TypeScript evaluator is reachable from a discovered test: ${trace
            .map((path) => repositoryPath(root, path))
            .join(" -> ")}`
        );
      }
      if (/\.tsx?$/i.test(dependency)) queue.push({path: dependency, trace});
    }
  }
}

export function testModuleUrl(path: string): URL {
  return pathToFileURL(resolve(path));
}

async function collectTestFiles(directory: string, files: string[]): Promise<void> {
  const entries = await readdir(directory, {withFileTypes: true});
  for (const entry of entries) {
    if (entry.isSymbolicLink()) {
      continue;
    }
    const path = join(directory, entry.name);
    if (entry.isDirectory()) {
      await collectTestFiles(path, files);
      continue;
    }
    if (entry.isFile() && TEST_MODULE_PATTERN.test(entry.name)) {
      files.push(path);
    }
  }
}

function extractLocalModuleSpecifiers(path: string, source: string): string[] {
  const kind = path.toLocaleLowerCase("en-US").endsWith(".tsx")
    ? ts.ScriptKind.TSX
    : ts.ScriptKind.TS;
  const parsed = ts.createSourceFile(path, source, ts.ScriptTarget.Latest, true, kind);
  if (parsed.parseDiagnostics.length > 0) {
    const diagnostic = parsed.parseDiagnostics[0];
    throw new Error(
      `Cannot parse TypeScript dependency source ${path}: ${
        diagnostic ? ts.flattenDiagnosticMessageText(diagnostic.messageText, " ") : "unknown syntax error"
      }`
    );
  }
  const specifiers = new Set<string>();
  const add = (value: string): void => {
    if (value.startsWith(".")) specifiers.add(value);
  };
  const visit = (node: ts.Node): void => {
    if ((ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) &&
        node.moduleSpecifier && ts.isStringLiteralLike(node.moduleSpecifier)) {
      add(node.moduleSpecifier.text);
    } else if (ts.isImportEqualsDeclaration(node) &&
               ts.isExternalModuleReference(node.moduleReference) &&
               node.moduleReference.expression &&
               ts.isStringLiteralLike(node.moduleReference.expression)) {
      add(node.moduleReference.expression.text);
    } else if (ts.isImportTypeNode(node) &&
               ts.isLiteralTypeNode(node.argument) &&
               ts.isStringLiteralLike(node.argument.literal)) {
      add(node.argument.literal.text);
    } else if (ts.isCallExpression(node) && node.arguments.length > 0) {
      const first = node.arguments[0];
      const dynamicImport = node.expression.kind === ts.SyntaxKind.ImportKeyword;
      const commonJsRequire = ts.isIdentifier(node.expression) && node.expression.text === "require";
      if ((dynamicImport || commonJsRequire) && first && ts.isStringLiteralLike(first)) {
        add(first.text);
      }
    }
    ts.forEachChild(node, visit);
  };
  visit(parsed);
  return [...specifiers].sort();
}

async function resolveLocalTypeScriptDependency(
  root: string,
  importer: string,
  specifier: string
): Promise<string> {
  const normalizedSpecifier = specifier.replaceAll("\\", "/");
  const base = resolve(dirname(importer), normalizedSpecifier);
  assertInsideRoot(root, base, `local dependency ${specifier}`);
  const extension = extname(base).toLocaleLowerCase("en-US");
  const candidates = [".js", ".jsx", ".mjs", ".cjs"].includes(extension)
    ? [base.slice(0, -extension.length) + ".ts", base.slice(0, -extension.length) + ".tsx"]
    : extension === ".ts" || extension === ".tsx"
      ? [base]
      : extension
        ? [base]
        : [base + ".ts", base + ".tsx", join(base, "index.ts"), join(base, "index.tsx")];
  const matches: string[] = [];
  for (const candidate of candidates) {
    assertInsideRoot(root, candidate, `local dependency ${specifier}`);
    await assertNoSymlinkComponents(root, candidate);
    try {
      const metadata = await lstat(candidate);
      if (metadata.isSymbolicLink()) {
        throw new Error(`TypeScript test dependency path is symlinked: ${repositoryPath(root, candidate)}`);
      }
      if (metadata.isFile()) matches.push(resolve(candidate));
    } catch (error) {
      if (isMissingPathError(error)) continue;
      throw error;
    }
  }
  if (matches.length === 0) {
    throw new Error(
      `Unresolved local TypeScript dependency from ${repositoryPath(root, importer)}: ${specifier}`
    );
  }
  if (matches.length > 1) {
    throw new Error(
      `Ambiguous local TypeScript dependency from ${repositoryPath(root, importer)}: ${specifier}`
    );
  }
  return matches[0] as string;
}

async function requireRegularUnsymbolicPath(root: string, path: string): Promise<void> {
  await assertNoSymlinkComponents(root, path);
  const metadata = await lstat(path);
  if (!metadata.isFile() || metadata.isSymbolicLink()) {
    throw new Error(`TypeScript test dependency must be a regular file: ${repositoryPath(root, path)}`);
  }
}

async function assertNoSymlinkComponents(root: string, path: string): Promise<void> {
  assertInsideRoot(root, path, "dependency path");
  const relativePath = relative(root, path);
  let cursor = root;
  for (const component of relativePath.split(/[\\/]/).filter(Boolean)) {
    cursor = join(cursor, component);
    try {
      const metadata = await lstat(cursor);
      if (metadata.isSymbolicLink()) {
        throw new Error(`TypeScript test dependency path is symlinked: ${repositoryPath(root, cursor)}`);
      }
    } catch (error) {
      if (isMissingPathError(error)) return;
      throw error;
    }
  }
}

function assertInsideRoot(root: string, path: string, label: string): void {
  const relativePath = relative(root, resolve(path));
  if (relativePath === "" || (!relativePath.startsWith("..") && !isAbsolute(relativePath))) return;
  throw new Error(`Unsafe ${label} escapes repository: ${path}`);
}

function isWithin(parent: string, child: string): boolean {
  const relativePath = relative(parent, child);
  return relativePath === "" || (!relativePath.startsWith("..") && !isAbsolute(relativePath));
}

function repositoryPath(root: string, path: string): string {
  return relative(root, path).replaceAll("\\", "/");
}

function normalizedPathKey(path: string): string {
  const resolved = resolve(path);
  return process.platform === "win32" ? resolved.toLocaleLowerCase("en-US") : resolved;
}

function isMissingPathError(error: unknown): error is NodeJS.ErrnoException {
  return error instanceof Error && "code" in error && (error as NodeJS.ErrnoException).code === "ENOENT";
}
