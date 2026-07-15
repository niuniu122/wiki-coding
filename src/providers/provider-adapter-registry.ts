import type {ApiProtocol, ProviderAdapterId} from "../types.js";
import {
  parseProviderAdapterId,
  parseProviderAdapterManifest,
  type ProviderAdapter
} from "./provider-adapter.js";
import {BuiltinProviderAdapter} from "./builtin-provider-adapter.js";

export interface AdapterConformanceEvidence {
  readonly schemaVersion: 1;
  readonly adapterId: ProviderAdapterId;
  readonly fixtureVersion: string;
  readonly protocols: readonly ApiProtocol[];
}

export type ProviderAdapterRegistryErrorCode =
  | "duplicate_adapter"
  | "invalid_conformance_evidence"
  | "dynamic_packages_disabled";

export class ProviderAdapterRegistryError extends Error {
  constructor(readonly code: ProviderAdapterRegistryErrorCode) {
    super(`Provider adapter registry rejected the operation (${code}).`);
    this.name = "ProviderAdapterRegistryError";
  }
}

interface RegisteredAdapter {
  readonly adapter: ProviderAdapter;
  readonly conformanceProtocols: ReadonlySet<ApiProtocol>;
}

export class ProviderAdapterRegistry {
  private readonly adapters = new Map<ProviderAdapterId, RegisteredAdapter>();

  registerBuiltin(
    adapter: ProviderAdapter,
    evidence: AdapterConformanceEvidence
  ): void {
    const manifest = parseProviderAdapterManifest(adapter.manifest, {origin: "builtin"});
    if (this.adapters.has(manifest.adapterId)) {
      throw new ProviderAdapterRegistryError("duplicate_adapter");
    }
    const parsedEvidence = parseConformanceEvidence(evidence);
    if (parsedEvidence.adapterId !== manifest.adapterId) {
      throw new ProviderAdapterRegistryError("invalid_conformance_evidence");
    }
    if (
      parsedEvidence.protocols.some(
        (protocol) => !manifest.protocols.includes(protocol)
      )
    ) {
      throw new ProviderAdapterRegistryError("invalid_conformance_evidence");
    }
    this.adapters.set(manifest.adapterId, {
      adapter,
      conformanceProtocols: new Set(parsedEvidence.protocols)
    });
  }

  get(adapterId: ProviderAdapterId | string): ProviderAdapter | undefined {
    let parsed: ProviderAdapterId;
    try {
      parsed = parseProviderAdapterId(adapterId);
    } catch {
      return undefined;
    }
    return this.adapters.get(parsed)?.adapter;
  }

  list(): readonly ProviderAdapter[] {
    return Object.freeze([...this.adapters.values()].map(({adapter}) => adapter));
  }

  hasConformanceFixture(
    adapterId: ProviderAdapterId | string,
    protocol: ApiProtocol
  ): boolean {
    let parsed: ProviderAdapterId;
    try {
      parsed = parseProviderAdapterId(adapterId);
    } catch {
      return false;
    }
    return this.adapters.get(parsed)?.conformanceProtocols.has(protocol) ?? false;
  }

  async loadDynamicPackage(_entrypoint: string): Promise<never> {
    throw new ProviderAdapterRegistryError("dynamic_packages_disabled");
  }
}

export function createBuiltinConformanceEvidence(
  adapter: BuiltinProviderAdapter
): AdapterConformanceEvidence {
  return Object.freeze({
    schemaVersion: 1,
    adapterId: adapter.manifest.adapterId,
    fixtureVersion: "1",
    protocols: Object.freeze([...adapter.manifest.protocols])
  });
}

export function createDefaultProviderAdapterRegistry(
  adapter = new BuiltinProviderAdapter()
): ProviderAdapterRegistry {
  const registry = new ProviderAdapterRegistry();
  registry.registerBuiltin(adapter, createBuiltinConformanceEvidence(adapter));
  return registry;
}

function parseConformanceEvidence(
  value: unknown
): AdapterConformanceEvidence {
  if (!isRecord(value)) {
    throw new ProviderAdapterRegistryError("invalid_conformance_evidence");
  }
  const keys = Object.keys(value);
  if (
    keys.some(
      (key) =>
        key !== "schemaVersion" &&
        key !== "adapterId" &&
        key !== "fixtureVersion" &&
        key !== "protocols"
    ) ||
    value.schemaVersion !== 1 ||
    typeof value.fixtureVersion !== "string" ||
    value.fixtureVersion.trim().length === 0 ||
    !Array.isArray(value.protocols) ||
    new Set(value.protocols).size !== value.protocols.length ||
    value.protocols.some(
      (protocol) => protocol !== "responses" && protocol !== "chat_completions"
    )
  ) {
    throw new ProviderAdapterRegistryError("invalid_conformance_evidence");
  }
  let adapterId: ProviderAdapterId;
  try {
    adapterId = parseProviderAdapterId(value.adapterId);
  } catch {
    throw new ProviderAdapterRegistryError("invalid_conformance_evidence");
  }
  return Object.freeze({
    schemaVersion: 1,
    adapterId,
    fixtureVersion: value.fixtureVersion.trim(),
    protocols: Object.freeze([...value.protocols])
  });
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
