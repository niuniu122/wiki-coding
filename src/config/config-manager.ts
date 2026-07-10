import {join} from "node:path";
import type {ApiProviderId, AppConfig, ModelProviderConfig} from "../types.js";
import {readJsonFile, writeJsonFile} from "../utils/jsonl.js";

export const BUILT_IN_MODEL_PROVIDERS: Record<ApiProviderId, ModelProviderConfig> = {
  "minimax-official": {
    name: "MiniMax Official",
    baseUrl: "https://api.minimax.io/v1",
    protocol: "responses",
    envKey: "MINIMAX_API_KEY",
    defaultModel: "MiniMax-M3",
    supportsThinkTags: false
  },
  hashsight: {
    name: "Hashsight OpenAI Compatible",
    baseUrl: "https://www.hashsight.cn/v1",
    protocol: "chat_completions",
    envKey: "HASHSIGHT_API_KEY",
    defaultModel: "MiniMax-M3",
    supportsThinkTags: true
  }
};

export const DEFAULT_CONFIG: AppConfig = {
  modelProvider: "minimax-official",
  modelProviders: BUILT_IN_MODEL_PROVIDERS,
  api: {
    provider: "minimax",
    protocol: "responses",
    baseUrl: "https://api.minimax.io/v1"
  },
  model: "MiniMax-M3",
  storage: {
    driver: "jsonl"
  },
  context: {
    workingContextLimit: 128000,
    autoCompactRatio: 0.9,
    maxCompletionTokens: 8192
  }
};

export class ConfigManager {
  constructor(private readonly rootDir: string) {}

  async load(): Promise<AppConfig> {
    const loaded = await readJsonFile<Partial<AppConfig>>(this.configPath(), {}, {
      parse: parseConfigFile
    });
    const modelProviders = {
      ...DEFAULT_CONFIG.modelProviders,
      ...loaded.modelProviders
    };
    const modelProvider = resolveLegacyProviderId(loaded, modelProviders);
    const activeProvider =
      modelProviders[modelProvider] ??
      DEFAULT_CONFIG.modelProviders["minimax-official"] ??
      BUILT_IN_MODEL_PROVIDERS["minimax-official"];
    if (!activeProvider) {
      throw new Error("No model provider configuration is available.");
    }
    return {
      ...DEFAULT_CONFIG,
      ...loaded,
      modelProvider,
      modelProviders,
      api: {
        ...DEFAULT_CONFIG.api,
        ...loaded.api,
        provider: toLegacyProviderName(modelProvider),
        protocol: activeProvider.protocol,
        baseUrl: activeProvider.baseUrl
      },
      storage: {...DEFAULT_CONFIG.storage, ...loaded.storage},
      context: {...DEFAULT_CONFIG.context, ...loaded.context}
    };
  }

  async save(config: AppConfig): Promise<void> {
    parseConfigFile(config);
    await writeJsonFile(this.configPath(), config);
  }

  private configPath(): string {
    return join(this.rootDir, "config.json");
  }
}

function parseConfigFile(value: unknown): Partial<AppConfig> {
  if (!isRecord(value)) {
    throw new Error("Invalid configuration: root must be a JSON object.");
  }

  const issues: string[] = [];
  validateOptionalNonEmptyString(value, "modelProvider", issues);
  validateOptionalNonEmptyString(value, "model", issues);
  validateProviders(value.modelProviders, issues);
  validateLegacyApi(value.api, issues);
  validateStorage(value.storage, issues);
  validateContext(value.context, issues);

  const configuredProvider = value.modelProvider;
  if (typeof configuredProvider === "string") {
    const customProviders = isRecord(value.modelProviders)
      ? Object.keys(value.modelProviders)
      : [];
    const available = new Set([...Object.keys(BUILT_IN_MODEL_PROVIDERS), ...customProviders]);
    if (!available.has(configuredProvider)) {
      issues.push(`modelProvider references unknown provider: ${configuredProvider}`);
    }
  }

  const context = isRecord(value.context) ? value.context : {};
  const workingContextLimit =
    typeof context.workingContextLimit === "number"
      ? context.workingContextLimit
      : DEFAULT_CONFIG.context.workingContextLimit;
  const maxCompletionTokens =
    typeof context.maxCompletionTokens === "number"
      ? context.maxCompletionTokens
      : DEFAULT_CONFIG.context.maxCompletionTokens;
  if (
    Number.isFinite(workingContextLimit) &&
    Number.isFinite(maxCompletionTokens) &&
    maxCompletionTokens >= workingContextLimit
  ) {
    issues.push("context.maxCompletionTokens must be smaller than context.workingContextLimit");
  }

  if (issues.length > 0) {
    throw new Error(`Invalid configuration: ${issues.join("; ")}`);
  }
  return value as Partial<AppConfig>;
}

function validateProviders(value: unknown, issues: string[]): void {
  if (value === undefined) {
    return;
  }
  if (!isRecord(value)) {
    issues.push("modelProviders must be an object");
    return;
  }

  for (const [id, provider] of Object.entries(value)) {
    const path = `modelProviders.${id}`;
    if (!isRecord(provider)) {
      issues.push(`${path} must be an object`);
      continue;
    }
    requireNonEmptyString(provider.name, `${path}.name`, issues);
    requireHttpUrl(provider.baseUrl, `${path}.baseUrl`, issues);
    if (provider.protocol !== "responses" && provider.protocol !== "chat_completions") {
      issues.push(`${path}.protocol must be responses or chat_completions`);
    }
    optionalString(provider.envKey, `${path}.envKey`, issues);
    optionalString(provider.defaultModel, `${path}.defaultModel`, issues);
    if (provider.supportsThinkTags !== undefined && typeof provider.supportsThinkTags !== "boolean") {
      issues.push(`${path}.supportsThinkTags must be boolean`);
    }
    if (provider.headers !== undefined) {
      if (!isRecord(provider.headers)) {
        issues.push(`${path}.headers must be an object`);
      } else if (Object.values(provider.headers).some((header) => typeof header !== "string")) {
        issues.push(`${path}.headers values must be strings`);
      }
    }
  }
}

function validateLegacyApi(value: unknown, issues: string[]): void {
  if (value === undefined) {
    return;
  }
  if (!isRecord(value)) {
    issues.push("api must be an object");
    return;
  }
  if (
    value.provider !== undefined &&
    value.provider !== "minimax" &&
    value.provider !== "hashsight" &&
    value.provider !== "openai-compatible"
  ) {
    issues.push("api.provider is invalid");
  }
  if (
    value.protocol !== undefined &&
    value.protocol !== "responses" &&
    value.protocol !== "chat_completions"
  ) {
    issues.push("api.protocol is invalid");
  }
  if (value.baseUrl !== undefined) {
    requireHttpUrl(value.baseUrl, "api.baseUrl", issues);
  }
}

function validateStorage(value: unknown, issues: string[]): void {
  if (value === undefined) {
    return;
  }
  if (!isRecord(value)) {
    issues.push("storage must be an object");
    return;
  }
  if (value.driver !== undefined && value.driver !== "jsonl" && value.driver !== "sqlite") {
    issues.push("storage.driver must be jsonl or sqlite");
  }
}

function validateContext(value: unknown, issues: string[]): void {
  if (value === undefined) {
    return;
  }
  if (!isRecord(value)) {
    issues.push("context must be an object");
    return;
  }
  positiveInteger(value.workingContextLimit, "context.workingContextLimit", issues);
  positiveInteger(value.maxCompletionTokens, "context.maxCompletionTokens", issues);
  if (
    value.autoCompactRatio !== undefined &&
    (typeof value.autoCompactRatio !== "number" ||
      !Number.isFinite(value.autoCompactRatio) ||
      value.autoCompactRatio <= 0 ||
      value.autoCompactRatio >= 1)
  ) {
    issues.push("context.autoCompactRatio must be a number between 0 and 1");
  }
}

function validateOptionalNonEmptyString(
  value: Record<string, unknown>,
  key: string,
  issues: string[]
): void {
  if (value[key] !== undefined) {
    requireNonEmptyString(value[key], key, issues);
  }
}

function positiveInteger(value: unknown, path: string, issues: string[]): void {
  if (
    value !== undefined &&
    (typeof value !== "number" || !Number.isInteger(value) || value <= 0)
  ) {
    issues.push(`${path} must be a positive integer`);
  }
}

function optionalString(value: unknown, path: string, issues: string[]): void {
  if (value !== undefined && typeof value !== "string") {
    issues.push(`${path} must be a string`);
  }
}

function requireNonEmptyString(value: unknown, path: string, issues: string[]): void {
  if (typeof value !== "string" || value.trim().length === 0) {
    issues.push(`${path} must be a non-empty string`);
  }
}

function requireHttpUrl(value: unknown, path: string, issues: string[]): void {
  if (typeof value !== "string") {
    issues.push(`${path} must be an absolute HTTP URL`);
    return;
  }
  try {
    const url = new URL(value);
    if (url.protocol !== "http:" && url.protocol !== "https:") {
      issues.push(`${path} must use http or https`);
    }
  } catch {
    issues.push(`${path} must be an absolute HTTP URL`);
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function resolveActiveProvider(config: AppConfig): ModelProviderConfig & {id: string} {
  const provider = config.modelProviders[config.modelProvider];
  if (!provider) {
    throw new Error(`Unknown model provider: ${config.modelProvider}`);
  }
  return {...provider, id: config.modelProvider};
}

function resolveLegacyProviderId(
  loaded: Partial<AppConfig>,
  providers: Record<ApiProviderId, ModelProviderConfig>
): ApiProviderId {
  if (loaded.modelProvider && providers[loaded.modelProvider]) {
    return loaded.modelProvider;
  }

  if (loaded.api?.provider === "hashsight" && providers.hashsight) {
    return "hashsight";
  }

  if (loaded.api?.provider === "minimax" && providers["minimax-official"]) {
    return "minimax-official";
  }

  return DEFAULT_CONFIG.modelProvider;
}

function toLegacyProviderName(providerId: string): "minimax" | "hashsight" | "openai-compatible" {
  if (providerId === "hashsight") {
    return "hashsight";
  }
  if (providerId === "minimax-official") {
    return "minimax";
  }
  return "openai-compatible";
}
