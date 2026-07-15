import {ConfigManager} from "../config/config-manager.js";
import {
  CredentialStore,
  type CredentialBackend,
  type LegacyCredentialMigrationResult,
  type CredentialStatus,
  type PlaintextConsent,
  isKeyringUnavailableError
} from "../config/credential-store.js";
import {
  formatProviderSummary,
  getActiveProvider,
  listProviders
} from "../config/provider-config.js";
import {createCredentialTarget} from "../config/provider-security.js";
import type {CredentialTarget} from "../config/provider-security.js";
import type {AppConfig} from "../types.js";
import type {ModelStateStore} from "../config/model-state-store.js";
import type {ModelCatalogView} from "../protocol.js";
import type {ModelCredentialHandle} from "./model-runtime.js";
import {
  ModelSelectionService,
  ModelSelectionError,
  type ActiveModelSelection,
  type ModelRuntimeSnapshot,
  type ModelSelectionActivity
} from "./model-selection-service.js";
import {
  projectedProviderProfileId,
  type ModelProfileRegistry,
  type ModelProfileRegistryEntry
} from "./model-profile-registry.js";

export interface ProviderModelSelectionOptions {
  readonly stateStore: ModelStateStore;
  readonly activity?: ModelSelectionActivity;
}

export class ProviderService {
  private currentConfig: AppConfig | undefined;
  private legacyMigration: LegacyCredentialMigrationResult = {status: "none"};
  private readonly modelSelectionService: ModelSelectionService | undefined;

  constructor(
    private readonly configManager: ConfigManager,
    private readonly credentialStore: CredentialStore,
    private readonly legacyCredentialMigrator?: LegacyCredentialMigrator,
    private readonly modelProfileRegistry?: ModelProfileRegistry,
    modelSelectionOptions?: ProviderModelSelectionOptions
  ) {
    this.modelSelectionService =
      modelProfileRegistry && modelSelectionOptions
        ? new ModelSelectionService(
            modelProfileRegistry,
            modelSelectionOptions.stateStore,
            {locate: (entry) => this.locateModelCredential(entry)},
            modelSelectionOptions.activity
          )
        : undefined;
  }

  async init(): Promise<void> {
    this.currentConfig = await this.configManager.load();
    this.legacyMigration =
      (await this.legacyCredentialMigrator?.migrate(this.credentialTarget())) ??
      {status: "none"};
    await this.modelProfileRegistry?.initialize(this.currentConfig);
    await this.modelSelectionService?.initialize(
      this.currentConfig.context.autoCompactRatio
    );
  }

  async inspectCredential(): Promise<CredentialStatus> {
    return this.credentialStore.inspect(this.credentialTarget());
  }

  async getApiKey(): Promise<string | null> {
    return this.credentialStore.get(this.credentialTarget());
  }

  async saveApiKey(
    value: string,
    consent?: PlaintextConsent
  ): Promise<CredentialBackend> {
    const target = this.credentialTarget();
    try {
      await this.credentialStore.saveToKeyring(target, value);
      await this.refreshLegacyMigration(target);
      return "os-keyring";
    } catch (error) {
      if (!isKeyringUnavailableError(error)) {
        throw error;
      }
      if (!consent) {
        return "unavailable";
      }
      await this.credentialStore.saveToUserFile(target, value, consent);
      await this.refreshLegacyMigration(target);
      return "user-file";
    }
  }

  list(): string[] {
    return listProviders(this.config).map((provider) => {
      const state = provider.id === this.config.modelProvider ? "active" : "available";
      return `${formatProviderSummary(this.config, provider)} | ${state}`;
    });
  }

  async switch(providerId: string): Promise<string> {
    const provider = this.config.modelProviders[providerId];
    if (!provider) {
      throw new Error(`Unknown model provider: ${providerId}`);
    }
    const nextConfig: AppConfig = {
      ...this.config,
      modelProvider: providerId,
      model: provider.defaultModel ?? this.config.model
    };
    await this.configManager.save(nextConfig);
    this.currentConfig = nextConfig;
    return formatProviderSummary(nextConfig, getActiveProvider(nextConfig));
  }

  get config(): AppConfig {
    if (!this.currentConfig) {
      throw new Error("ProviderService has not been initialized.");
    }
    return this.currentConfig;
  }

  getLegacyCredentialNotice(): Extract<
    LegacyCredentialMigrationResult,
    {status: "reentry_required"}
  > | undefined {
    return this.legacyMigration.status === "reentry_required"
      ? this.legacyMigration
      : undefined;
  }

  get profiles(): ModelProfileRegistry | undefined {
    return this.modelProfileRegistry;
  }

  get modelSelection(): ModelSelectionService | undefined {
    return this.modelSelectionService;
  }

  getRuntimeSnapshot(): ModelRuntimeSnapshot {
    if (!this.modelSelectionService) {
      throw new Error("Model selection is not configured.");
    }
    return this.modelSelectionService.getRuntimeSnapshot();
  }

  assertAgentCompatible(): void {
    if (!this.modelSelectionService) {
      throw new Error("Model selection is not configured.");
    }
    this.modelSelectionService.assertAgentCompatible();
  }

  async switchModel(modelProfileId: string): Promise<ActiveModelSelection> {
    if (!this.modelSelectionService) {
      throw new Error("Model selection is not configured.");
    }
    return this.modelSelectionService.switchModel(modelProfileId);
  }

  getActiveModelSelection(): ActiveModelSelection {
    return this.getRuntimeSnapshot().selection;
  }

  async listModels(): Promise<readonly ModelCatalogView[]> {
    if (!this.modelProfileRegistry || !this.modelSelectionService) {
      throw new Error("Model selection is not configured.");
    }
    const activeId = this.getActiveModelSelection().modelProfileId;
    return Promise.all(
      this.modelProfileRegistry.listModels().map(async (entry) => {
        const isActive = entry.modelProfile.modelProfileId === activeId;
        let availability: ModelCatalogView["availability"] = isActive
          ? "active"
          : "available";
        let reason: ModelCatalogView["reason"];
        if (!entry.stickyEligible && !isActive) {
          availability = "unavailable";
          reason = "workspace_profile_requires_promotion";
        } else if (!isActive) {
          const credential = await this.locateModelCredential(entry);
          if (!credential.hasCredential) {
            availability = "unavailable";
            reason = "credential_unavailable";
          }
        }
        return Object.freeze({
          modelProfileId: entry.modelProfile.modelProfileId,
          providerProfileId: entry.providerProfile.providerProfileId,
          modelDisplayName: entry.modelProfile.displayName,
          providerDisplayName: entry.providerProfile.displayName,
          model: entry.modelProfile.model,
          source: entry.source,
          availability,
          ...(reason ? {reason} : {})
        });
      })
    );
  }

  async switchProvider(providerId: string): Promise<ActiveModelSelection> {
    const provider = this.config.modelProviders[providerId];
    if (!provider || !this.modelProfileRegistry) {
      throw new ModelSelectionError("model_unavailable");
    }
    const providerProfileId = projectedProviderProfileId(providerId, provider);
    const preferredModel =
      provider.defaultModel ??
      (providerId === this.config.modelProvider ? this.config.model : undefined);
    const candidates = this.modelProfileRegistry
      .listModels()
      .filter(
        (entry) =>
          entry.providerProfile.providerProfileId === providerProfileId &&
          entry.stickyEligible
      );
    const selected =
      candidates.find((entry) => entry.modelProfile.model === preferredModel) ??
      candidates[0];
    if (!selected) {
      throw new ModelSelectionError("model_unavailable");
    }
    return this.switchModel(selected.modelProfile.modelProfileId);
  }

  private credentialTarget() {
    const provider = getActiveProvider(this.config);
    return createCredentialTarget(provider.id, provider);
  }

  private async refreshLegacyMigration(target: CredentialTarget): Promise<void> {
    this.legacyMigration =
      (await this.legacyCredentialMigrator?.migrate(target)) ?? {status: "none"};
  }

  private async locateModelCredential(
    entry: ModelProfileRegistryEntry
  ): Promise<{handle: ModelCredentialHandle; hasCredential: boolean}> {
    const target = this.modelCredentialTarget(entry);
    const current = await this.credentialStore.peek(target);
    const handle: ModelCredentialHandle = Object.freeze({
      targetId: target.fingerprint,
      readSecret: async () => {
        const value = await this.credentialStore.peek(target);
        if (!value) {
          throw new Error("The selected model credential is unavailable.");
        }
        return value;
      }
    });
    return Object.freeze({handle, hasCredential: Boolean(current)});
  }

  private modelCredentialTarget(entry: ModelProfileRegistryEntry) {
    const providerProfile = entry.providerProfile;
    const matchingProvider = Object.entries(this.config.modelProviders).find(
      ([providerId, provider]) =>
        projectedProviderProfileId(providerId, provider) ===
        providerProfile.providerProfileId
    );
    const providerId = matchingProvider?.[0] ?? providerProfile.providerProfileId;
    return createCredentialTarget(providerId, {
      name: providerProfile.displayName,
      baseUrl: providerProfile.transport.baseUrl,
      protocol: providerProfile.transport.protocol,
      headers: {...providerProfile.transport.publicHeaders},
      allowInsecureLoopback: providerProfile.transport.allowInsecureLoopback,
      ...(providerProfile.authentication.envBinding
        ? {envKey: providerProfile.authentication.envBinding}
        : {}),
      defaultModel: entry.modelProfile.model
    });
  }
}

export interface LegacyCredentialMigrator {
  migrate(target: CredentialTarget): Promise<LegacyCredentialMigrationResult>;
}
