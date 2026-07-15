import {readFile} from "node:fs/promises";
import {join} from "node:path";
import type {
  ApiProtocol,
  ApiProviderId,
  AppConfig,
  ContextConfig,
  ModelProviderConfig
} from "../types.js";
import {readJsonFile, writeJsonFile, writeTextFileAtomic} from "../utils/jsonl.js";
import {
  assertProviderSecurity,
  normalizeProviderEndpoint,
  normalizePublicProviderHeaders
} from "./provider-security.js";
import {parseAgentFeatureFlagConfig} from "./feature-flags.js";

export const BUILT_IN_MODEL_PROVIDERS: Record<ApiProviderId, ModelProviderConfig> = {
  "minimax-official": {
    name: "MiniMax Official",
    baseUrl: "https://api.minimax.io/v1",
    protocol: "responses",
    envKey: "MINIMAX_API_KEY",
    defaultModel: "MiniMax-M3"
  },
  hashsight: {
    name: "Hashsight OpenAI Compatible",
    baseUrl: "https://www.hashsight.cn/v1",
    protocol: "chat_completions",
    envKey: "HASHSIGHT_API_KEY",
    defaultModel: "MiniMax-M3"
  }
};

export const DEFAULT_CONFIG: AppConfig = {
  schemaVersion: 1,
  modelProvider: "minimax-official",
  modelProviders: cloneProviders(BUILT_IN_MODEL_PROVIDERS),
  model: "MiniMax-M3",
  context: {
    workingContextLimit: 128000,
    autoCompactRatio: 0.9,
    maxCompletionTokens: 8192
  }
};

interface ParsedConfigFile {
  config: AppConfig;
  requiresRewrite: boolean;
}

interface LegacyApiConfig {
  provider?: "minimax" | "hashsight" | "openai-compatible";
  protocol?: ApiProtocol;
  baseUrl?: string;
}

export class ConfigManager {
  constructor(private readonly rootDir: string) {}

  async load(): Promise<AppConfig> {
    return this.loadConfig(true);
  }

  async loadReadOnly(): Promise<AppConfig> {
    return this.loadConfig(false);
  }

  private async loadConfig(rewriteLegacy: boolean): Promise<AppConfig> {
    await rejectExplicitSqlite(this.configPath());
    const fallback: ParsedConfigFile = {
      config: cloneConfig(DEFAULT_CONFIG),
      requiresRewrite: false
    };
    const parsed = await readJsonFile(this.configPath(), fallback, {
      parse: parseConfigFile
    });
    if (rewriteLegacy && parsed.requiresRewrite) {
      await this.preserveLegacyConfig();
      await writeJsonFile(this.configPath(), parsed.config);
    }
    return cloneConfig(parsed.config);
  }

  async save(config: AppConfig): Promise<void> {
    const parsed = parseConfigFile(config);
    await writeJsonFile(this.configPath(), parsed.config);
  }

  private configPath(): string {
    return join(this.rootDir, "config.json");
  }

  private async preserveLegacyConfig(): Promise<void> {
    const raw = await readFile(this.configPath(), "utf8");
    // Validate the exact bytes being retained, including after primary-file recovery.
    parseConfigFile(JSON.parse(raw) as unknown);
    const backupPath = `${this.configPath()}.v0.bak`;
    try {
      const existing = await readFile(backupPath, "utf8");
      if (existing !== raw) {
        throw new Error(
          `Legacy configuration backup at ${backupPath} does not match config.json.`
        );
      }
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== "ENOENT") {
        throw error;
      }
      await writeTextFileAtomic(backupPath, raw, 0o600);
    }
  }
}

async function rejectExplicitSqlite(filePath: string): Promise<void> {
  let raw: string;
  try {
    raw = await readFile(filePath, "utf8");
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") {
      return;
    }
    throw error;
  }

  let parsed: unknown;
  try {
    parsed = JSON.parse(raw) as unknown;
  } catch {
    return;
  }
  if (
    isRecord(parsed) &&
    isRecord(parsed.storage) &&
    parsed.storage.driver === "sqlite"
  ) {
    throw new Error(
      "SQLite is not supported during configuration migration; use JSONL storage."
    );
  }
}

function parseConfigFile(value: unknown): ParsedConfigFile {
  if (!isRecord(value)) {
    throw new Error("Invalid configuration: root must be a JSON object.");
  }

  const issues: string[] = [];
  if (value.schemaVersion !== undefined && value.schemaVersion !== 1) {
    issues.push("schemaVersion must be 1");
  }
  validateOptionalNonEmptyString(value, "modelProvider", issues);
  validateOptionalNonEmptyString(value, "model", issues);
  validateProviders(value.modelProviders, issues);
  validateLegacyApi(value.api, issues);
  validateLegacyStorage(value.storage, issues);
  validateContext(value.context, issues);
  let features;
  try { features = parseAgentFeatureFlagConfig(value.features); }
  catch (error) { issues.push(errorMessage(error).replace(/^Invalid configuration:\s*/u, "")); }

  if (issues.length > 0) {
    throw new Error(`Invalid configuration: ${issues.join("; ")}`);
  }

  const providers = mergeProviders(value.modelProviders);
  const legacyApi = isRecord(value.api) ? (value.api as LegacyApiConfig) : undefined;
  const modelProvider = resolveProviderId(value.modelProvider, legacyApi, providers);
  migrateLegacyApi(legacyApi, modelProvider, providers);

  for (const [providerId, provider] of Object.entries(providers)) {
    try {
      const baseUrl = normalizeProviderEndpoint(provider.baseUrl);
      const canonicalProvider = {...provider, baseUrl};
      assertProviderSecurity(canonicalProvider);
      const headers = normalizePublicProviderHeaders(provider.headers);
      providers[providerId] = {
        ...canonicalProvider,
        ...(headers ? {headers} : {})
      };
    } catch (error) {
      throw new Error(
        `Invalid configuration: modelProviders.${providerId}: ${errorMessage(error)}`
      );
    }
  }

  if (!providers[modelProvider]) {
    throw new Error(
      `Invalid configuration: modelProvider references unknown provider: ${modelProvider}`
    );
  }

  const context = mergeContext(value.context);
  validateContextBudget(context);
  const selected = providers[modelProvider];
  if (!selected) {
    throw new Error(`No model provider configuration is available for ${modelProvider}.`);
  }

  const config: AppConfig = {
    schemaVersion: 1,
    modelProvider,
    modelProviders: providers,
    model:
      typeof value.model === "string"
        ? value.model.trim()
        : selected.defaultModel ?? DEFAULT_CONFIG.model,
    context,
    ...(features ? {features} : {})
  };

  return {
    config,
    requiresRewrite: needsCanonicalRewrite(value)
  };
}

function mergeProviders(value: unknown): Record<ApiProviderId, ModelProviderConfig> {
  const custom = isRecord(value)
    ? (value as Record<ApiProviderId, ModelProviderConfig>)
    : {};
  return cloneProviders({...BUILT_IN_MODEL_PROVIDERS, ...custom});
}

function resolveProviderId(
  configured: unknown,
  legacyApi: LegacyApiConfig | undefined,
  providers: Record<ApiProviderId, ModelProviderConfig>
): ApiProviderId {
  if (typeof configured === "string") {
    if (!providers[configured]) {
      throw new Error(
        `Invalid configuration: modelProvider references unknown provider: ${configured}`
      );
    }
    return configured;
  }

  const legacyId = legacyProviderId(legacyApi?.provider);
  if (legacyId === "openai-compatible" && !providers[legacyId]) {
    if (!legacyApi?.baseUrl) {
      throw new Error(
        "Invalid configuration: api.baseUrl is required for openai-compatible migration"
      );
    }
    providers[legacyId] = {
      name: "OpenAI Compatible",
      baseUrl: legacyApi.baseUrl,
      protocol: legacyApi.protocol ?? "chat_completions"
    };
  }
  return legacyId ?? DEFAULT_CONFIG.modelProvider;
}

function migrateLegacyApi(
  legacyApi: LegacyApiConfig | undefined,
  modelProvider: ApiProviderId,
  providers: Record<ApiProviderId, ModelProviderConfig>
): void {
  if (!legacyApi) {
    return;
  }
  const provider = providers[modelProvider];
  if (!provider) {
    return;
  }
  providers[modelProvider] = {
    ...provider,
    ...(legacyApi.baseUrl ? {baseUrl: legacyApi.baseUrl} : {}),
    ...(legacyApi.protocol ? {protocol: legacyApi.protocol} : {})
  };
}

function legacyProviderId(
  provider: LegacyApiConfig["provider"]
): ApiProviderId | undefined {
  if (provider === "minimax") {
    return "minimax-official";
  }
  return provider;
}

function mergeContext(value: unknown): ContextConfig {
  if (!isRecord(value)) {
    return {...DEFAULT_CONFIG.context};
  }
  return {
    workingContextLimit:
      typeof value.workingContextLimit === "number"
        ? value.workingContextLimit
        : DEFAULT_CONFIG.context.workingContextLimit,
    autoCompactRatio:
      typeof value.autoCompactRatio === "number"
        ? value.autoCompactRatio
        : DEFAULT_CONFIG.context.autoCompactRatio,
    maxCompletionTokens:
      typeof value.maxCompletionTokens === "number"
        ? value.maxCompletionTokens
        : DEFAULT_CONFIG.context.maxCompletionTokens
  };
}

function validateContextBudget(context: ContextConfig): void {
  if (context.maxCompletionTokens >= context.workingContextLimit) {
    throw new Error(
      "Invalid configuration: context.maxCompletionTokens must be smaller than context.workingContextLimit"
    );
  }
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
    if (
      provider.allowInsecureLoopback !== undefined &&
      typeof provider.allowInsecureLoopback !== "boolean"
    ) {
      issues.push(`${path}.allowInsecureLoopback must be a boolean`);
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

function validateLegacyStorage(value: unknown, issues: string[]): void {
  if (value === undefined) {
    return;
  }
  if (!isRecord(value)) {
    issues.push("storage must be an object");
    return;
  }
  if (value.driver === "sqlite") {
    throw new Error(
      "SQLite is not supported during configuration migration; use JSONL storage."
    );
  }
  if (value.driver !== undefined && value.driver !== "jsonl") {
    issues.push("storage.driver must be jsonl");
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

function needsCanonicalRewrite(value: Record<string, unknown>): boolean {
  const canonicalKeys = new Set([
    "schemaVersion",
    "modelProvider",
    "modelProviders",
    "model",
    "context",
    "features"
  ]);
  return (
    value.schemaVersion !== 1 ||
    Object.keys(value).some((key) => !canonicalKeys.has(key))
  );
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

function cloneConfig(config: AppConfig): AppConfig {
  return {
    ...config,
    modelProviders: cloneProviders(config.modelProviders),
    context: {...config.context},
    ...(config.features ? {features: {...config.features}} : {})
  };
}

function cloneProviders(
  providers: Record<ApiProviderId, ModelProviderConfig>
): Record<ApiProviderId, ModelProviderConfig> {
  return Object.fromEntries(
    Object.entries(providers).map(([id, provider]) => [
      id,
      {
        ...provider,
        ...(provider.headers ? {headers: {...provider.headers}} : {})
      }
    ])
  );
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export function resolveActiveProvider(config: AppConfig): ModelProviderConfig & {id: string} {
  const provider = config.modelProviders[config.modelProvider];
  if (!provider) {
    throw new Error(`Unknown model provider: ${config.modelProvider}`);
  }
  return {...provider, id: config.modelProvider};
}
