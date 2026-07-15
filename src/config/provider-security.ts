import {createHash} from "node:crypto";
import type {ApiProtocol, ModelProviderConfig} from "../types.js";

export type ProviderAuthScheme = "bearer";

export interface CredentialTarget {
  providerId: string;
  endpoint: string;
  protocol: ApiProtocol;
  authScheme: ProviderAuthScheme;
  fingerprint: string;
  configuredEnvironmentKey?: string;
}

export interface TrustedCredentialBinding {
  environmentKey: string;
  legacyProviderId: string;
}

interface TrustedProviderBinding {
  endpoint: string;
  protocol: ApiProtocol;
  environmentKey: string;
}

const TRUSTED_PROVIDER_BINDINGS: Readonly<Record<string, TrustedProviderBinding>> = {
  "minimax-official": {
    endpoint: "https://api.minimax.io/v1",
    protocol: "responses",
    environmentKey: "MINIMAX_API_KEY"
  },
  hashsight: {
    endpoint: "https://www.hashsight.cn/v1",
    protocol: "chat_completions",
    environmentKey: "HASHSIGHT_API_KEY"
  }
};

const PUBLIC_HEADER_NAMES: Readonly<Record<string, string>> = {
  accept: "Accept",
  "user-agent": "User-Agent",
  "openai-beta": "OpenAI-Beta",
  "anthropic-version": "Anthropic-Version",
  "http-referer": "HTTP-Referer",
  "x-title": "X-Title"
};
const HEADER_TOKEN = /^[!#$%&'*+.^_`|~0-9A-Za-z-]+$/;
const INVALID_HEADER_VALUE = /[\u0000-\u001f\u007f]/;

export function normalizeProviderEndpoint(value: string): string {
  let url: URL;
  try {
    url = new URL(value);
  } catch {
    throw new Error("Provider baseUrl must be an absolute HTTP URL.");
  }
  if (url.protocol !== "http:" && url.protocol !== "https:") {
    throw new Error("Provider baseUrl must use HTTP or HTTPS.");
  }
  if (url.username || url.password) {
    throw new Error("Provider baseUrl must not contain userinfo.");
  }
  if (url.search) {
    throw new Error("Provider baseUrl must not contain a query.");
  }
  if (url.hash) {
    throw new Error("Provider baseUrl must not contain a fragment.");
  }
  url.pathname = url.pathname.replace(/\/+$/, "") || "/";
  return url.toString().replace(/\/$/, url.pathname === "/" ? "" : "");
}

export function assertProviderSecurity(provider: ModelProviderConfig): void {
  const endpoint = normalizeProviderEndpoint(provider.baseUrl);
  const url = new URL(endpoint);
  if (url.protocol === "http:") {
    if (!provider.allowInsecureLoopback) {
      throw new Error(
        "Provider HTTP endpoints require allowInsecureLoopback: true and a loopback host; HTTPS is required otherwise."
      );
    }
    if (!isLoopbackHostname(url.hostname)) {
      throw new Error("Provider HTTP endpoints are restricted to loopback hosts; HTTPS is required otherwise.");
    }
  }
  normalizePublicProviderHeaders(provider.headers);
}

export function normalizePublicProviderHeaders(
  headers: Record<string, string> | undefined
): Record<string, string> | undefined {
  if (!headers) {
    return undefined;
  }
  const normalizedHeaders: Record<string, string> = {};
  for (const [name, value] of Object.entries(headers)) {
    const normalized = name.toLowerCase();
    const canonical = PUBLIC_HEADER_NAMES[normalized];
    if (!HEADER_TOKEN.test(name) || !canonical) {
      throw new Error(`Provider header ${name} is not allowed in workspace configuration.`);
    }
    if (INVALID_HEADER_VALUE.test(value)) {
      throw new Error(`Provider header ${name} contains prohibited control characters.`);
    }
    normalizedHeaders[canonical] = value;
  }
  return normalizedHeaders;
}

export function createCredentialTarget(
  providerId: string,
  provider: ModelProviderConfig
): CredentialTarget {
  assertProviderSecurity(provider);
  const endpoint = normalizeProviderEndpoint(provider.baseUrl);
  const authScheme: ProviderAuthScheme = "bearer";
  const targetBase = {providerId, endpoint, protocol: provider.protocol, authScheme};
  const fingerprint = credentialTargetFingerprint(targetBase);
  return {
    ...targetBase,
    fingerprint,
    ...(provider.envKey ? {configuredEnvironmentKey: provider.envKey} : {})
  };
}

export function resolveTrustedCredentialBinding(
  target: CredentialTarget
): TrustedCredentialBinding | undefined {
  const trusted = TRUSTED_PROVIDER_BINDINGS[target.providerId];
  if (
    !trusted ||
    normalizeProviderEndpoint(target.endpoint) !== normalizeProviderEndpoint(trusted.endpoint) ||
    target.protocol !== trusted.protocol ||
    target.authScheme !== "bearer" ||
    target.configuredEnvironmentKey !== trusted.environmentKey
  ) {
    return undefined;
  }
  return {
    environmentKey: trusted.environmentKey,
    legacyProviderId: target.providerId
  };
}

export function credentialTargetFingerprint(
  target: Pick<CredentialTarget, "providerId" | "endpoint" | "authScheme">
): string {
  const endpoint = normalizeProviderEndpoint(target.endpoint);
  return createHash("sha256")
    .update(JSON.stringify({
      providerId: target.providerId,
      endpoint,
      authScheme: target.authScheme
    }))
    .digest("hex");
}

function isLoopbackHostname(hostname: string): boolean {
  const normalized = hostname.toLowerCase();
  if (normalized === "localhost" || normalized === "[::1]" || normalized === "::1") {
    return true;
  }
  const match = /^(\d+)\.(\d+)\.(\d+)\.(\d+)$/.exec(normalized);
  return match !== null && Number(match[1]) === 127 && match.slice(1).every((part) => Number(part) <= 255);
}
