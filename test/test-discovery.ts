import {readdir} from "node:fs/promises";
import {join, resolve} from "node:path";
import {pathToFileURL, type URL} from "node:url";

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
