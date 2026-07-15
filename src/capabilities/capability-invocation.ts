import {randomUUID} from "node:crypto";

export interface CapabilityInvocation {
  readonly schemaVersion: 1;
  readonly invocationId: string;
  readonly capabilityId: string;
  readonly snapshotVersion: string;
  readonly arguments: Readonly<Record<string, unknown>>;
  readonly approved: boolean;
}

export function createCapabilityInvocation(input: Omit<CapabilityInvocation, "schemaVersion" | "invocationId"> & {invocationId?: string}): CapabilityInvocation {
  if (!input.capabilityId.startsWith("capability:") || !input.snapshotVersion || !input.arguments || typeof input.arguments !== "object" || Array.isArray(input.arguments) || JSON.stringify(input.arguments).length > 16_000) throw new Error("Invalid capability invocation.");
  return Object.freeze({schemaVersion: 1, invocationId: input.invocationId ?? `invocation-${randomUUID()}`, capabilityId: input.capabilityId, snapshotVersion: input.snapshotVersion, arguments: Object.freeze({...input.arguments}), approved: input.approved});
}
