import assert from "node:assert/strict";
import {mkdtemp, rm} from "node:fs/promises";
import {tmpdir} from "node:os";
import {join} from "node:path";
import test from "node:test";
import {AgentBudget} from "../src/agent/agent-budget.js";
import {createAgentCheckpoint} from "../src/agent/agent-checkpoint.js";
import {parseCapabilityManifest} from "../src/capabilities/capability-manifest.js";
import {buildCapabilityCards} from "../src/capabilities/search/capability-card.js";
import type {RuntimeEvent} from "../src/protocol.js";
import {AgentRunEngine} from "../src/runtime/agent-run-engine.js";
import {SessionService} from "../src/runtime/session-service.js";
import {JsonlStorageProvider} from "../src/storage/jsonl-storage.js";

const MODEL = "model:test/local/agent";
const CAPABILITY = parseCapabilityManifest({schemaVersion: 1, id: "capability:test/read", name: "read", description: "read", safetyClass: "workspace_read", idempotent: true, execution: {kind: "workspace_read", operation: "read_file"}}, {kind: "minimax", scope: "builtin", root: "builtin", file: "read.json"});

async function crashedRun(stage: "before_dispatch" | "executing" | "result_durable") {
  const root = await mkdtemp(join(tmpdir(), "agent-recovery-"));
  const repository = new JsonlStorageProvider(root);
  const first = new SessionService(repository);
  await first.init("agent", root);
  const turn = await first.createTurn("inspect", {schemaVersion: 1, adapterId: "adapter:test/builtin", providerProfileId: "provider:test/local", modelProfileId: MODEL, model: "agent", protocol: "responses"});
  await repository.appendItem(first.createAgentItem({turnId: turn.id, sequence: 0, payload: {kind: "user", text: "inspect"}}));
  const savedBudget = new AgentBudget({maxSteps: 4, maxToolCalls: 4, maxTotalTokens: 100_000, timeoutMs: 5_000});
  for (let index = 0; index < 4; index += 1) savedBudget.beginStep();
  await repository.appendItem(first.createAgentItem({turnId: turn.id, sequence: 1, payload: createAgentCheckpoint({turnId: turn.id, lastSequence: 0, continuationGeneration: 0, snapshotVersion: "snapshot-v1", modelProfileId: MODEL, budget: savedBudget})}));
  if (stage !== "before_dispatch") {
    await repository.appendItem(first.createAgentItem({turnId: turn.id, sequence: 2, payload: {kind: "tool_request", invocationId: "invocation-stable", capabilityId: CAPABILITY.id, arguments: {path: "README.md"}}}));
  }
  if (stage === "result_durable") {
    await repository.appendItem(first.createAgentItem({turnId: turn.id, sequence: 3, payload: {kind: "tool_result", invocationId: "invocation-stable", status: "completed", output: "durable file contents"}}));
  }
  const restarted = new SessionService(repository);
  const events = await restarted.init("agent", root);
  return {root, repository, session: restarted, turn, events};
}

test("restart marks an in-flight invocation indeterminate and never offers automatic continuation", async () => {
  const f = await crashedRun("executing");
  try {
    assert.equal(f.events.some((event) => event.type === "agent.recovery.blocked"), true);
    const snapshot = await f.repository.readThread(f.session.activeThread.id);
    const result = snapshot.items.find((item) => item.agent?.payload.kind === "tool_result");
    assert.equal(result?.agent?.payload.kind === "tool_result" && result.agent.payload.status, "indeterminate");
    assert.equal(snapshot.turns.find((turn) => turn.id === f.turn.id)?.status, "interrupted");
  } finally { await rm(f.root, {recursive: true, force: true}); }
});

test("restart offers recovery before dispatch and after a durable result", async () => {
  for (const stage of ["before_dispatch", "result_durable"] as const) {
    const f = await crashedRun(stage);
    try {
      assert.equal(f.events.some((event) => event.type === "agent.recovery.available"), true, stage);
    } finally { await rm(f.root, {recursive: true, force: true}); }
  }
});

test("continue replays a durable tool result into model context without dispatching it again", async () => {
  const f = await crashedRun("result_durable");
  try {
    let dispatches = 0;
    const requests: unknown[] = [];
    const runtime = {
      adapterId: "adapter:test/builtin", providerProfileId: "provider:test/local", modelProfileId: MODEL,
      async *stream(request: {messages: unknown}) { requests.push(request.messages); yield {type: "delta" as const, delta: "continued answer"}; yield {type: "completed" as const}; },
      async dispose() {}
    };
    const engine = new AgentRunEngine({
      sessionService: f.session, repository: f.repository,
      modelRuntime: {assertAgentCompatible() {}, getRuntimeSnapshot() { return {selection: {adapterId: runtime.adapterId, providerProfileId: runtime.providerProfileId, modelProfileId: runtime.modelProfileId, providerDisplayName: "test", modelDisplayName: "test", model: "agent", protocol: "responses", source: "builtin", contextWindow: 32_000, maxOutputTokens: 2_000, autoCompactRatio: 0.9, supportsNativeToolCalls: true}, runtime} as never; }},
      retriever: {async retrieve() { return {snapshotVersion: "snapshot-v1", path: "exact", descriptors: [CAPABILITY], cards: buildCapabilityCards([CAPABILITY], 32_000), confident: true}; }},
      createDispatcher: () => ({async dispatch() { dispatches += 1; throw new Error("completed invocation must not replay"); }}),
      budgetLimits: {maxSteps: 4, maxToolCalls: 4, maxTotalTokens: 100_000, timeoutMs: 5_000}
    });
    const events = await collect(engine.continue());
    assert.equal(events[0]?.type, "agent.continued");
    assert.equal(events.at(-1)?.type, "agent.completed");
    assert.equal(dispatches, 0);
    assert.match(JSON.stringify(requests), /durable file contents/);
    const snapshot = await f.repository.readThread(f.session.activeThread.id);
    assert.equal(snapshot.turns.find((turn) => turn.id === f.turn.id)?.status, "completed");
    const checkpoints = snapshot.items.flatMap((item) => item.agent?.payload.kind === "checkpoint" ? [item.agent.payload] : []);
    assert.equal(checkpoints.at(-1)?.continuationGeneration, 1);
    assert.equal(checkpoints.at(-1)?.step, 0);
  } finally { await rm(f.root, {recursive: true, force: true}); }
});

async function collect(stream: AsyncGenerator<RuntimeEvent>): Promise<RuntimeEvent[]> {
  const events: RuntimeEvent[] = [];
  for await (const event of stream) events.push(event);
  return events;
}
