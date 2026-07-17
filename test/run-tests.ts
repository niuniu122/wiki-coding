import {dirname} from "node:path";
import {fileURLToPath} from "node:url";
import {
  discoverTestFiles,
  importTestFiles,
  validateDiscoveredTestGraph
} from "./test-discovery.js";

const testRoot = dirname(fileURLToPath(import.meta.url));
const testFiles = await discoverTestFiles(testRoot);
await validateDiscoveredTestGraph(dirname(testRoot), testFiles);
await importTestFiles(testFiles);
