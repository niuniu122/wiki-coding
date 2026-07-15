import type {ApiProtocol, ProviderAdapterId, ProviderProfileId, ModelProfileId} from "../types.js";
import type {ModelProfile} from "./model-profile.js";
import type {ProviderProfile} from "./provider-profile.js";
import type {
  ModelRuntime,
  ModelRuntimeFactory,
  ModelRuntimeFactoryInput
} from "../runtime/model-runtime.js";

export const PROVIDER_ADAPTER_MANIFEST_SCHEMA_VERSION = 1 as const;
export const PROVIDER_ADAPTER_API_VERSION = 1 as const;
export const PROVIDER_FEATURE_PROFILE_SCHEMA_VERSION = 1 as const;

export const PROVIDER_FEATURE_KEYS = [
  "streaming",
  "native_tool_calls",
  "parallel_tool_calls",
  "structured_output",
  "reasoning_metadata",
  "usage",
  "prompt_caching",
  "image_input",
  "audio_input",
  "provider_hosted_tools"
] as const;

export type ProviderFeature = (typeof PROVIDER_FEATURE_KEYS)[number];
export type ProviderFeatureMatrix = Readonly<Record<ProviderFeature, boolean>>;
export type ProviderAdapterOrigin = "builtin" | "managed";

export const MINIMAX_BUILTIN_ADAPTER_ID =
  "adapter:minimax/builtin" as ProviderAdapterId;

export interface ProviderAdapterManifest {
  readonly schemaVersion: 1;
  readonly adapterId: ProviderAdapterId;
  readonly displayName: string;
  readonly packageVersion: string;
  readonly apiVersion: 1;
  readonly protocols: readonly ApiProtocol[];
}

export interface ProviderFeatureProfile {
  readonly schemaVersion: 1;
  readonly features: ProviderFeatureMatrix;
}

export type ProviderContractErrorCode =
  | "invalid_type"
  | "missing_field"
  | "unknown_field"
  | "unsupported_schema_version"
  | "unsupported_api_version"
  | "invalid_identifier"
  | "identifier_kind_mismatch"
  | "invalid_value"
  | "unknown_feature"
  | "protected_identifier";

export interface ProviderContractIssue {
  readonly code: ProviderContractErrorCode;
  readonly path: string;
  readonly message: string;
}

export type ValidationResult =
  | {readonly ok: true}
  | {readonly ok: false; readonly issues: readonly ProviderContractIssue[]};

export class ProviderContractError extends Error {
  constructor(
    readonly code: ProviderContractErrorCode,
    readonly path: string
  ) {
    super(`Provider contract validation failed at ${path} (${code}).`);
    this.name = "ProviderContractError";
  }
}

export interface ProviderAdapter extends ModelRuntimeFactory {
  readonly manifest: ProviderAdapterManifest;
  validateProfile(profile: ProviderProfile): ValidationResult;
  describeFeatures(model: ModelProfile): ProviderFeatureProfile;
  createRuntime(input: ModelRuntimeFactoryInput): Promise<ModelRuntime>;
}

const QUALIFIED_ID_PATTERN =
  /^(adapter|provider|model):[A-Za-z0-9][A-Za-z0-9._-]*(?:\/[A-Za-z0-9][A-Za-z0-9._@-]*)+$/;
const ID_KINDS = new Set(["adapter", "provider", "model"]);
const PROTECTED_ADAPTER_IDS = new Set<string>([MINIMAX_BUILTIN_ADAPTER_ID]);
const API_PROTOCOLS = new Set<ApiProtocol>(["responses", "chat_completions"]);

export function parseProviderAdapterId(
  value: unknown,
  path = "adapterId"
): ProviderAdapterId {
  return parseQualifiedId(value, "adapter", path) as ProviderAdapterId;
}

export function parseProviderProfileId(
  value: unknown,
  path = "providerProfileId"
): ProviderProfileId {
  return parseQualifiedId(value, "provider", path) as ProviderProfileId;
}

export function parseModelProfileId(
  value: unknown,
  path = "modelProfileId"
): ModelProfileId {
  return parseQualifiedId(value, "model", path) as ModelProfileId;
}

export function parseProviderAdapterManifest(
  value: unknown,
  options: {readonly origin: ProviderAdapterOrigin} = {origin: "managed"}
): ProviderAdapterManifest {
  const path = "manifest";
  const record = requireContractRecord(value, path);
  assertContractKeys(record, [
    "schemaVersion",
    "adapterId",
    "displayName",
    "packageVersion",
    "apiVersion",
    "protocols"
  ], path);

  const schemaVersion = parseContractVersion(
    requireContractField(record, "schemaVersion", path),
    `${path}.schemaVersion`,
    PROVIDER_ADAPTER_MANIFEST_SCHEMA_VERSION,
    "unsupported_schema_version"
  );
  const adapterId = parseProviderAdapterId(
    requireContractField(record, "adapterId", path),
    `${path}.adapterId`
  );
  if (options.origin !== "builtin" && options.origin !== "managed") {
    throw new ProviderContractError("invalid_value", `${path}.origin`);
  }
  if (options.origin !== "builtin" && PROTECTED_ADAPTER_IDS.has(adapterId)) {
    throw new ProviderContractError("protected_identifier", `${path}.adapterId`);
  }
  const displayName = parseContractNonEmptyString(
    requireContractField(record, "displayName", path),
    `${path}.displayName`
  );
  const packageVersion = parseContractNonEmptyString(
    requireContractField(record, "packageVersion", path),
    `${path}.packageVersion`
  );
  const apiVersion = parseContractVersion(
    requireContractField(record, "apiVersion", path),
    `${path}.apiVersion`,
    PROVIDER_ADAPTER_API_VERSION,
    "unsupported_api_version"
  );
  const protocols = parseProtocols(
    requireContractField(record, "protocols", path),
    `${path}.protocols`
  );

  return Object.freeze({
    schemaVersion,
    adapterId,
    displayName,
    packageVersion,
    apiVersion,
    protocols
  });
}

export function parseProviderFeatureProfile(
  value: unknown,
  path = "featureProfile"
): ProviderFeatureProfile {
  const record = requireContractRecord(value, path);
  assertContractKeys(record, ["schemaVersion", "features"], path);
  const schemaVersion = parseContractVersion(
    requireContractField(record, "schemaVersion", path),
    `${path}.schemaVersion`,
    PROVIDER_FEATURE_PROFILE_SCHEMA_VERSION,
    "unsupported_schema_version"
  );
  const featuresPath = `${path}.features`;
  const featureRecord = requireContractRecord(
    requireContractField(record, "features", path),
    featuresPath
  );
  assertContractKeys(
    featureRecord,
    PROVIDER_FEATURE_KEYS,
    featuresPath,
    "unknown_feature"
  );

  const features = {} as Record<ProviderFeature, boolean>;
  for (const feature of PROVIDER_FEATURE_KEYS) {
    features[feature] = parseContractBoolean(
      requireContractField(featureRecord, feature, featuresPath),
      `${featuresPath}.${feature}`
    );
  }

  return Object.freeze({
    schemaVersion,
    features: Object.freeze(features)
  });
}

export function validateProviderContract(operation: () => unknown): ValidationResult {
  try {
    operation();
    return {ok: true};
  } catch (error) {
    const contractError =
      error instanceof ProviderContractError
        ? error
        : new ProviderContractError("invalid_value", "contract");
    return {
      ok: false,
      issues: [
        {
          code: contractError.code,
          path: contractError.path,
          message: contractError.message
        }
      ]
    };
  }
}

/** @internal Shared by the versioned profile parsers. */
export function requireContractRecord(
  value: unknown,
  path: string
): Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new ProviderContractError("invalid_type", path);
  }
  return value as Record<string, unknown>;
}

/** @internal Shared by the versioned profile parsers. */
export function assertContractKeys(
  record: Readonly<Record<string, unknown>>,
  allowedKeys: readonly string[],
  path: string,
  unknownCode: Extract<ProviderContractErrorCode, "unknown_field" | "unknown_feature"> =
    "unknown_field"
): void {
  const allowed = new Set(allowedKeys);
  for (const key of Object.keys(record)) {
    if (!allowed.has(key)) {
      throw new ProviderContractError(unknownCode, `${path}.${key}`);
    }
  }
}

/** @internal Shared by the versioned profile parsers. */
export function requireContractField(
  record: Readonly<Record<string, unknown>>,
  key: string,
  path: string
): unknown {
  if (!Object.prototype.hasOwnProperty.call(record, key)) {
    throw new ProviderContractError("missing_field", `${path}.${key}`);
  }
  return record[key];
}

/** @internal Shared by the versioned profile parsers. */
export function parseContractNonEmptyString(value: unknown, path: string): string {
  if (typeof value !== "string") {
    throw new ProviderContractError("invalid_type", path);
  }
  const normalized = value.trim();
  if (!normalized) {
    throw new ProviderContractError("invalid_value", path);
  }
  return normalized;
}

/** @internal Shared by the versioned profile parsers. */
export function parseContractBoolean(value: unknown, path: string): boolean {
  if (typeof value !== "boolean") {
    throw new ProviderContractError("invalid_type", path);
  }
  return value;
}

/** @internal Shared by the versioned profile parsers. */
export function parseContractVersion<T extends number>(
  value: unknown,
  path: string,
  expected: T,
  code: Extract<
    ProviderContractErrorCode,
    "unsupported_schema_version" | "unsupported_api_version"
  >
): T {
  if (typeof value !== "number" || !Number.isInteger(value)) {
    throw new ProviderContractError("invalid_type", path);
  }
  if (value !== expected) {
    throw new ProviderContractError(code, path);
  }
  return expected;
}

/** @internal Shared by the versioned profile parsers. */
export function parseContractPositiveInteger(value: unknown, path: string): number {
  if (typeof value !== "number" || !Number.isInteger(value)) {
    throw new ProviderContractError("invalid_type", path);
  }
  if (value <= 0) {
    throw new ProviderContractError("invalid_value", path);
  }
  return value;
}

function parseQualifiedId(
  value: unknown,
  expectedKind: "adapter" | "provider" | "model",
  path: string
): string {
  if (typeof value !== "string") {
    throw new ProviderContractError("invalid_type", path);
  }
  const prefix = /^([a-z]+):/.exec(value)?.[1];
  if (prefix && ID_KINDS.has(prefix) && prefix !== expectedKind) {
    throw new ProviderContractError("identifier_kind_mismatch", path);
  }
  if (!QUALIFIED_ID_PATTERN.test(value) || prefix !== expectedKind) {
    throw new ProviderContractError("invalid_identifier", path);
  }
  return value;
}

function parseProtocols(value: unknown, path: string): readonly ApiProtocol[] {
  if (!Array.isArray(value)) {
    throw new ProviderContractError("invalid_type", path);
  }
  if (value.length === 0) {
    throw new ProviderContractError("invalid_value", path);
  }
  const protocols: ApiProtocol[] = [];
  for (const [index, protocol] of value.entries()) {
    if (typeof protocol !== "string" || !API_PROTOCOLS.has(protocol as ApiProtocol)) {
      throw new ProviderContractError("invalid_value", `${path}.${index}`);
    }
    if (protocols.includes(protocol as ApiProtocol)) {
      throw new ProviderContractError("invalid_value", `${path}.${index}`);
    }
    protocols.push(protocol as ApiProtocol);
  }
  return Object.freeze(protocols);
}
