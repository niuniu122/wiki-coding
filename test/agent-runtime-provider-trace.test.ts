import assert from "node:assert/strict";
import {mkdtemp, rm} from "node:fs/promises";
import {tmpdir} from "node:os";
import {join} from "node:path";
import test from "node:test";
import {ConfigManager, DEFAULT_CONFIG} from "../src/config/config-manager.js";
import {SecretStore} from "../src/config/secret-store.js";
import {AgentRuntime} from "../src/runtime/agent-runtime.js";
import type {ModelAdapter, ModelAdapterEvent} from "../src/runtime/model-adapter.js";
import type {AppConfig, ModelContextMessage} from "../src/types.js";

class DiagnosticModelAdapter implements ModelAdapter {
  async *streamResponse(_params: {
    config: AppConfig;
    apiKey: string;
    messages: ModelContextMessage[];
  }): AsyncGenerator<ModelAdapterEvent> {
    yield {
      type: "diagnostic",
      code: "provider.reasoning.filtered",
      facts: {
        providerId: "minimax-official",
        hiddenCharacters: 9,
        rawReasoning: "RUNTIME_MUST_DROP_THIS"
      }
    };
    yield {type: "delta", delta: "safe answer"};
    yield {type: "completed"};
  }
}

test("runtime persists provider diagnostics through the safe trace boundary", async () => {
  const cwd = await mkdtemp(join(tmpdir(), "minimax-provider-trace-"));
  const stateRoot = join(cwd, ".mini-codex");
  const configManager = new ConfigManager(stateRoot);
  const secretStore = new SecretStore(stateRoot, {
    userConfigDir: join(stateRoot, "user-config"),
    keytar: null
  });

  try {
    await configManager.save(DEFAULT_CONFIG);
    await secretStore.setApiKey("fake-test-key", "minimax-official");
    const runtime = new AgentRuntime(
      cwd,
      stateRoot,
      configManager,
      secretStore,
      new DiagnosticModelAdapter()
    );
    await runtime.init();
    const events = [];
    for await (const event of runtime.submitUserInput("hello")) {
      events.push(event);
    }

    const trace = events.find(
      (event) =>
        event.type === "trace.event" && event.event.code === "provider.reasoning.filtered"
    );
    assert.equal(trace?.type, "trace.event");
    assert.equal(JSON.stringify(trace).includes("RUNTIME_MUST_DROP_THIS"), false);
  } finally {
    await rm(cwd, {recursive: true, force: true});
  }
});
