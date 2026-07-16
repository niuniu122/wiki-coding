import {createHash} from "node:crypto";
import {lstatSync, readFileSync} from "node:fs";
import {dirname, resolve} from "node:path";
import {spawnSync} from "node:child_process";
import {fileURLToPath} from "node:url";

const defaultRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const evidencePath = "fixtures/compat/release/hosted-gates.v1.json";

export function computeProductFingerprint(root = defaultRoot) {
  const listed = spawnSync(
    "git",
    ["-C", root, "ls-files", "-z", "--cached", "--others", "--exclude-standard"],
    {encoding: "buffer", shell: false}
  );
  if (listed.status !== 0) {
    const detail = listed.stderr?.toString("utf8").trim();
    throw new Error(`cannot enumerate product inputs${detail ? `: ${detail}` : ""}`);
  }
  const paths = listed.stdout.toString("utf8")
    .split("\0")
    .filter(Boolean)
    .map((path) => path.replaceAll("\\", "/"))
    .filter((path) => path !== evidencePath && !path.startsWith(".planning/"))
    .sort((left, right) => Buffer.from(left).compare(Buffer.from(right)));
  const fingerprint = createHash("sha256");
  fingerprint.update("minimax-codex-product-v1\0", "utf8");
  for (const path of paths) {
    const absolute = resolve(root, path);
    const metadata = lstatSync(absolute);
    if (!metadata.isFile() || metadata.isSymbolicLink()) {
      throw new Error(`product input is not a regular file: ${path}`);
    }
    fingerprint.update(path, "utf8");
    fingerprint.update(Buffer.from([0]));
    fingerprint.update(createHash("sha256").update(readFileSync(absolute)).digest());
  }
  return {schemaVersion: 1, fingerprint: fingerprint.digest("hex"), fileCount: paths.length};
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    process.stdout.write(`${JSON.stringify(computeProductFingerprint())}\n`);
  } catch (error) {
    process.stderr.write(`product fingerprint failed: ${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 1;
  }
}
