import type {ApiProtocol, ProviderAdapterId, ProviderProfileId} from "../types.js";
import {normalizePublicProviderHeaders} from "../config/provider-security.js";
import {
  ProviderContractError,
  assertContractKeys,
  parseContractBoolean,
  parseContractNonEmptyString,
  parseContractVersion,
  parseProviderAdapterId,
  parseProviderProfileId,
  requireContractField,
  requireContractRecord
} from "./provider-adapter.js";

export const PROVIDER_PROFILE_SCHEMA_VERSION = 1 as const;

export interface ProviderTransportProfile {
  readonly baseUrl: string;
  readonly protocol: ApiProtocol;
  readonly publicHeaders: Readonly<Record<string, string>>;
  readonly allowInsecureLoopback: boolean;
}

export interface ProviderAuthenticationProfile {
  readonly kind: "bearer";
  readonly envBinding?: string;
}

export interface ProviderProfile {
  readonly schemaVersion: 1;
  readonly providerProfileId: ProviderProfileId;
  readonly adapterId: ProviderAdapterId;
  readonly displayName: string;
  readonly enabled: boolean;
  readonly transport: ProviderTransportProfile;
  readonly authentication: ProviderAuthenticationProfile;
}

export function parseProviderProfile(value: unknown): ProviderProfile {
  const path = "providerProfile";
  const record = requireContractRecord(value, path);
  assertContractKeys(record, [
    "schemaVersion",
    "providerProfileId",
    "adapterId",
    "displayName",
    "enabled",
    "transport",
    "authentication"
  ], path);

  const schemaVersion = parseContractVersion(
    requireContractField(record, "schemaVersion", path),
    `${path}.schemaVersion`,
    PROVIDER_PROFILE_SCHEMA_VERSION,
    "unsupported_schema_version"
  );
  const providerProfileId = parseProviderProfileId(
    requireContractField(record, "providerProfileId", path),
    `${path}.providerProfileId`
  );
  const adapterId = parseProviderAdapterId(
    requireContractField(record, "adapterId", path),
    `${path}.adapterId`
  );
  const displayName = parseContractNonEmptyString(
    requireContractField(record, "displayName", path),
    `${path}.displayName`
  );
  const enabled = parseContractBoolean(
    requireContractField(record, "enabled", path),
    `${path}.enabled`
  );
  const transport = parseTransport(
    requireContractField(record, "transport", path),
    `${path}.transport`
  );
  const authentication = parseAuthentication(
    requireContractField(record, "authentication", path),
    `${path}.authentication`
  );

  return Object.freeze({
    schemaVersion,
    providerProfileId,
    adapterId,
    displayName,
    enabled,
    transport,
    authentication
  });
}

function parseTransport(value: unknown, path: string): ProviderTransportProfile {
  const record = requireContractRecord(value, path);
  assertContractKeys(
    record,
    ["baseUrl", "protocol", "publicHeaders", "allowInsecureLoopback"],
    path
  );
  const baseUrl = parseContractNonEmptyString(
    requireContractField(record, "baseUrl", path),
    `${path}.baseUrl`
  );
  const protocol = parseProtocol(
    requireContractField(record, "protocol", path),
    `${path}.protocol`
  );
  const publicHeaders = Object.prototype.hasOwnProperty.call(record, "publicHeaders")
    ? parsePublicHeaders(record.publicHeaders, `${path}.publicHeaders`)
    : Object.freeze({});
  const allowInsecureLoopback = Object.prototype.hasOwnProperty.call(
    record,
    "allowInsecureLoopback"
  )
    ? parseContractBoolean(record.allowInsecureLoopback, `${path}.allowInsecureLoopback`)
    : false;

  return Object.freeze({
    baseUrl,
    protocol,
    publicHeaders,
    allowInsecureLoopback
  });
}

function parseAuthentication(
  value: unknown,
  path: string
): ProviderAuthenticationProfile {
  const record = requireContractRecord(value, path);
  assertContractKeys(record, ["kind", "envBinding"], path);
  const kind = parseContractNonEmptyString(
    requireContractField(record, "kind", path),
    `${path}.kind`
  );
  if (kind !== "bearer") {
    throw new ProviderContractError("invalid_value", `${path}.kind`);
  }

  if (!Object.prototype.hasOwnProperty.call(record, "envBinding")) {
    return Object.freeze({kind});
  }
  const envBinding = parseContractNonEmptyString(
    record.envBinding,
    `${path}.envBinding`
  );
  if (!/^[A-Z][A-Z0-9_]*$/.test(envBinding)) {
    throw new ProviderContractError("invalid_value", `${path}.envBinding`);
  }
  return Object.freeze({kind, envBinding});
}

function parseProtocol(value: unknown, path: string): ApiProtocol {
  if (value !== "responses" && value !== "chat_completions") {
    throw new ProviderContractError(
      typeof value === "string" ? "invalid_value" : "invalid_type",
      path
    );
  }
  return value;
}

function parsePublicHeaders(
  value: unknown,
  path: string
): Readonly<Record<string, string>> {
  const record = requireContractRecord(value, path);
  const headers: Record<string, string> = {};
  for (const [name, headerValue] of Object.entries(record)) {
    if (typeof headerValue !== "string") {
      throw new ProviderContractError("invalid_type", `${path}.${name}`);
    }
    headers[name] = headerValue;
  }
  try {
    return Object.freeze(normalizePublicProviderHeaders(headers) ?? {});
  } catch {
    throw new ProviderContractError("invalid_value", path);
  }
}
