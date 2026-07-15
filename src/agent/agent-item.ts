export type AgentItemKind = "user" | "assistant" | "tool_request" | "tool_result" | "checkpoint" | "error" | "final";

export type AgentItemPayload =
  | {readonly kind: "user"; readonly text: string}
  | {readonly kind: "assistant"; readonly text: string}
  | {readonly kind: "tool_request"; readonly invocationId: string; readonly capabilityId: string; readonly arguments: Readonly<Record<string, unknown>>}
  | {readonly kind: "tool_result"; readonly invocationId: string; readonly status: "completed" | "failed" | "indeterminate"; readonly output: string}
  | {readonly kind: "checkpoint"; readonly checkpointId: string; readonly turnId: string; readonly lastSequence: number; readonly continuationGeneration: number; readonly snapshotVersion: string; readonly modelProfileId: string; readonly step: number; readonly toolCalls: number; readonly tokensUsed: number; readonly remainingToolCalls: number}
  | {readonly kind: "error"; readonly code: string; readonly message: string}
  | {readonly kind: "final"; readonly text: string};

export interface AgentItemEnvelope {readonly schemaVersion: 1; readonly sequence: number; readonly payload: AgentItemPayload}

export function createAgentItemEnvelope(sequence: number, payload: AgentItemPayload): AgentItemEnvelope {
  if (!Number.isInteger(sequence) || sequence < 0) throw new Error("Agent item sequence must be a non-negative integer.");
  validatePayload(payload);
  return Object.freeze({schemaVersion: 1, sequence, payload: Object.freeze({...payload}) as AgentItemPayload});
}

export function isAgentItemEnvelope(value: unknown): value is AgentItemEnvelope {
  if (!value || typeof value !== "object" || Array.isArray(value)) return false;
  const item = value as Partial<AgentItemEnvelope>;
  if (item.schemaVersion !== 1 || !Number.isInteger(item.sequence) || (item.sequence ?? -1) < 0 || !item.payload) return false;
  try { validatePayload(item.payload); return true; } catch { return false; }
}

function validatePayload(payload: AgentItemPayload): void {
  if (!payload || typeof payload !== "object") throw new Error("Invalid Agent item payload.");
  const bounded = (value: unknown, max = 32_000) => typeof value === "string" && value.length <= max;
  switch (payload.kind) {
    case "user": case "assistant": case "final": if (!bounded(payload.text)) throw new Error("Invalid Agent text payload."); return;
    case "tool_request": if (!bounded(payload.invocationId, 200) || !bounded(payload.capabilityId, 300) || !payload.arguments || typeof payload.arguments !== "object" || Array.isArray(payload.arguments) || JSON.stringify(payload.arguments).length > 16_000) throw new Error("Invalid tool request payload."); return;
    case "tool_result": if (!bounded(payload.invocationId, 200) || !bounded(payload.output) || !["completed", "failed", "indeterminate"].includes(payload.status)) throw new Error("Invalid tool result payload."); return;
    case "checkpoint": if (!bounded(payload.checkpointId, 200) || !bounded(payload.turnId, 200) || !bounded(payload.snapshotVersion, 200) || !bounded(payload.modelProfileId, 300) || !Number.isInteger(payload.lastSequence) || payload.lastSequence < 0 || !Number.isInteger(payload.continuationGeneration) || payload.continuationGeneration < 0 || !Number.isInteger(payload.step) || payload.step < 0 || !Number.isInteger(payload.toolCalls) || payload.toolCalls < 0 || !Number.isInteger(payload.tokensUsed) || payload.tokensUsed < 0 || !Number.isInteger(payload.remainingToolCalls) || payload.remainingToolCalls < 0) throw new Error("Invalid checkpoint payload."); return;
    case "error": if (!bounded(payload.code, 200) || !bounded(payload.message, 4_000)) throw new Error("Invalid Agent error payload."); return;
    default: throw new Error("Unknown Agent item payload.");
  }
}
