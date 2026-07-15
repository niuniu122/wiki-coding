import {createHash} from "node:crypto";
import {
  BUILT_IN_MODEL_PROVIDERS,
  type ConfigManager
} from "../config/config-manager.js";
import type {
  UserProfileIssue,
  UserProfileSnapshot,
  UserProfileStore
} from "../config/user-profile-store.js";
import type {
  AppConfig,
  ModelProfileId,
  ModelProviderConfig,
  ProviderProfileId
} from "../types.js";
import {
  BUILTIN_PROVIDER_FEATURES,
  findUnsupportedModelFeatures
} from "../providers/builtin-provider-adapter.js";
import type {ProviderAdapter} from "../providers/provider-adapter.js";
import {parseModelProfileId, parseProviderProfileId} from "../providers/provider-adapter.js";
import type {ProviderAdapterRegistry} from "../providers/provider-adapter-registry.js";
import {parseModelProfile, type ModelProfile} from "../providers/model-profile.js";
import {
  parseProviderProfile,
  type ProviderProfile
} from "../providers/provider-profile.js";

export type ModelProfileSource = "builtin" | "legacy_workspace" | "user";

export type ModelProfileRegistryIssueCode =
  | "invalid_profile"
  | "duplicate_profile_id"
  | "missing_adapter"
  | "missing_provider"
  | "missing_conformance_fixture"
  | "unsupported_feature"
  | "disabled_profile"
  | "store_recovery_required";

export interface ModelProfileRegistryIssue {
  readonly kind: "provider" | "model" | "store";
  readonly code: ModelProfileRegistryIssueCode;
  readonly profileId?: string;
}

export interface ProviderProfileRegistryEntry {
  readonly providerProfile: ProviderProfile;
  readonly adapter: ProviderAdapter;
  readonly source: ModelProfileSource;
}

export interface ModelProfileRegistryEntry {
  readonly modelProfile: ModelProfile;
  readonly providerProfile: ProviderProfile;
  readonly adapter: ProviderAdapter;
  readonly source: ModelProfileSource;
  readonly stickyEligible: boolean;
}

interface ProviderCandidate {
  readonly profile: ProviderProfile;
  readonly source: ModelProfileSource;
}

interface ModelCandidate {
  readonly profile: ModelProfile;
  readonly source: ModelProfileSource;
  readonly configuredDefault: boolean;
}

interface CompatibilityProjection {
  readonly providers: readonly ProviderCandidate[];
  readonly models: readonly ModelCandidate[];
}

const BUILTIN_PROFILE_IDS: Readonly<
  Record<string, {providerProfileId: string; modelProfilePrefix: string}>
> = Object.freeze({
  "minimax-official": {
    providerProfileId: "provider:minimax/official",
    modelProfilePrefix: "model:minimax/official"
  },
  hashsight: {
    providerProfileId: "provider:minimax/hashsight",
    modelProfilePrefix: "model:minimax/hashsight"
  }
});

const EMPTY_USER_SNAPSHOT: UserProfileSnapshot = Object.freeze({
  providerProfiles: Object.freeze([]),
  modelProfiles: Object.freeze([]),
  issues: Object.freeze([])
});

export class ModelProfileRegistry {
  private providerEntries = new Map<ProviderProfileId, ProviderProfileRegistryEntry>();
  private modelEntries = new Map<ModelProfileId, ModelProfileRegistryEntry>();
  private configuredDefault: ModelProfileRegistryEntry | undefined;
  private registryIssues: readonly ModelProfileRegistryIssue[] = Object.freeze([]);

  constructor(
    private readonly adapters: ProviderAdapterRegistry,
    private readonly userProfiles?: UserProfileStore
  ) {}

  async initialize(config: AppConfig): Promise<void> {
    const projection = projectLegacyConfigProfiles(config);
    const userSnapshot = this.userProfiles
      ? await this.userProfiles.load()
      : EMPTY_USER_SNAPSHOT;
    const providerEntries = new Map<
      ProviderProfileId,
      ProviderProfileRegistryEntry
    >();
    const modelEntries = new Map<ModelProfileId, ModelProfileRegistryEntry>();
    const issues: ModelProfileRegistryIssue[] = userSnapshot.issues.map(mapStoreIssue);

    const providerCandidates: ProviderCandidate[] = [
      ...projection.providers,
      ...userSnapshot.providerProfiles.map((profile) => ({
        profile,
        source: "user" as const
      }))
    ];
    for (const candidate of providerCandidates) {
      const id = candidate.profile.providerProfileId;
      if (providerEntries.has(id)) {
        issues.push({kind: "provider", code: "duplicate_profile_id", profileId: id});
        continue;
      }
      const adapter = this.adapters.get(candidate.profile.adapterId);
      if (!adapter) {
        issues.push({kind: "provider", code: "missing_adapter", profileId: id});
        continue;
      }
      if (
        !this.adapters.hasConformanceFixture(
          candidate.profile.adapterId,
          candidate.profile.transport.protocol
        )
      ) {
        issues.push({
          kind: "provider",
          code: "missing_conformance_fixture",
          profileId: id
        });
        continue;
      }
      if (!adapter.validateProfile(candidate.profile).ok) {
        issues.push({kind: "provider", code: "invalid_profile", profileId: id});
        continue;
      }
      providerEntries.set(
        id,
        Object.freeze({
          providerProfile: candidate.profile,
          adapter,
          source: candidate.source
        })
      );
    }

    const modelCandidates: ModelCandidate[] = [
      ...projection.models,
      ...userSnapshot.modelProfiles.map((profile) => ({
        profile,
        source: "user" as const,
        configuredDefault: false
      }))
    ];
    let configuredDefaultId: ModelProfileId | undefined;
    for (const candidate of modelCandidates) {
      const id = candidate.profile.modelProfileId;
      if (candidate.configuredDefault) {
        configuredDefaultId = id;
      }
      if (modelEntries.has(id)) {
        issues.push({kind: "model", code: "duplicate_profile_id", profileId: id});
        continue;
      }
      const provider = providerEntries.get(candidate.profile.providerProfileId);
      if (!provider) {
        issues.push({kind: "model", code: "missing_provider", profileId: id});
        continue;
      }
      if (!candidate.profile.enabled || !provider.providerProfile.enabled) {
        issues.push({kind: "model", code: "disabled_profile", profileId: id});
        continue;
      }
      const unsupported = findUnsupportedModelFeatures(
        candidate.profile,
        provider.adapter.describeFeatures(candidate.profile).features
      );
      if (unsupported.length > 0) {
        issues.push({kind: "model", code: "unsupported_feature", profileId: id});
        continue;
      }
      modelEntries.set(
        id,
        Object.freeze({
          modelProfile: candidate.profile,
          providerProfile: provider.providerProfile,
          adapter: provider.adapter,
          source: candidate.source,
          stickyEligible: candidate.source !== "legacy_workspace"
        })
      );
    }

    this.providerEntries = providerEntries;
    this.modelEntries = modelEntries;
    this.configuredDefault = configuredDefaultId
      ? modelEntries.get(configuredDefaultId)
      : undefined;
    this.registryIssues = Object.freeze(issues);
  }

  async initializeReadOnly(configManager: ConfigManager): Promise<void> {
    await this.initialize(await configManager.loadReadOnly());
  }

  listProviders(): readonly ProviderProfileRegistryEntry[] {
    return Object.freeze(
      [...this.providerEntries.values()].sort((left, right) =>
        left.providerProfile.providerProfileId.localeCompare(
          right.providerProfile.providerProfileId
        )
      )
    );
  }

  listModels(): readonly ModelProfileRegistryEntry[] {
    return Object.freeze(
      [...this.modelEntries.values()].sort((left, right) =>
        left.modelProfile.modelProfileId.localeCompare(
          right.modelProfile.modelProfileId
        )
      )
    );
  }

  getModel(modelProfileId: ModelProfileId | string): ModelProfileRegistryEntry | undefined {
    try {
      return this.modelEntries.get(parseModelProfileId(modelProfileId));
    } catch {
      return undefined;
    }
  }

  getConfiguredDefault(): ModelProfileRegistryEntry | undefined {
    return this.configuredDefault;
  }

  get issues(): readonly ModelProfileRegistryIssue[] {
    return this.registryIssues;
  }
}

export function projectLegacyConfigProfiles(config: AppConfig): CompatibilityProjection {
  const providers: ProviderCandidate[] = [];
  const models: ModelCandidate[] = [];

  for (const [providerId, provider] of Object.entries(BUILT_IN_MODEL_PROVIDERS)) {
    const configured = config.modelProviders[providerId];
    if (configured && isCanonicalBuiltinProvider(providerId, configured)) {
      continue;
    }
    const providerProfile = createProjectedProviderProfile(
      providerId,
      provider,
      "builtin"
    );
    providers.push({profile: providerProfile, source: "builtin"});
    if (provider.defaultModel) {
      models.push({
        profile: createProjectedModelProfile(
          config,
          providerId,
          provider,
          providerProfile.providerProfileId,
          provider.defaultModel,
          "builtin"
        ),
        source: "builtin",
        configuredDefault: false
      });
    }
  }

  for (const [providerId, provider] of Object.entries(config.modelProviders)) {
    const canonicalBuiltin = isCanonicalBuiltinProvider(providerId, provider);
    const providerSource: ModelProfileSource = canonicalBuiltin
      ? "builtin"
      : "legacy_workspace";
    const providerProfile = createProjectedProviderProfile(
      providerId,
      provider,
      providerSource
    );
    providers.push({profile: providerProfile, source: providerSource});

    const model =
      providerId === config.modelProvider ? config.model : provider.defaultModel;
    if (!model) {
      continue;
    }
    const builtinDefault = BUILT_IN_MODEL_PROVIDERS[providerId]?.defaultModel;
    const modelSource: ModelProfileSource =
      canonicalBuiltin && model === builtinDefault ? "builtin" : "legacy_workspace";
    const modelProfile = createProjectedModelProfile(
      config,
      providerId,
      provider,
      providerProfile.providerProfileId,
      model,
      modelSource
    );
    models.push({
      profile: modelProfile,
      source: modelSource,
      configuredDefault: providerId === config.modelProvider
    });
  }

  return Object.freeze({
    providers: Object.freeze(providers),
    models: Object.freeze(models)
  });
}

function createProjectedProviderProfile(
  providerId: string,
  provider: ModelProviderConfig,
  source: ModelProfileSource
): ProviderProfile {
  const providerProfileId =
    source === "builtin"
      ? builtinProviderProfileId(providerId)
      : legacyProviderProfileId(providerId);
  const authentication =
    provider.envKey && /^[A-Z][A-Z0-9_]*$/.test(provider.envKey)
      ? {kind: "bearer" as const, envBinding: provider.envKey}
      : {kind: "bearer" as const};
  return parseProviderProfile({
    schemaVersion: 1,
    providerProfileId,
    adapterId: "adapter:minimax/builtin",
    displayName: provider.name,
    enabled: true,
    transport: {
      baseUrl: provider.baseUrl,
      protocol: provider.protocol,
      publicHeaders: provider.headers ?? {},
      allowInsecureLoopback: provider.allowInsecureLoopback ?? false
    },
    authentication
  });
}

function createProjectedModelProfile(
  config: AppConfig,
  providerId: string,
  provider: ModelProviderConfig,
  providerProfileId: ProviderProfileId,
  model: string,
  source: ModelProfileSource
): ModelProfile {
  const modelProfileId =
    source === "builtin"
      ? builtinModelProfileId(providerId, model)
      : legacyModelProfileId(providerId, model);
  return parseModelProfile({
    schemaVersion: 1,
    modelProfileId,
    providerProfileId,
    displayName: `${provider.name} / ${model}`,
    model,
    enabled: true,
    featureProfile: {
      schemaVersion: 1,
      features: BUILTIN_PROVIDER_FEATURES,
      contextWindow: config.context.workingContextLimit,
      maxOutputTokens: config.context.maxCompletionTokens
    }
  });
}

export function legacyProviderProfileId(providerId: string): ProviderProfileId {
  return parseProviderProfileId(`provider:legacy/${safeIdSegment(providerId)}`);
}

export function legacyModelProfileId(
  providerId: string,
  model: string
): ModelProfileId {
  return parseModelProfileId(
    `model:legacy/${safeIdSegment(providerId)}/${safeIdSegment(model)}`
  );
}

export function projectedProviderProfileId(
  providerId: string,
  provider: ModelProviderConfig
): ProviderProfileId {
  return isCanonicalBuiltinProvider(providerId, provider)
    ? builtinProviderProfileId(providerId)
    : legacyProviderProfileId(providerId);
}

function builtinProviderProfileId(providerId: string): ProviderProfileId {
  const definition = BUILTIN_PROFILE_IDS[providerId];
  if (!definition) {
    throw new Error("Builtin provider profile identity is missing.");
  }
  return parseProviderProfileId(definition.providerProfileId);
}

function builtinModelProfileId(providerId: string, model: string): ModelProfileId {
  const definition = BUILTIN_PROFILE_IDS[providerId];
  if (!definition) {
    throw new Error("Builtin model profile identity is missing.");
  }
  return parseModelProfileId(`${definition.modelProfilePrefix}/${safeIdSegment(model)}`);
}

function isCanonicalBuiltinProvider(
  providerId: string,
  provider: ModelProviderConfig
): boolean {
  const builtin = BUILT_IN_MODEL_PROVIDERS[providerId];
  if (!builtin || !BUILTIN_PROFILE_IDS[providerId]) {
    return false;
  }
  return (
    provider.name === builtin.name &&
    provider.baseUrl === builtin.baseUrl &&
    provider.protocol === builtin.protocol &&
    provider.envKey === builtin.envKey &&
    provider.defaultModel === builtin.defaultModel &&
    (provider.allowInsecureLoopback ?? false) ===
      (builtin.allowInsecureLoopback ?? false) &&
    recordsEqual(provider.headers, builtin.headers)
  );
}

function recordsEqual(
  left: Readonly<Record<string, string>> | undefined,
  right: Readonly<Record<string, string>> | undefined
): boolean {
  const leftEntries = Object.entries(left ?? {}).sort(([a], [b]) => a.localeCompare(b));
  const rightEntries = Object.entries(right ?? {}).sort(([a], [b]) => a.localeCompare(b));
  return JSON.stringify(leftEntries) === JSON.stringify(rightEntries);
}

function safeIdSegment(value: string): string {
  if (/^[A-Za-z0-9][A-Za-z0-9._@-]*$/.test(value)) {
    return value;
  }
  const normalized = value
    .trim()
    .replace(/[^A-Za-z0-9._@-]+/g, "-")
    .replace(/^[^A-Za-z0-9]+/, "")
    .replace(/-+$/, "");
  const digest = createHash("sha256").update(value).digest("hex").slice(0, 8);
  return `${normalized || "profile"}-${digest}`;
}

function mapStoreIssue(issue: UserProfileIssue): ModelProfileRegistryIssue {
  return {
    kind: issue.kind,
    code: issue.code,
    ...(issue.profileId ? {profileId: issue.profileId} : {})
  };
}
