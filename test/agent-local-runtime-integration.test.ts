import assert from "node:assert/strict";
import {mkdtemp, rm, writeFile} from "node:fs/promises";
import {tmpdir} from "node:os";
import {join} from "node:path";
import test from "node:test";
import {LocalCapabilityRuntime} from "../src/capabilities/local-capability-runtime.js";
import {resolveAgentFeatureFlags} from "../src/config/feature-flags.js";
import {AgentRunEngine} from "../src/runtime/agent-run-engine.js";
import {SessionService} from "../src/runtime/session-service.js";
import {JsonlStorageProvider} from "../src/storage/jsonl-storage.js";

test("explicit Agent route can retrieve and execute the real bounded local read capability end to end", async () => {
  const root = await mkdtemp(join(tmpdir(), "agent-local-runtime-"));
  try {
    await writeFile(join(root, "README.md"), "bounded local evidence");
    const stateRoot = join(root, ".mini-codex");
    const repository = new JsonlStorageProvider(stateRoot);
    const session = new SessionService(repository); await session.init("agent", root);
    const capabilities = new LocalCapabilityRuntime({workspaceRoot: root, stateRoot, userConfigRoot: join(root, "user"), getPermissionMode: () => "confirm", env: {}, homeDir: join(root, "home")});
    await capabilities.initialize(resolveAgentFeatureFlags({capabilityCatalog: true, capabilityEmbedding: false, agentExecution: true, agentDefaultRoute: false}, {releaseGatePassed: true}));
    let calls = 0;
    const runtime = {
      adapterId: "adapter:test/builtin", providerProfileId: "provider:test/local", modelProfileId: "model:test/local/agent",
      async *stream() {
        calls += 1;
        if (calls === 1) {
          yield {type: "tool_call" as const, call: {callId: "invocation-read-1", name: "invoke_local_capability", argumentsJson: JSON.stringify({capabilityId: "capability:minimax/read-file", arguments: {path: "README.md"}})}};
        } else {
          yield {type: "delta" as const, delta: "I found bounded local evidence."};
        }
        yield {type: "completed" as const};
      }, async dispose() {}
    };
    const engine = new AgentRunEngine({
      sessionService: session, repository, retriever: capabilities, createDispatcher: (recorder) => capabilities.createDispatcher(recorder),
      modelRuntime: {assertAgentCompatible() {}, getRuntimeSnapshot() { return {selection: {adapterId: runtime.adapterId, providerProfileId: runtime.providerProfileId, modelProfileId: runtime.modelProfileId, providerDisplayName: "test", modelDisplayName: "test", model: "agent", protocol: "responses", source: "builtin", contextWindow: 32_000, maxOutputTokens: 2_000, autoCompactRatio: 0.9, supportsNativeToolCalls: true}, runtime} as never; }}
    });
    const events = [];
    for await (const event of engine.submit("read file")) events.push(event);
    assert.equal(events.at(-1)?.type, "agent.completed");
    assert.equal(calls, 2);
    const snapshot = await repository.readThread(session.activeThread.id);
    const result = snapshot.items.find((item) => item.agent?.payload.kind === "tool_result");
    assert.equal(result?.agent?.payload.kind === "tool_result" && result.agent.payload.output, "bounded local evidence");
  } finally { await rm(root, {recursive: true, force: true}); }
});
