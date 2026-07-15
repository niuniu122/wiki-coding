import assert from "node:assert/strict";
import test from "node:test";
import {formatHistoryMessages} from "../src/ui/format-runtime-event.js";
import {initialUiState, reduceRuntimeEvent} from "../src/ui/ui-state.js";
import {createAgentItemEnvelope} from "../src/agent/agent-item.js";
import type {ThreadItem} from "../src/types.js";

function ready() {
  return reduceRuntimeEvent(initialUiState(), {type: "runtime.ready", hasApiKey: true, providerSummary: "test", recoveredTurns: 0});
}

test("Agent reducer displays retrieval, tool, permission, checkpoint and final states", () => {
  let state = reduceRuntimeEvent(ready(), {type: "agent.started", turnId: "turn-agent", input: "inspect"});
  state = reduceRuntimeEvent(state, {type: "agent.retrieval.started", turnId: "turn-agent", query: "inspect"});
  state = reduceRuntimeEvent(state, {type: "agent.retrieval.completed", turnId: "turn-agent", snapshotVersion: "v1", candidates: ["capability:test/read"], path: "exact"});
  state = reduceRuntimeEvent(state, {type: "agent.tool.requested", turnId: "turn-agent", invocationId: "inv-1", capabilityId: "capability:test/read"});
  state = reduceRuntimeEvent(state, {type: "agent.tool.completed", turnId: "turn-agent", invocationId: "inv-1", status: "succeeded"});
  state = reduceRuntimeEvent(state, {type: "agent.permission.required", turnId: "turn-agent", invocationId: "inv-2", capabilityId: "capability:test/test"});
  assert.equal(state.recoverableAgentTurnId, "turn-agent");
  assert.match(state.messages.map((item) => item.content).join("\n"), /召回.*capability:test\/read|capability:test\/read.*召回/);
  assert.match(state.messages.map((item) => item.content).join("\n"), /需要确认/);
  state = reduceRuntimeEvent(state, {type: "agent.recovery.available", turnId: "turn-agent", checkpointId: "checkpoint-1"});
  assert.match(state.status, /checkpoint-1/);
  state = reduceRuntimeEvent(state, {type: "agent.continued", turnId: "turn-agent", checkpointId: "checkpoint-1"});
  state = reduceRuntimeEvent(state, {type: "agent.assistant.delta", turnId: "turn-agent", delta: "final"});
  const finalItem: ThreadItem = {id: "final-item", threadId: "thread-1", turnId: "turn-agent", type: "agent_item", content: "final", createdAt: "2026-07-14T00:00:00.000Z", agent: createAgentItemEnvelope(9, {kind: "final", text: "final"})};
  state = reduceRuntimeEvent(state, {type: "agent.completed", turnId: "turn-agent", item: finalItem});
  assert.equal(state.phase, "idle");
  assert.equal(state.recoverableAgentTurnId, null);
  assert.equal(state.messages.find((item) => item.id === "assistant-turn-agent")?.content, "final");
});

test("history shows bounded Agent status but never expands tool output", () => {
  const secret = "SECRET_TOOL_OUTPUT_" + "x".repeat(10_000);
  const base = {threadId: "thread-1", turnId: "turn-1", type: "agent_item" as const, createdAt: "2026-07-14T00:00:00.000Z"};
  const items: ThreadItem[] = [
    {...base, id: "u", content: "inspect", agent: createAgentItemEnvelope(0, {kind: "user", text: "inspect"})},
    {...base, id: "r", content: "Tool request", agent: createAgentItemEnvelope(1, {kind: "tool_request", invocationId: "inv-1", capabilityId: "capability:test/read", arguments: {path: "README.md"}})},
    {...base, id: "o", content: "Tool result", agent: createAgentItemEnvelope(2, {kind: "tool_result", invocationId: "inv-1", status: "completed", output: secret})},
    {...base, id: "c", content: "Checkpoint", agent: createAgentItemEnvelope(3, {kind: "checkpoint", checkpointId: "cp-1", turnId: "turn-1", lastSequence: 2, continuationGeneration: 0, snapshotVersion: "v1", modelProfileId: "model:test/local/agent", step: 1, toolCalls: 1, tokensUsed: 10, remainingToolCalls: 2})},
    {...base, id: "f", content: "done", agent: createAgentItemEnvelope(4, {kind: "final", text: "done"})}
  ];
  const formatted = formatHistoryMessages(items);
  assert.doesNotMatch(JSON.stringify(formatted), /SECRET_TOOL_OUTPUT/);
  assert.match(JSON.stringify(formatted), /Agent 本机能力结果：completed/);
  assert.match(JSON.stringify(formatted), /Agent 检查点：cp-1/);
  assert.equal(formatted.at(-1)?.content, "done");
});
