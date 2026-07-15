import {createHash} from "node:crypto";
import {readFile} from "node:fs/promises";
import {join} from "node:path";
import {resolveManagedPath} from "../path-policy.js";
import {parseEmbeddingResourceManifest, type EmbeddingResourceManifest} from "./embedding-resource-manifest.js";

export type EmbeddingResourceResult =
  | {readonly status: "ready"; readonly directory: string; readonly manifest: EmbeddingResourceManifest}
  | {readonly status: "unavailable"; readonly reason: "missing" | "incompatible_cpu" | "invalid_manifest" | "hash_mismatch"};

export class EmbeddingResourceLocator {
  constructor(private readonly managedRoot: string, private readonly hasAvx2: () => boolean = defaultAvx2Check) {}
  async locate(): Promise<EmbeddingResourceResult> {
    if (!this.hasAvx2()) return {status: "unavailable", reason: "incompatible_cpu"};
    let root: string;
    try { root = await resolveManagedPath(this.managedRoot, "."); } catch { return {status: "unavailable", reason: "missing"}; }
    let manifest: EmbeddingResourceManifest;
    try {
      const manifestPath = await resolveManagedPath(root, join(root, "manifest.json"));
      manifest = parseEmbeddingResourceManifest(JSON.parse(await readFile(manifestPath, "utf8")));
    } catch { return {status: "unavailable", reason: "invalid_manifest"}; }
    for (const [file, expected] of Object.entries(manifest.files)) {
      try {
        const path = await resolveManagedPath(root, join(root, file));
        const actual = createHash("sha256").update(await readFile(path)).digest("hex");
        if (actual !== expected) return {status: "unavailable", reason: "hash_mismatch"};
      } catch { return {status: "unavailable", reason: "hash_mismatch"}; }
    }
    return {status: "ready", directory: root, manifest};
  }
}

function defaultAvx2Check(): boolean {
  return process.arch === "x64";
}
