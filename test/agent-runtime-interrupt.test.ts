import assert from "node:assert/strict";
import {mkdtemp, rm} from "node:fs/promises";
import {tmpdir} from "node:os";
import {join} from "node:path";
import test from "node:test";
import {parseCapabilityManifest} from "../src/capabilities/capability-manifest.js";
import {buildCapabilityCards} from "../src/capabilities/search/capability-card.js";
import {AgentRunEngine} from "../src/runtime/agent-run-engine.js";
import {SessionService} from "../src/runtime/session-service.js";
import {JsonlStorageProvider} from "../src/storage/jsonl-storage.js";

test("interrupt aborts the active Agent model request and leaves a durable interrupted Turn", async () => {
  const root = await mkdtemp(join(tmpdir(), "agent-interrupt-"));
  try {
    const repository = new JsonlStorageProvider(root);
    const session = new SessionService(repository); await session.init("agent", root);
    const capability = parseCapabilityManifest({schemaVersion: 1, id: "capability:test/read", name: "read", description: "read", safetyClass: "workspace_read", execution: {kind: "workspace_read", operation: "read_file"}}, {kind: "minimax", scope: "builtin", root: "builtin", file: "read.json"});
    const runtime = {
      adapterId: "adapter:test/builtin", providerProfileId: "provider:test/local", modelProfileId: "model:test/local/agent",
      async *stream(request: {signal?: AbortSignal}) {
        await new Promise<void>((resolve) => request.signal?.addEventListener("abort", () => resolve(), {once: true}));
        throw Object.assign(new Error("aborted"), {name: "AbortError"});
        yield {type: "completed" as const};
      }, async dispose() {}
    };
    const engine = new AgentRunEngine({
      sessionService: session, repository,
      modelRuntime: {assertAgentCompatible() {}, getRuntimeSnapshot() { return {selection: {adapterId: runtime.adapterId, providerProfileId: runtime.providerProfileId, modelProfileId: runtime.modelProfileId, providerDisplayName: "test", modelDisplayName: "test", model: "agent", protocol: "responses", source: "builtin", contextWindow: 32_000, maxOutputTokens: 2_000, autoCompactRatio: 0.9, supportsNativeToolCalls: true}, runtime} as never; }},
      retriever: {async retrieve() { return {snapshotVersion: "v1", path: "exact", descriptors: [capability], cards: buildCapabilityCards([capability], 32_000), confident: true}; }},
      createDispatcher: () => ({async dispatch() { throw new Error("unused"); }}),
      budgetLimits: {maxSteps: 2, maxToolCalls: 2, maxTotalTokens: 100_000, timeoutMs: 5_000}
    });
    const iterator = engine.submit("inspect");
    assert.equal((await iterator.next()).value?.type, "agent.started");
    assert.equal((await iterator.next()).value?.type, "agent.retrieval.started");
    assert.equal((await iterator.next()).value?.type, "agent.retrieval.completed");
    assert.equal((await iterator.next()).value?.type, "agent.model.started");
    const pending = iterator.next();
    assert.equal(engine.interrupt().type, "turn.interrupt.requested");
    assert.equal((await pending).value?.type, "agent.stopped");
    await iterator.return(undefined);
    const snapshot = await repository.readThread(session.activeThread.id);
    assert.equal(snapshot.turns.at(-1)?.status, "interrupted");

    const paused = engine.submit("shutdown while paused");
    await paused.next();
    await paused.next();
    await paused.next();
    assert.equal((await paused.next()).value?.type, "agent.model.started");
    await engine.shutdown();
    await paused.return(undefined);
    const afterShutdown = await repository.readThread(session.activeThread.id);
    assert.equal(afterShutdown.turns.find((turn) => turn.userInput === "shutdown while paused")?.status, "interrupted");
  } finally { await rm(root, {recursive: true, force: true}); }
});
