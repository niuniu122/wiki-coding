import {join} from "node:path";
import {readJsonFile, writeJsonFile} from "../utils/jsonl.js";
import type {CapabilitySnapshot} from "./capability-snapshot.js";

export type SnapshotLoadResult =
  | {readonly status: "missing"}
  | {readonly status: "loaded"; readonly snapshot: CapabilitySnapshot}
  | {readonly status: "recovery_required"};

export class CapabilitySnapshotStore {
  readonly path: string;
  constructor(root: string) {
    this.path = join(root, "capability-snapshot.json");
  }

  async load(): Promise<SnapshotLoadResult> {
    try {
      const snapshot = await readJsonFile<CapabilitySnapshot | null>(this.path, null, {validate: isSnapshot});
      return snapshot ? {status: "loaded", snapshot} : {status: "missing"};
    } catch {
      return {status: "recovery_required"};
    }
  }

  async save(snapshot: CapabilitySnapshot): Promise<void> {
    if (!isSnapshot(snapshot)) throw new Error("Invalid capability snapshot.");
    await writeJsonFile(this.path, snapshot, {mode: 0o600});
  }
}

function isSnapshot(value: unknown): value is CapabilitySnapshot {
  if (!value || typeof value !== "object") return false;
  const item = value as Partial<CapabilitySnapshot>;
  return item.schemaVersion === 1 && typeof item.version === "string" &&
    typeof item.createdAt === "string" && typeof item.tokenizerVersion === "string" &&
    typeof item.fingerprint === "string" && Array.isArray(item.entries) &&
    (item.health?.status === "fresh" || item.health?.status === "stale");
}
