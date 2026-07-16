import {createHash} from "node:crypto";
import {lstatSync, readFileSync} from "node:fs";
import {dirname, resolve} from "node:path";
import {spawnSync} from "node:child_process";
import {fileURLToPath} from "node:url";

const defaultRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const evidencePath = "fixtures/compat/release/hosted-gates.v1.json";

export function computeProductFingerprint(root = defaultRoot) {
  const inputs = new Map();
  for (const record of git(root, ["ls-files", "-s", "-z", "--cached"]).toString("utf8").split("\0").filter(Boolean)) {
    const match = /^([0-9]{6}) ([0-9a-f]{40}) ([0-3])\t(.+)$/u.exec(record);
    if (!match || match[3] !== "0" || !["100644", "100755"].includes(match[1])) {
      throw new Error(`product index contains an unsupported entry: ${record}`);
    }
    const path = match[4].replaceAll("\\", "/");
    if (!excluded(path)) inputs.set(path, `${match[1]}:${match[2]}`);
  }
  for (const rawPath of git(root, ["ls-files", "-z", "--others", "--exclude-standard"]).toString("utf8").split("\0").filter(Boolean)) {
    const path = rawPath.replaceAll("\\", "/");
    if (excluded(path)) continue;
    const absolute = resolve(root, path);
    const metadata = lstatSync(absolute);
    if (!metadata.isFile() || metadata.isSymbolicLink() || inputs.has(path)) {
      throw new Error(`product input is not one unique regular file: ${path}`);
    }
    inputs.set(path, `untracked:${createHash("sha256").update(readFileSync(absolute)).digest("hex")}`);
  }
  const paths = [...inputs.keys()].sort((left, right) => Buffer.from(left).compare(Buffer.from(right)));
  const fingerprint = createHash("sha256");
  fingerprint.update("minimax-codex-product-v2\0", "utf8");
  for (const path of paths) {
    fingerprint.update(path, "utf8");
    fingerprint.update(Buffer.from([0]));
    fingerprint.update(inputs.get(path), "ascii");
  }
  return {schemaVersion: 1, fingerprint: fingerprint.digest("hex"), fileCount: paths.length};
}

function git(root, args) {
  const result = spawnSync("git", ["-C", root, ...args], {encoding: "buffer", shell: false});
  if (result.status !== 0) {
    const detail = result.stderr?.toString("utf8").trim();
    throw new Error(`cannot enumerate product inputs${detail ? `: ${detail}` : ""}`);
  }
  return result.stdout;
}

function excluded(path) {
  return path === evidencePath || path.startsWith(".planning/");
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    process.stdout.write(`${JSON.stringify(computeProductFingerprint())}\n`);
  } catch (error) {
    process.stderr.write(`product fingerprint failed: ${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 1;
  }
}
