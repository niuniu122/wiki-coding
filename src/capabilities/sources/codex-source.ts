import {readdir, readFile} from "node:fs/promises";
import {basename, join} from "node:path";
import {descriptorFromMarkdown, parseCapabilityManifest} from "../capability-manifest.js";
import {resolveManagedPath} from "../path-policy.js";
import type {CapabilitySourceAdapter, CapabilitySourceResult} from "../source-adapter.js";
import type {CapabilityDescriptor, CapabilityIssue, CapabilitySourceScope} from "../types.js";

export class CodexCapabilitySource implements CapabilitySourceAdapter {
  readonly kind = "codex_skill" as const;
  constructor(private readonly root: string, private readonly scope: CapabilitySourceScope) {}

  async scan(): Promise<CapabilitySourceResult> {
    const descriptors: CapabilityDescriptor[] = [];
    const issues: CapabilityIssue[] = [];
    let root: string;
    try { root = await resolveManagedPath(this.root, "."); } catch {
      return {descriptors: Object.freeze([]), issues: Object.freeze([])};
    }
    for (const entry of await readdir(root, {withFileTypes: true})) {
      if (!entry.isDirectory()) continue;
      const file = join(root, entry.name, "SKILL.md");
      try {
        const managed = await resolveManagedPath(root, file);
        const markdown = await readFile(managed, "utf8");
        const segment = safeSegment(entry.name);
        descriptors.push(descriptorFromMarkdown(markdown, {
          id: `capability:codex/${segment}`,
          name: entry.name,
          description: `Installed Codex skill ${entry.name}`
        }, {kind: this.kind, scope: this.scope, root, file: managed}));
      } catch {
        issues.push({code: "invalid_manifest", sourceKind: this.kind, file: basename(file)});
      }
    }
    return {descriptors: Object.freeze(descriptors), issues: Object.freeze(issues)};
  }
}

export class CodexPluginCapabilitySource implements CapabilitySourceAdapter {
  readonly kind = "codex_plugin" as const;
  constructor(private readonly root: string, private readonly scope: CapabilitySourceScope) {}

  async scan(): Promise<CapabilitySourceResult> {
    const descriptors: CapabilityDescriptor[] = [];
    const issues: CapabilityIssue[] = [];
    let root: string;
    try { root = await resolveManagedPath(this.root, "."); } catch {
      return {descriptors: Object.freeze([]), issues: Object.freeze([])};
    }
    for (const entry of await readdir(root, {withFileTypes: true})) {
      if (!entry.isDirectory()) continue;
      const file = join(root, entry.name, ".codex-plugin", "plugin.json");
      try {
        const managed = await resolveManagedPath(root, file);
        const value = JSON.parse(await readFile(managed, "utf8")) as unknown;
        if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error("invalid plugin metadata");
        const metadata = value as Record<string, unknown>;
        const name = typeof metadata.name === "string" && metadata.name.trim() ? metadata.name.trim() : entry.name;
        const description = typeof metadata.description === "string" && metadata.description.trim() ? metadata.description.trim() : `Installed Codex plugin ${name}`;
        descriptors.push(parseCapabilityManifest({
          schemaVersion: 1,
          id: `capability:codex-plugin/${safeSegment(entry.name)}`,
          name,
          description,
          aliases: [],
          commands: [],
          safetyClass: "catalog_read",
          idempotent: false,
          execution: {kind: "metadata_only"},
          facets: {domain: ["plugin"], action: ["discover"], object: ["capability"]}
        }, {kind: this.kind, scope: this.scope, root, file: managed}));
      } catch {
        issues.push({code: "invalid_manifest", sourceKind: this.kind, file: basename(file)});
      }
    }
    return {descriptors: Object.freeze(descriptors), issues: Object.freeze(issues)};
  }
}

function safeSegment(value: string): string {
  return value.normalize("NFC").replace(/[^A-Za-z0-9._@-]+/g, "-").replace(/^-+|-+$/g, "") || "skill";
}
