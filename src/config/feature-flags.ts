export interface AgentFeatureFlagConfig {
  readonly capabilityCatalog: boolean;
  readonly capabilityEmbedding: boolean;
  readonly agentExecution: boolean;
  readonly agentDefaultRoute: boolean;
}

export interface ResolvedAgentFeatureFlags extends AgentFeatureFlagConfig {
  readonly diagnostics: readonly FeatureFlagDiagnostic[];
}

export type FeatureFlagDiagnostic =
  | "catalog_disabled"
  | "embedding_disabled"
  | "embedding_requires_catalog"
  | "agent_disabled"
  | "agent_requires_catalog"
  | "default_route_disabled"
  | "default_route_requires_agent"
  | "default_route_gate_failed"
  | "catalog_runtime_failed";

export const DEFAULT_AGENT_FEATURE_FLAGS: AgentFeatureFlagConfig = Object.freeze({
  capabilityCatalog: false,
  capabilityEmbedding: false,
  agentExecution: false,
  agentDefaultRoute: false
});

const KEYS = Object.freeze(Object.keys(DEFAULT_AGENT_FEATURE_FLAGS) as (keyof AgentFeatureFlagConfig)[]);

export function parseAgentFeatureFlagConfig(value: unknown): AgentFeatureFlagConfig | undefined {
  if (value === undefined) return undefined;
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error("Invalid configuration: features must be an object");
  const record = value as Record<string, unknown>;
  const unknown = Object.keys(record).find((key) => !KEYS.includes(key as keyof AgentFeatureFlagConfig));
  if (unknown) throw new Error(`Invalid configuration: features.${unknown} is unknown`);
  for (const key of KEYS) {
    if (record[key] !== undefined && typeof record[key] !== "boolean") throw new Error(`Invalid configuration: features.${key} must be a boolean`);
  }
  return Object.freeze({...DEFAULT_AGENT_FEATURE_FLAGS, ...record}) as AgentFeatureFlagConfig;
}

export function resolveAgentFeatureFlags(config: AgentFeatureFlagConfig | undefined, options: {releaseGatePassed: boolean}): ResolvedAgentFeatureFlags {
  const requested = config ?? DEFAULT_AGENT_FEATURE_FLAGS;
  const capabilityCatalog = requested.capabilityCatalog;
  const capabilityEmbedding = capabilityCatalog && requested.capabilityEmbedding;
  const agentExecution = capabilityCatalog && requested.agentExecution;
  const agentDefaultRoute = agentExecution && requested.agentDefaultRoute && options.releaseGatePassed;
  const diagnostics: FeatureFlagDiagnostic[] = [];
  if (!capabilityCatalog) diagnostics.push("catalog_disabled");
  else if (!capabilityEmbedding) diagnostics.push("embedding_disabled");
  if (requested.capabilityEmbedding && !capabilityCatalog) diagnostics.push("embedding_requires_catalog");
  if (!requested.agentExecution) diagnostics.push("agent_disabled");
  if (requested.agentExecution && !capabilityCatalog) diagnostics.push("agent_requires_catalog");
  if (!requested.agentDefaultRoute) diagnostics.push("default_route_disabled");
  if (requested.agentDefaultRoute && !agentExecution) diagnostics.push("default_route_requires_agent");
  if (requested.agentDefaultRoute && agentExecution && !options.releaseGatePassed) diagnostics.push("default_route_gate_failed");
  return Object.freeze({capabilityCatalog, capabilityEmbedding, agentExecution, agentDefaultRoute, diagnostics: Object.freeze([...new Set(diagnostics)])});
}

export class RuntimeFeatureFlagService {
  private flags = resolveAgentFeatureFlags(undefined, {releaseGatePassed: false});
  constructor(private readonly releaseGatePassed: boolean) {}
  get current(): ResolvedAgentFeatureFlags { return this.flags; }
  initialize(config: AgentFeatureFlagConfig | undefined): ResolvedAgentFeatureFlags {
    this.flags = resolveAgentFeatureFlags(config, {releaseGatePassed: this.releaseGatePassed});
    return this.flags;
  }
  disableCapabilityRuntime(): ResolvedAgentFeatureFlags {
    const disabled = resolveAgentFeatureFlags(undefined, {releaseGatePassed: false});
    this.flags = Object.freeze({
      ...disabled,
      diagnostics: Object.freeze([...new Set([...disabled.diagnostics, "catalog_runtime_failed" as const])])
    });
    return this.flags;
  }
}
