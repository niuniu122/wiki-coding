import {readdir, readFile} from "node:fs/promises";
import {basename, join} from "node:path";
import {parseCapabilityManifest} from "../capability-manifest.js";
import {resolveManagedPath} from "../path-policy.js";
import type {CapabilitySourceAdapter, CapabilitySourceResult} from "../source-adapter.js";
import type {CapabilityDescriptor, CapabilityIssue, CapabilitySourceScope} from "../types.js";

export class MiniMaxCapabilitySource implements CapabilitySourceAdapter {
  readonly kind = "minimax" as const;
  constructor(private readonly root: string, private readonly scope: CapabilitySourceScope) {}

  async scan(): Promise<CapabilitySourceResult> {
    const descriptors: CapabilityDescriptor[] = [];
    const issues: CapabilityIssue[] = [];
    let root: string;
    try {
      root = await resolveManagedPath(this.root, ".");
    } catch {
      return Object.freeze({descriptors: Object.freeze([]), issues: Object.freeze([])});
    }
    for (const entry of await readdir(root, {withFileTypes: true})) {
      if (!entry.isFile() || !entry.name.endsWith(".json")) continue;
      const file = join(root, entry.name);
      try {
        const managed = await resolveManagedPath(root, file);
        const value = JSON.parse(await readFile(managed, "utf8")) as unknown;
        descriptors.push(parseCapabilityManifest(value, {
          kind: this.kind,
          scope: this.scope,
          root,
          file: managed
        }));
      } catch {
        issues.push({code: "invalid_manifest", sourceKind: this.kind, file: basename(file)});
      }
    }
    return Object.freeze({descriptors: Object.freeze(descriptors), issues: Object.freeze(issues)});
  }
}
