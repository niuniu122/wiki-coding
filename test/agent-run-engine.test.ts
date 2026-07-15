import assert from "node:assert/strict";
import {mkdtemp, rm} from "node:fs/promises";
import {tmpdir} from "node:os";
import {join} from "node:path";
import test from "node:test";
import type {CapabilityDispatchResult, CapabilityInvocationRecorder} from "../src/capabilities/capability-dispatcher.js";
import {parseCapabilityManifest} from "../src/capabilities/capability-manifest.js";
import {buildCapabilityCards} from "../src/capabilities/search/capability-card.js";
import type {ModelAdapterEvent} from "../src/runtime/model-adapter.js";
import {AgentRunEngine, type AgentCapabilityDispatcherFactory} from "../src/runtime/agent-run-engine.js";
import {SessionService} from "../src/runtime/session-service.js";
import {JsonlStorageProvider} from "../src/storage/jsonl-storage.js";
import type {RuntimeEvent} from "../src/protocol.js";

const CAPABILITY = parseCapabilityManifest({
  schemaVersion: 1,
  id: "capability:test/read-file",
  name: "Read file",
  description: "Read a local workspace file",
  safetyClass: "workspace_read",
  execution: {kind: "workspace_read", operation: "read_file"}
}, {kind: "minimax", scope: "builtin", root: "builtin", file: "read-file.json"});

const SELECTION = {
  adapterId: "adapter:test/builtin",
  providerProfileId: "provider:test/local",
  modelProfileId: "model:test/local/agent",
  providerDisplayName: "Test",
  modelDisplayName: "Test Agent",
  model: "agent",
  protocol: "responses",
  source: "builtin",
  contextWindow: 32_000,
  maxOutputTokens: 2_000,
  autoCompactRatio: 0.9,
  supportsNativeToolCalls: true
} as const;

function retrieval(confident = true, hasCandidate = true) {
  const descriptors = hasCandidate ? [CAPABILITY] : [];
  return {
    snapshotVersion: "snapshot-v1",
    path: hasCandidate ? "exact" as const : "none" as const,
    descriptors,
    cards: buildCapabilityCards(descriptors, 32_000),
    fallbackReason: "embedding_unavailable" as const,
    confident
  };
}

function toolCall(id = CAPABILITY.id): ModelAdapterEvent[] {
  return [
    {type: "tool_call", call: {callId: "invocation-1", name: "invoke_local_capability", argumentsJson: JSON.stringify({capabilityId: id, arguments: {path: "README.md"}})}},
    {type: "completed"}
  ];
}

async function fixture(responses: readonly (readonly ModelAdapterEvent[])[], options: {confident?: boolean; hasCandidate?: boolean; compatible?: boolean; budget?: {maxSteps: number; maxToolCalls: number; maxTotalTokens: number; timeoutMs: number}} = {}) {
  const root = await mkdtemp(join(tmpdir(), "agent-engine-"));
  const repository = new JsonlStorageProvider(root);
  const session = new SessionService(repository);
  await session.init("agent", root);
  let modelCalls = 0;
  let retrievalCalls = 0;
  let dispatchCalls = 0;
  const requests: {tools: unknown; messages: unknown}[] = [];
  const runtime = {
    adapterId: SELECTION.adapterId,
    providerProfileId: SELECTION.providerProfileId,
    modelProfileId: SELECTION.modelProfileId,
    async *stream(request: {tools?: unknown; messages: unknown}) {
      requests.push({tools: request.tools, messages: request.messages});
      const events = responses[modelCalls++] ?? [{type: "completed"} as ModelAdapterEvent];
      for (const event of events) yield event;
    },
    async dispose() {}
  };
  const dispatcherFactory: AgentCapabilityDispatcherFactory = (recorder: CapabilityInvocationRecorder) => ({
    async dispatch(invocation): Promise<CapabilityDispatchResult> {
      dispatchCalls += 1;
      await recorder.recordRequest(invocation, CAPABILITY);
      const result = {status: "succeeded", invocationId: invocation.invocationId, output: "file contents"} as const;
      await recorder.recordResult(invocation, result);
      return result;
    }
  });
  const engine = new AgentRunEngine({
    sessionService: session,
    repository,
    modelRuntime: {
      assertAgentCompatible() { if (options.compatible === false) throw new Error("unsupported"); },
      getRuntimeSnapshot() { return {selection: SELECTION, runtime} as never; }
    },
    retriever: {async retrieve() { retrievalCalls += 1; return retrieval(options.confident ?? true, options.hasCandidate ?? true); }},
    createDispatcher: dispatcherFactory,
    budgetLimits: options.budget ?? {maxSteps: 4, maxToolCalls: 4, maxTotalTokens: 100_000, timeoutMs: 5_000}
  });
  return {root, repository, session, engine, requests, counts: {get model() { return modelCalls; }, get retrieval() { return retrievalCalls; }, get dispatch() { return dispatchCalls; }}};
}

test("text-only Agent run retrieves once and makes one model request", async () => {
  const f = await fixture([[{type: "delta", delta: "done"}, {type: "completed"}]]);
  try {
    const events = await collect(f.engine.submit("inspect project"));
    assert.equal(events.at(-1)?.type, "agent.completed");
    assert.equal(f.counts.retrieval, 1);
    assert.equal(f.counts.model, 1);
    assert.equal(f.counts.dispatch, 0);
    assert.ok(Array.isArray(f.requests[0]?.tools));
    const snapshot = await f.repository.readThread(f.session.activeThread.id);
    assert.deepEqual(snapshot.items.flatMap((item) => item.agent ? [item.agent.payload.kind] : []), ["user", "checkpoint", "assistant", "final"]);
  } finally { await rm(f.root, {recursive: true, force: true}); }
});

test("single tool run performs retrieval -> model -> policy dispatcher -> model", async () => {
  const f = await fixture([toolCall(), [{type: "delta", delta: "final answer"}, {type: "completed"}]]);
  try {
    const events = await collect(f.engine.submit("read the readme"));
    assert.equal(events.at(-1)?.type, "agent.completed");
    assert.equal(f.counts.retrieval, 1);
    assert.equal(f.counts.model, 2);
    assert.equal(f.counts.dispatch, 1);
    assert.match(JSON.stringify(f.requests[1]?.messages), /file contents/);
    const followUpMessages = f.requests[1]?.messages as Array<Record<string, unknown>>;
    assert.deepEqual(followUpMessages.slice(-2), [
      {
        role: "assistant",
        content: "",
        toolCalls: [{
          callId: "invocation-1",
          name: "invoke_local_capability",
          argumentsJson: JSON.stringify({
            capabilityId: CAPABILITY.id,
            arguments: {path: "README.md"}
          })
        }]
      },
      {
        role: "tool",
        toolCallId: "invocation-1",
        content: `Local capability result (${CAPABILITY.id}, succeeded):\nfile contents`
      }
    ]);
    const snapshot = await f.repository.readThread(f.session.activeThread.id);
    assert.deepEqual(snapshot.items.flatMap((item) => item.agent ? [item.agent.payload.kind] : []), ["user", "checkpoint", "tool_request", "tool_result", "checkpoint", "assistant", "final"]);
  } finally { await rm(f.root, {recursive: true, force: true}); }
});

test("no candidate, low confidence and unsupported model stop without a Provider request", async () => {
  for (const options of [{hasCandidate: false}, {confident: false}, {compatible: false}]) {
    const f = await fixture([], options);
    try {
      const events = await collect(f.engine.submit("unknown work"));
      assert.equal(events.at(-1)?.type, "agent.stopped");
      assert.equal(f.counts.model, 0);
      assert.equal(f.counts.dispatch, 0);
    } finally { await rm(f.root, {recursive: true, force: true}); }
  }
});

test("model cannot invoke an un-retrieved capability and the dispatcher never sees it", async () => {
  const f = await fixture([toolCall("capability:invented/root")]);
  try {
    const events = await collect(f.engine.submit("read"));
    assert.equal(events.at(-1)?.type, "agent.stopped");
    assert.equal((events.at(-1) as Extract<RuntimeEvent, {type: "agent.stopped"}>).reason, "agent_failed");
    assert.equal(f.counts.dispatch, 0);
  } finally { await rm(f.root, {recursive: true, force: true}); }
});

test("step budget ends a repeated tool loop deterministically", async () => {
  const f = await fixture([toolCall(), toolCall()], {budget: {maxSteps: 1, maxToolCalls: 4, maxTotalTokens: 100_000, timeoutMs: 5_000}});
  try {
    const events = await collect(f.engine.submit("repeat"));
    assert.equal((events.at(-1) as Extract<RuntimeEvent, {type: "agent.stopped"}>).reason, "step_limit");
    assert.equal(f.counts.model, 1);
    assert.equal(f.counts.dispatch, 1);
  } finally { await rm(f.root, {recursive: true, force: true}); }
});

async function collect(stream: AsyncGenerator<RuntimeEvent>): Promise<RuntimeEvent[]> {
  const events: RuntimeEvent[] = [];
  for await (const event of stream) events.push(event);
  return events;
}
