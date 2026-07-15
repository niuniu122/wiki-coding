import assert from "node:assert/strict";
import test from "node:test";
import {auditAgentItems, isExplicitlyRetryable} from "../src/agent/agent-checkpoint.js";
import {createAgentItemEnvelope} from "../src/agent/agent-item.js";
import type {ThreadItem, TurnRecord} from "../src/types.js";

const TURN: TurnRecord = {id: "turn-recovery", threadId: "thread-recovery", userInput: "inspect", status: "running", startedAt: "2026-07-14T00:00:00.000Z"};

function item(sequence: number, payload: Parameters<typeof createAgentItemEnvelope>[1]): ThreadItem {
  return {id: `item-${sequence}`, threadId: TURN.threadId, turnId: TURN.id, type: "agent_item", content: payload.kind, createdAt: TURN.startedAt, agent: createAgentItemEnvelope(sequence, payload)};
}

const checkpoint = {kind: "checkpoint", checkpointId: "checkpoint-1", turnId: TURN.id, lastSequence: 0, continuationGeneration: 0, snapshotVersion: "snapshot-1", modelProfileId: "model:test/local/agent", step: 0, toolCalls: 0, tokensUsed: 0, remainingToolCalls: 4} as const;
const request = {kind: "tool_request", invocationId: "invocation-stable-1", capabilityId: "capability:test/read", arguments: {path: "README.md"}} as const;

test("recovery audit distinguishes pre-dispatch, in-flight and durable-result crashes", () => {
  const base = [item(0, {kind: "user", text: "inspect"}), item(1, checkpoint)];
  assert.equal(auditAgentItems(TURN, base).status, "recoverable");

  const inFlight = auditAgentItems(TURN, [...base, item(2, request)]);
  assert.equal(inFlight.status, "blocked");
  assert.deepEqual(inFlight.unmatchedInvocationIds, [request.invocationId]);

  const afterResult = auditAgentItems(TURN, [...base, item(2, request), item(3, {kind: "tool_result", invocationId: request.invocationId, status: "completed", output: "read"})]);
  assert.equal(afterResult.status, "recoverable");

  const indeterminate = auditAgentItems(TURN, [...base, item(2, request), item(3, {kind: "tool_result", invocationId: request.invocationId, status: "indeterminate", output: "unknown"})]);
  assert.equal(indeterminate.status, "blocked");
});

test("only an explicitly idempotent capability with a stable invocation identity is retryable", () => {
  assert.equal(isExplicitlyRetryable({idempotent: true}, "invocation-stable-1"), true);
  assert.equal(isExplicitlyRetryable({idempotent: false}, "invocation-stable-1"), false);
  assert.equal(isExplicitlyRetryable({idempotent: true}, "provider opaque id"), false);
});

test("duplicate or orphan invocation records fail recovery closed", () => {
  const base = [item(0, {kind: "user", text: "inspect"}), item(1, checkpoint)];
  assert.equal(auditAgentItems(TURN, [...base, item(2, request), item(3, request)]).status, "blocked");
  assert.equal(auditAgentItems(TURN, [...base, item(2, {kind: "tool_result", invocationId: "missing", status: "completed", output: "bad"})]).status, "blocked");
});
