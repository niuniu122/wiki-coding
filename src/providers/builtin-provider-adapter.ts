import {
  assertProviderSecurity,
  normalizePublicProviderHeaders
} from "../config/provider-security.js";
import type {ModelAdapterEvent} from "../runtime/model-adapter.js";
import type {
  ModelRuntime,
  ModelRuntimeFactoryInput,
  ModelRuntimeRequest
} from "../runtime/model-runtime.js";
import {
  MINIMAX_BUILTIN_ADAPTER_ID,
  PROVIDER_FEATURE_KEYS,
  ProviderContractError,
  parseProviderAdapterManifest,
  type ProviderAdapter,
  type ProviderFeature,
  type ProviderFeatureMatrix,
  type ProviderFeatureProfile,
  type ValidationResult
} from "./provider-adapter.js";
import type {ModelProfile} from "./model-profile.js";
import type {ProviderProfile} from "./provider-profile.js";
import {
  StrictProviderGateway,
  type ProfileProviderGateway
} from "./provider-gateway.js";
import {
  FetchHttpStreamTransport,
  type HttpStreamTransport
} from "./http-transport.js";

export const BUILTIN_PROVIDER_FEATURES: ProviderFeatureMatrix = Object.freeze({
  streaming: true,
  native_tool_calls: true,
  parallel_tool_calls: true,
  structured_output: false,
  reasoning_metadata: true,
  usage: true,
  prompt_caching: false,
  image_input: false,
  audio_input: false,
  provider_hosted_tools: false
});

export class BuiltinProviderAdapter implements ProviderAdapter {
  readonly manifest = parseProviderAdapterManifest(
    {
      schemaVersion: 1,
      adapterId: MINIMAX_BUILTIN_ADAPTER_ID,
      displayName: "Built-in Responses and Chat Completions Adapter",
      packageVersion: "1.0.0",
      apiVersion: 1,
      protocols: ["responses", "chat_completions"]
    },
    {origin: "builtin"}
  );

  private readonly gateway: ProfileProviderGateway;

  constructor(
    transport: HttpStreamTransport = new FetchHttpStreamTransport(),
    gateway?: ProfileProviderGateway
  ) {
    this.gateway = gateway ?? new StrictProviderGateway(transport);
  }

  validateProfile(profile: ProviderProfile): ValidationResult {
    if (profile.adapterId !== this.manifest.adapterId) {
      return validationFailure("identifier_kind_mismatch", "providerProfile.adapterId");
    }
    if (!this.manifest.protocols.includes(profile.transport.protocol)) {
      return validationFailure("invalid_value", "providerProfile.transport.protocol");
    }
    try {
      const publicHeaders = normalizePublicProviderHeaders(
        profile.transport.publicHeaders
      );
      assertProviderSecurity({
        name: profile.displayName,
        baseUrl: profile.transport.baseUrl,
        protocol: profile.transport.protocol,
        allowInsecureLoopback: profile.transport.allowInsecureLoopback,
        ...(publicHeaders ? {headers: publicHeaders} : {})
      });
      return {ok: true};
    } catch {
      return validationFailure("invalid_value", "providerProfile.transport");
    }
  }

  describeFeatures(_model: ModelProfile): ProviderFeatureProfile {
    return Object.freeze({
      schemaVersion: 1,
      features: BUILTIN_PROVIDER_FEATURES
    });
  }

  async createRuntime(input: ModelRuntimeFactoryInput): Promise<ModelRuntime> {
    const validation = this.validateProfile(input.providerProfile);
    if (!validation.ok) {
      const issue = validation.issues[0];
      throw new ProviderContractError(
        issue?.code ?? "invalid_value",
        issue?.path ?? "providerProfile"
      );
    }
    if (
      input.modelProfile.providerProfileId !==
      input.providerProfile.providerProfileId
    ) {
      throw new ProviderContractError(
        "invalid_value",
        "modelProfile.providerProfileId"
      );
    }
    if (!input.providerProfile.enabled || !input.modelProfile.enabled) {
      throw new ProviderContractError("invalid_value", "modelProfile.enabled");
    }
    const unsupported = findUnsupportedModelFeatures(
      input.modelProfile,
      this.describeFeatures(input.modelProfile).features
    );
    if (unsupported.length > 0) {
      throw new Error(`Model profile declares unsupported feature: ${unsupported[0]}.`);
    }
    return new BuiltinModelRuntime(input, this.gateway);
  }
}

export function findUnsupportedModelFeatures(
  model: ModelProfile,
  supported: ProviderFeatureMatrix
): readonly ProviderFeature[] {
  return PROVIDER_FEATURE_KEYS.filter(
    (feature) => model.featureProfile.features[feature] && !supported[feature]
  );
}

class BuiltinModelRuntime implements ModelRuntime {
  readonly adapterId;
  readonly providerProfileId;
  readonly modelProfileId;
  private disposed = false;

  constructor(
    private readonly input: ModelRuntimeFactoryInput,
    private readonly gateway: ProfileProviderGateway
  ) {
    this.adapterId = input.providerProfile.adapterId;
    this.providerProfileId = input.providerProfile.providerProfileId;
    this.modelProfileId = input.modelProfile.modelProfileId;
  }

  async *stream(request: ModelRuntimeRequest): AsyncGenerator<ModelAdapterEvent> {
    if (this.disposed) {
      throw new Error("Model runtime has been disposed.");
    }
    if (
      !Number.isInteger(request.maxOutputTokens) ||
      request.maxOutputTokens <= 0 ||
      request.maxOutputTokens > this.input.modelProfile.featureProfile.maxOutputTokens
    ) {
      throw new Error("Requested output token limit exceeds the model profile limit.");
    }
    const apiKey = await this.input.credential.readSecret();
    for await (const event of this.gateway.streamProfile({
      providerProfile: this.input.providerProfile,
      modelProfile: this.input.modelProfile,
      apiKey,
      messages: request.messages,
      maxOutputTokens: request.maxOutputTokens,
      ...(request.tools ? {tools: request.tools} : {}),
      ...(request.signal ? {signal: request.signal} : {})
    })) {
      if (event.type === "text.delta") {
        yield {type: "delta", delta: event.delta};
      } else if (event.type === "tool.call") {
        yield {type: "tool_call", call: event.call};
      } else {
        yield event;
      }
    }
  }

  async dispose(): Promise<void> {
    this.disposed = true;
  }
}

function validationFailure(
  code: ConstructorParameters<typeof ProviderContractError>[0],
  path: string
): ValidationResult {
  const error = new ProviderContractError(code, path);
  return {
    ok: false,
    issues: [{code, path, message: error.message}]
  };
}
