import type {
  CapabilityAvailability,
  CapabilityDescriptor,
  CapabilityExecution,
  CapabilityFacets,
  CapabilitySafetyClass,
  CapabilitySource
} from "./types.js";

const SAFETY_CLASSES = new Set<CapabilitySafetyClass>([
  "catalog_read",
  "workspace_read",
  "local_diagnostic",
  "workspace_write",
  "network",
  "process_control"
]);
const ALLOWED_FIELDS = new Set([
  "schemaVersion",
  "id",
  "name",
  "description",
  "aliases",
  "commands",
  "safetyClass",
  "enabled",
  "execution",
  "idempotent",
  "facets"
]);

export class CapabilityManifestError extends Error {
  readonly name = "CapabilityManifestError";
  constructor(readonly code: "invalid_manifest", readonly field: string) {
    super(`Invalid capability manifest (${field}).`);
  }
}

export function parseCapabilityManifest(
  value: unknown,
  source: CapabilitySource
): CapabilityDescriptor {
  const record = asRecord(value, "manifest");
  for (const key of Object.keys(record)) {
    if (!ALLOWED_FIELDS.has(key)) {
      throw new CapabilityManifestError("invalid_manifest", key);
    }
  }
  if (record.schemaVersion !== 1) {
    throw new CapabilityManifestError("invalid_manifest", "schemaVersion");
  }
  const id = requiredString(record.id, "id");
  if (!/^capability:[a-z0-9][a-z0-9._-]*(?:\/[A-Za-z0-9][A-Za-z0-9._@-]*)+$/.test(id)) {
    throw new CapabilityManifestError("invalid_manifest", "id");
  }
  const name = requiredString(record.name, "name");
  const description = requiredString(record.description, "description");
  const aliases = stringArray(record.aliases, "aliases");
  const commands = stringArray(record.commands, "commands");
  if (!SAFETY_CLASSES.has(record.safetyClass as CapabilitySafetyClass)) {
    throw new CapabilityManifestError("invalid_manifest", "safetyClass");
  }
  const execution = parseExecution(record.execution);
  if (record.idempotent !== undefined && typeof record.idempotent !== "boolean") {
    throw new CapabilityManifestError("invalid_manifest", "idempotent");
  }
  const facets = parseFacets(record.facets);
  const availability: CapabilityAvailability =
    record.enabled === false ? "disabled" : "available";
  return Object.freeze({
    schemaVersion: 1,
    id,
    name,
    description,
    aliases: Object.freeze(aliases),
    commands: Object.freeze(commands),
    safetyClass: record.safetyClass as CapabilitySafetyClass,
    availability,
    execution,
    idempotent: record.idempotent === true,
    facets,
    intentDocument: [
      name,
      description,
      ...aliases,
      ...commands,
      ...facets.domain,
      ...facets.action,
      ...facets.object
    ].join("\n"),
    source: Object.freeze({...source})
  });
}

export function descriptorFromMarkdown(
  markdown: string,
  fallback: {id: string; name: string; description: string; command?: string},
  source: CapabilitySource
): CapabilityDescriptor {
  const frontmatter = readFrontmatter(markdown);
  return parseCapabilityManifest(
    {
      schemaVersion: 1,
      id: frontmatter.id ?? fallback.id,
      name: frontmatter.name ?? fallback.name,
      description: frontmatter.description ?? fallback.description,
      aliases: splitList(frontmatter.aliases),
      commands: fallback.command ? [fallback.command] : [],
      safetyClass: frontmatter.safetyClass ?? "catalog_read",
      enabled: frontmatter.enabled !== "false",
      execution: fallback.command
        ? {kind: "slash_command", command: fallback.command}
        : {kind: "metadata_only"},
      idempotent: false,
      facets: {domain: [], action: [], object: []}
    },
    source
  );
}

function parseExecution(value: unknown): CapabilityExecution {
  const record = asRecord(value, "execution");
  if (record.kind === "metadata_only") {
    return Object.freeze({kind: "metadata_only"});
  }
  if (record.kind === "slash_command") {
    const command = requiredString(record.command, "execution.command");
    if (!/^\/[A-Za-z0-9][A-Za-z0-9:_-]*$/.test(command)) {
      throw new CapabilityManifestError("invalid_manifest", "execution.command");
    }
    return Object.freeze({kind: "slash_command", command});
  }
  if (record.kind === "workspace_read") {
    if (record.operation !== "read_file" && record.operation !== "list_files") {
      throw new CapabilityManifestError("invalid_manifest", "execution.operation");
    }
    return Object.freeze({kind: "workspace_read", operation: record.operation});
  }
  if (record.kind === "npm_script") {
    const script = requiredString(record.script, "execution.script");
    if (!/^[A-Za-z0-9][A-Za-z0-9:_-]{0,127}$/.test(script)) {
      throw new CapabilityManifestError("invalid_manifest", "execution.script");
    }
    const argv = stringArray(record.argv, "execution.argv");
    if (argv.length > 32 || argv.some((item) => item.length > 512 || item.includes("\0"))) {
      throw new CapabilityManifestError("invalid_manifest", "execution.argv");
    }
    return Object.freeze({
      kind: "npm_script",
      script,
      argv: Object.freeze(argv)
    });
  }
  throw new CapabilityManifestError("invalid_manifest", "execution.kind");
}

function parseFacets(value: unknown): CapabilityFacets {
  const record = value === undefined ? {} : asRecord(value, "facets");
  return Object.freeze({
    domain: Object.freeze(stringArray(record.domain, "facets.domain")),
    action: Object.freeze(stringArray(record.action, "facets.action")),
    object: Object.freeze(stringArray(record.object, "facets.object"))
  });
}

function readFrontmatter(markdown: string): Record<string, string> {
  if (!markdown.startsWith("---")) return {};
  const end = markdown.indexOf("\n---", 3);
  if (end < 0) return {};
  const result: Record<string, string> = {};
  for (const line of markdown.slice(3, end).split(/\r?\n/)) {
    const separator = line.indexOf(":");
    if (separator <= 0) continue;
    const key = line.slice(0, separator).trim();
    const raw = line.slice(separator + 1).trim();
    result[key] = raw.replace(/^['"]|['"]$/g, "");
  }
  return result;
}

function splitList(value: string | undefined): string[] {
  if (!value) return [];
  const inner = value.replace(/^\[/, "").replace(/\]$/, "");
  return inner.split(",").map((item) => item.trim().replace(/^['"]|['"]$/g, "")).filter(Boolean);
}

function asRecord(value: unknown, field: string): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new CapabilityManifestError("invalid_manifest", field);
  }
  return value as Record<string, unknown>;
}

function requiredString(value: unknown, field: string): string {
  if (typeof value !== "string" || !value.trim()) {
    throw new CapabilityManifestError("invalid_manifest", field);
  }
  return value.trim();
}

function stringArray(value: unknown, field: string): string[] {
  if (value === undefined) return [];
  if (!Array.isArray(value) || value.some((item) => typeof item !== "string" || !item.trim())) {
    throw new CapabilityManifestError("invalid_manifest", field);
  }
  return [...new Set(value.map((item) => (item as string).trim()))];
}
