import {randomUUID} from "node:crypto";
import type {AgentBudget} from "./agent-budget.js";
import type {AgentItemPayload} from "./agent-item.js";
import type {ThreadItem, TurnRecord} from "../types.js";

export type AgentCheckpointPayload = Extract<AgentItemPayload, {kind: "checkpoint"}>;

export interface AgentRecoveryAudit {
  readonly status: "none" | "complete" | "recoverable" | "blocked";
  readonly checkpoint?: AgentCheckpointPayload;
  readonly unmatchedInvocationIds: readonly string[];
  readonly nextSequence: number;
}

export function createAgentCheckpoint(input: {
  turnId: string;
  lastSequence: number;
  continuationGeneration: number;
  snapshotVersion: string;
  modelProfileId: string;
  budget: AgentBudget;
}): AgentCheckpointPayload {
  const state = input.budget.snapshot;
  return Object.freeze({
    kind: "checkpoint",
    checkpointId: `checkpoint-${randomUUID()}`,
    turnId: input.turnId,
    lastSequence: input.lastSequence,
    continuationGeneration: input.continuationGeneration,
    snapshotVersion: input.snapshotVersion,
    modelProfileId: input.modelProfileId,
    step: state.steps,
    toolCalls: state.toolCalls,
    tokensUsed: state.tokens,
    remainingToolCalls: state.remainingToolCalls
  });
}

export function auditAgentItems(turn: TurnRecord, items: readonly ThreadItem[]): AgentRecoveryAudit {
  const envelopes = items
    .filter((item) => item.turnId === turn.id && item.type === "agent_item" && item.agent)
    .map((item) => item.agent!)
    .sort((left, right) => left.sequence - right.sequence);
  if (envelopes.length === 0) return {status: "none", unmatchedInvocationIds: [], nextSequence: 0};
  const sequences = new Set<number>();
  const requests = new Map<string, number>();
  const results = new Set<string>();
  let checkpoint: AgentCheckpointPayload | undefined;
  let hasFinal = false;
  let hasIndeterminate = false;
  let invalid = false;
  for (const envelope of envelopes) {
    if (sequences.has(envelope.sequence)) invalid = true;
    sequences.add(envelope.sequence);
    const payload = envelope.payload;
    if (payload.kind === "tool_request") {
      if (requests.has(payload.invocationId)) invalid = true;
      requests.set(payload.invocationId, envelope.sequence);
    } else if (payload.kind === "tool_result") {
      if (!requests.has(payload.invocationId) || results.has(payload.invocationId)) invalid = true;
      results.add(payload.invocationId);
      hasIndeterminate ||= payload.status === "indeterminate";
    } else if (payload.kind === "checkpoint") {
      if (payload.turnId !== turn.id || payload.lastSequence >= envelope.sequence) invalid = true;
      checkpoint = payload;
    } else if (payload.kind === "final") {
      hasFinal = true;
    }
  }
  const nextSequence = Math.max(...envelopes.map((item) => item.sequence)) + 1;
  const unmatchedInvocationIds = [...requests.keys()].filter((id) => !results.has(id));
  if (invalid || hasIndeterminate || unmatchedInvocationIds.length > 0) return {status: "blocked", ...(checkpoint ? {checkpoint} : {}), unmatchedInvocationIds, nextSequence};
  if (hasFinal) return {status: "complete", ...(checkpoint ? {checkpoint} : {}), unmatchedInvocationIds: [], nextSequence};
  return checkpoint
    ? {status: "recoverable", checkpoint, unmatchedInvocationIds: [], nextSequence}
    : {status: "blocked", unmatchedInvocationIds: [], nextSequence};
}

export function isExplicitlyRetryable(item: {idempotent: boolean}, invocationId: string): boolean {
  return item.idempotent && /^invocation-[A-Za-z0-9._-]{1,160}$/.test(invocationId);
}
