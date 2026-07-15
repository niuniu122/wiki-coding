import type {ModelProfileId, ModelContextMessage, ProviderAdapterId, ProviderProfileId} from "../types.js";
import type {ModelAdapterEvent} from "./model-adapter.js";
import type {ModelProfile} from "../providers/model-profile.js";
import type {ProviderProfile} from "../providers/provider-profile.js";
import type {ModelToolDefinition} from "../agent/model-action.js";

export interface ModelCredentialHandle {
  readonly targetId: string;
  readSecret(): Promise<string>;
}

export interface ModelRuntimeFactoryInput {
  readonly providerProfile: ProviderProfile;
  readonly modelProfile: ModelProfile;
  readonly credential: ModelCredentialHandle;
}

export interface ModelRuntimeRequest {
  readonly messages: readonly ModelContextMessage[];
  readonly maxOutputTokens: number;
  readonly tools?: readonly ModelToolDefinition[];
  readonly signal?: AbortSignal;
}

export interface ModelRuntime {
  readonly adapterId: ProviderAdapterId;
  readonly providerProfileId: ProviderProfileId;
  readonly modelProfileId: ModelProfileId;
  stream(request: ModelRuntimeRequest): AsyncGenerator<ModelAdapterEvent>;
  dispose(): Promise<void>;
}

export interface ModelRuntimeFactory {
  createRuntime(input: ModelRuntimeFactoryInput): Promise<ModelRuntime>;
}
