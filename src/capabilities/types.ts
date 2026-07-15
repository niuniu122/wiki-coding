export type CapabilitySafetyClass =
  | "catalog_read"
  | "workspace_read"
  | "local_diagnostic"
  | "workspace_write"
  | "network"
  | "process_control";

export type CapabilityAvailability =
  | "available"
  | "disabled"
  | "unavailable"
  | "invalid"
  | "shadowed"
  | "stale";

export type CapabilitySourceKind =
  | "minimax"
  | "codex_skill"
  | "codex_plugin"
  | "claw_code";

export type CapabilitySourceScope =
  | "builtin"
  | "project_native"
  | "user_native"
  | "project_compat"
  | "user_compat";

export interface CapabilitySource {
  readonly kind: CapabilitySourceKind;
  readonly scope: CapabilitySourceScope;
  readonly root: string;
  readonly file: string;
}

export type CapabilityExecution =
  | {readonly kind: "metadata_only"}
  | {readonly kind: "slash_command"; readonly command: string}
  | {readonly kind: "workspace_read"; readonly operation: "read_file" | "list_files"}
  | {
      readonly kind: "npm_script";
      readonly script: string;
      readonly argv: readonly string[];
    };

export interface CapabilityFacets {
  readonly domain: readonly string[];
  readonly action: readonly string[];
  readonly object: readonly string[];
}

export interface CapabilityDescriptor {
  readonly schemaVersion: 1;
  readonly id: string;
  readonly name: string;
  readonly description: string;
  readonly aliases: readonly string[];
  readonly commands: readonly string[];
  readonly safetyClass: CapabilitySafetyClass;
  readonly availability: CapabilityAvailability;
  readonly execution: CapabilityExecution;
  readonly idempotent: boolean;
  readonly facets: CapabilityFacets;
  readonly intentDocument: string;
  readonly source: CapabilitySource;
}

export interface CapabilityIssue {
  readonly code:
    | "outside_managed_root"
    | "invalid_manifest"
    | "invalid_path"
    | "read_failed";
  readonly sourceKind: CapabilitySourceKind;
  readonly file?: string;
}
