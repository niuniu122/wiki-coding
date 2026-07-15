import {readdir, readFile} from "node:fs/promises";
import {basename, extname, join} from "node:path";
import {descriptorFromMarkdown} from "../capability-manifest.js";
import {resolveManagedPath} from "../path-policy.js";
import type {CapabilitySourceAdapter, CapabilitySourceResult} from "../source-adapter.js";
import type {CapabilityDescriptor, CapabilityIssue, CapabilitySourceScope} from "../types.js";

export class ClawCodeCapabilitySource implements CapabilitySourceAdapter {
  readonly kind = "claw_code" as const;
  constructor(private readonly root: string, private readonly scope: CapabilitySourceScope) {}

  async scan(): Promise<CapabilitySourceResult> {
    const descriptors: CapabilityDescriptor[] = [];
    const issues: CapabilityIssue[] = [];
    let root: string;
    try { root = await resolveManagedPath(this.root, "."); } catch {
      return {descriptors: Object.freeze([]), issues: Object.freeze([])};
    }
    for (const entry of await readdir(root, {withFileTypes: true})) {
      if (!entry.isFile() || extname(entry.name).toLowerCase() !== ".md") continue;
      const file = join(root, entry.name);
      try {
        const managed = await resolveManagedPath(root, file);
        const markdown = await readFile(managed, "utf8");
        const name = basename(entry.name, extname(entry.name));
        const command = `/${name}`;
        descriptors.push(descriptorFromMarkdown(markdown, {
          id: `capability:claw-code/${safeSegment(name)}`,
          name,
          description: `Installed Claw Code command ${command}`,
          command
        }, {kind: this.kind, scope: this.scope, root, file: managed}));
      } catch {
        issues.push({code: "invalid_manifest", sourceKind: this.kind, file: entry.name});
      }
    }
    return {descriptors: Object.freeze(descriptors), issues: Object.freeze(issues)};
  }
}

function safeSegment(value: string): string {
  return value.normalize("NFC").replace(/[^A-Za-z0-9._@-]+/g, "-").replace(/^-+|-+$/g, "") || "command";
}
