import type {
  ModelProfileId,
  ProviderAdapterId,
  ProviderProfileId
} from "../types.js";
import type {
  ModelStateLoadResult,
  ModelStateStore
} from "../config/model-state-store.js";
import type {
  ModelProfileRegistry,
  ModelProfileRegistryEntry,
  ModelProfileSource
} from "./model-profile-registry.js";
import type {ModelCredentialHandle, ModelRuntime} from "./model-runtime.js";
import type {ApiProtocol} from "../types.js";

export interface ActiveModelSelection {
  readonly adapterId: ProviderAdapterId;
  readonly providerProfileId: ProviderProfileId;
  readonly modelProfileId: ModelProfileId;
  readonly providerDisplayName: string;
  readonly modelDisplayName: string;
  readonly model: string;
  readonly protocol: ApiProtocol;
  readonly source: ModelProfileSource;
  readonly contextWindow: number;
  readonly maxOutputTokens: number;
  readonly autoCompactRatio: number;
  readonly supportsNativeToolCalls: boolean;
}

export interface ModelRuntimeSnapshot {
  readonly selection: ActiveModelSelection;
  readonly runtime: ModelRuntime;
}

export interface ModelRuntimeSnapshotPort {
  getRuntimeSnapshot(): ModelRuntimeSnapshot;
  assertAgentCompatible(): void;
}

export interface ModelCredentialBinding {
  readonly handle: ModelCredentialHandle;
  readonly hasCredential: boolean;
}

export interface ModelCredentialLocator {
  locate(entry: ModelProfileRegistryEntry): Promise<ModelCredentialBinding>;
}

export interface ModelSelectionActivity {
  isTurnActive(): boolean;
}

export type ModelSelectionErrorCode =
  | "not_initialized"
  | "model_unavailable"
  | "credential_unavailable"
  | "turn_active"
  | "sticky_selection_forbidden"
  | "agent_feature_unsupported"
  | "recovery_required";

export class ModelSelectionError extends Error {
  constructor(
    readonly code: ModelSelectionErrorCode,
    readonly configuredDefaultModelProfileId?: ModelProfileId
  ) {
    super(`Model selection failed (${code}).`);
    this.name = "ModelSelectionError";
  }
}

type ModelProfileCatalog = Pick<
  ModelProfileRegistry,
  "getModel" | "getConfiguredDefault"
>;

type ModelStatePort = Pick<ModelStateStore, "load" | "save">;

export class ModelSelectionService implements ModelRuntimeSnapshotPort {
  private snapshot: ModelRuntimeSnapshot | undefined;
  private autoCompactRatio: number | undefined;

  constructor(
    private readonly registry: ModelProfileCatalog,
    private readonly stateStore: ModelStatePort,
    private readonly credentialLocator: ModelCredentialLocator,
    private readonly activity: ModelSelectionActivity = {isTurnActive: () => false}
  ) {}

  async initialize(autoCompactRatio: number): Promise<ActiveModelSelection> {
    assertAutoCompactRatio(autoCompactRatio);
    const configuredDefault = this.registry.getConfiguredDefault();
    const state = await this.stateStore.load();
    let entry: ModelProfileRegistryEntry | undefined;

    if (state.status === "recovery_required") {
      throw recoveryError(configuredDefault);
    }
    if (state.status === "selected") {
      entry = this.registry.getModel(state.state.lastSelectedModelProfileId);
      if (!entry?.stickyEligible) {
        throw recoveryError(configuredDefault);
      }
    } else {
      entry = configuredDefault;
    }
    if (!entry) {
      throw new ModelSelectionError("model_unavailable");
    }

    const binding = await this.credentialLocator.locate(entry);
    if (state.status === "selected" && !binding.hasCredential) {
      throw recoveryError(configuredDefault);
    }
    const runtime = await entry.adapter.createRuntime({
      providerProfile: entry.providerProfile,
      modelProfile: entry.modelProfile,
      credential: binding.handle
    });
    const previous = this.snapshot;
    this.autoCompactRatio = autoCompactRatio;
    this.snapshot = createSnapshot(entry, runtime, autoCompactRatio);
    await disposeQuietly(previous?.runtime);
    return this.snapshot.selection;
  }

  async switchModel(modelProfileId: string): Promise<ActiveModelSelection> {
    if (this.activity.isTurnActive()) {
      throw new ModelSelectionError("turn_active");
    }
    const autoCompactRatio = this.autoCompactRatio;
    if (autoCompactRatio === undefined || !this.snapshot) {
      throw new ModelSelectionError("not_initialized");
    }
    const entry = this.registry.getModel(modelProfileId);
    if (!entry) {
      throw new ModelSelectionError("model_unavailable");
    }
    if (!entry.stickyEligible) {
      throw new ModelSelectionError("sticky_selection_forbidden");
    }
    const binding = await this.credentialLocator.locate(entry);
    if (!binding.hasCredential) {
      throw new ModelSelectionError("credential_unavailable");
    }
    const runtime = await entry.adapter.createRuntime({
      providerProfile: entry.providerProfile,
      modelProfile: entry.modelProfile,
      credential: binding.handle
    });
    if (this.activity.isTurnActive()) {
      await disposeQuietly(runtime);
      throw new ModelSelectionError("turn_active");
    }
    const nextSnapshot = createSnapshot(entry, runtime, autoCompactRatio);

    try {
      await this.stateStore.save(entry.modelProfile.modelProfileId);
    } catch (error) {
      await disposeQuietly(runtime);
      throw error;
    }

    const previous = this.snapshot;
    this.snapshot = nextSnapshot;
    await disposeQuietly(previous.runtime);
    return this.snapshot.selection;
  }

  getRuntimeSnapshot(): ModelRuntimeSnapshot {
    if (!this.snapshot) {
      throw new ModelSelectionError("not_initialized");
    }
    return this.snapshot;
  }

  assertAgentCompatible(): void {
    if (!this.getRuntimeSnapshot().selection.supportsNativeToolCalls) {
      throw new ModelSelectionError("agent_feature_unsupported");
    }
  }
}

export type {ModelStateLoadResult};

function createSnapshot(
  entry: ModelProfileRegistryEntry,
  runtime: ModelRuntime,
  autoCompactRatio: number
): ModelRuntimeSnapshot {
  const selection: ActiveModelSelection = Object.freeze({
    adapterId: entry.adapter.manifest.adapterId,
    providerProfileId: entry.providerProfile.providerProfileId,
    modelProfileId: entry.modelProfile.modelProfileId,
    providerDisplayName: entry.providerProfile.displayName,
    modelDisplayName: entry.modelProfile.displayName,
    model: entry.modelProfile.model,
    protocol: entry.providerProfile.transport.protocol,
    source: entry.source,
    contextWindow: entry.modelProfile.featureProfile.contextWindow,
    maxOutputTokens: entry.modelProfile.featureProfile.maxOutputTokens,
    autoCompactRatio,
    supportsNativeToolCalls:
      entry.modelProfile.featureProfile.features.native_tool_calls
  });
  return Object.freeze({selection, runtime});
}

function recoveryError(
  configuredDefault: ModelProfileRegistryEntry | undefined
): ModelSelectionError {
  return new ModelSelectionError(
    "recovery_required",
    configuredDefault?.modelProfile.modelProfileId
  );
}

function assertAutoCompactRatio(value: number): void {
  if (!Number.isFinite(value) || value <= 0 || value >= 1) {
    throw new Error("Model selection auto-compact ratio must be between zero and one.");
  }
}

async function disposeQuietly(runtime: ModelRuntime | undefined): Promise<void> {
  if (!runtime) {
    return;
  }
  try {
    await runtime.dispose();
  } catch {
    // The new selection is already durable; old-runtime cleanup cannot roll it back.
  }
}
