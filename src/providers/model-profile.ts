import type {ModelProfileId, ProviderProfileId} from "../types.js";
import {
  ProviderContractError,
  assertContractKeys,
  parseContractBoolean,
  parseContractNonEmptyString,
  parseContractPositiveInteger,
  parseContractVersion,
  parseModelProfileId,
  parseProviderFeatureProfile,
  parseProviderProfileId,
  requireContractField,
  requireContractRecord,
  type ProviderFeatureMatrix
} from "./provider-adapter.js";

export const MODEL_PROFILE_SCHEMA_VERSION = 1 as const;
export const MODEL_FEATURE_PROFILE_SCHEMA_VERSION = 1 as const;

export interface ModelFeatureProfile {
  readonly schemaVersion: 1;
  readonly features: ProviderFeatureMatrix;
  readonly contextWindow: number;
  readonly maxOutputTokens: number;
}

export interface ModelProfile {
  readonly schemaVersion: 1;
  readonly modelProfileId: ModelProfileId;
  readonly providerProfileId: ProviderProfileId;
  readonly displayName: string;
  readonly model: string;
  readonly enabled: boolean;
  readonly featureProfile: ModelFeatureProfile;
}

export function parseModelFeatureProfile(
  value: unknown,
  path = "modelFeatureProfile"
): ModelFeatureProfile {
  const record = requireContractRecord(value, path);
  assertContractKeys(
    record,
    ["schemaVersion", "features", "contextWindow", "maxOutputTokens"],
    path
  );
  const schemaVersion = parseContractVersion(
    requireContractField(record, "schemaVersion", path),
    `${path}.schemaVersion`,
    MODEL_FEATURE_PROFILE_SCHEMA_VERSION,
    "unsupported_schema_version"
  );
  const {features} = parseProviderFeatureProfile(
    {
      schemaVersion: 1,
      features: requireContractField(record, "features", path)
    },
    path
  );
  const contextWindow = parseContractPositiveInteger(
    requireContractField(record, "contextWindow", path),
    `${path}.contextWindow`
  );
  const maxOutputTokens = parseContractPositiveInteger(
    requireContractField(record, "maxOutputTokens", path),
    `${path}.maxOutputTokens`
  );
  if (maxOutputTokens > contextWindow) {
    throw new ProviderContractError("invalid_value", `${path}.maxOutputTokens`);
  }

  return Object.freeze({
    schemaVersion,
    features,
    contextWindow,
    maxOutputTokens
  });
}

export function parseModelProfile(value: unknown): ModelProfile {
  const path = "modelProfile";
  const record = requireContractRecord(value, path);
  assertContractKeys(record, [
    "schemaVersion",
    "modelProfileId",
    "providerProfileId",
    "displayName",
    "model",
    "enabled",
    "featureProfile"
  ], path);

  const schemaVersion = parseContractVersion(
    requireContractField(record, "schemaVersion", path),
    `${path}.schemaVersion`,
    MODEL_PROFILE_SCHEMA_VERSION,
    "unsupported_schema_version"
  );
  const modelProfileId = parseModelProfileId(
    requireContractField(record, "modelProfileId", path),
    `${path}.modelProfileId`
  );
  const providerProfileId = parseProviderProfileId(
    requireContractField(record, "providerProfileId", path),
    `${path}.providerProfileId`
  );
  const displayName = parseContractNonEmptyString(
    requireContractField(record, "displayName", path),
    `${path}.displayName`
  );
  const model = parseContractNonEmptyString(
    requireContractField(record, "model", path),
    `${path}.model`
  );
  const enabled = parseContractBoolean(
    requireContractField(record, "enabled", path),
    `${path}.enabled`
  );
  const featureProfile = parseModelFeatureProfile(
    requireContractField(record, "featureProfile", path),
    `${path}.featureProfile`
  );

  return Object.freeze({
    schemaVersion,
    modelProfileId,
    providerProfileId,
    displayName,
    model,
    enabled,
    featureProfile
  });
}
