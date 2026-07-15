import {dirname} from "node:path";
import {fileURLToPath} from "node:url";
import {discoverTestFiles, importTestFiles} from "./test-discovery.js";

const testRoot = dirname(fileURLToPath(import.meta.url));
await importTestFiles(await discoverTestFiles(testRoot));
