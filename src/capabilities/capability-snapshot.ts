import {createHash} from "node:crypto";
import type {CapabilityCatalogEntry} from "./capability-catalog.js";

export interface CapabilitySnapshot {
  readonly schemaVersion: 1;
  readonly version: string;
  readonly createdAt: string;
  readonly tokenizerVersion: string;
  readonly fingerprint: string;
  readonly entries: readonly CapabilityCatalogEntry[];
  readonly health: {
    readonly status: "fresh" | "stale";
    readonly reason?: string;
  };
}

export function createCapabilitySnapshot(
  entries: readonly CapabilityCatalogEntry[],
  options: {version?: string; tokenizerVersion?: string; now?: string} = {}
): CapabilitySnapshot {
  const serialized = JSON.stringify(entries);
  return Object.freeze({
    schemaVersion: 1,
    version: options.version ?? createHash("sha256").update(serialized).digest("hex").slice(0, 16),
    createdAt: options.now ?? new Date().toISOString(),
    tokenizerVersion: options.tokenizerVersion ?? "lexical-v1",
    fingerprint: createHash("sha256").update(serialized).digest("hex"),
    entries: Object.freeze([...entries]),
    health: Object.freeze({status: "fresh"})
  });
}

export function staleCapabilitySnapshot(snapshot: CapabilitySnapshot, reason: string): CapabilitySnapshot {
  return Object.freeze({...snapshot, health: Object.freeze({status: "stale", reason})});
}
