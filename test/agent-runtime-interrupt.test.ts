import assert from "node:assert/strict";
import {mkdtemp, rm} from "node:fs/promises";
import {tmpdir} from "node:os";
import {join} from "node:path";
import test from "node:test";
import {ConfigManager, DEFAULT_CONFIG} from "../src/config/config-manager.js";
import {SecretStore} from "../src/config/secret-store.js";
import type {RuntimeEvent} from "../src/protocol.js";
import {AgentRuntime} from "../src/runtime/agent-runtime.js";
import type {ModelAdapter, ModelAdapterEvent} from "../src/runtime/model-adapter.js";
import {JsonlStorageProvider} from "../src/storage/jsonl-storage.js";
import type {AppConfig, ModelContextMessage} from "../src/types.js";

class InterruptibleModelAdapter implements ModelAdapter {
  private markStarted!: () => void;
  readonly started = new Promise<void>((resolve) => {
    this.markStarted = resolve;
  });

  async *streamResponse(params: {
    config: AppConfig;
    apiKey: string;
    messages: ModelContextMessage[];
    signal?: AbortSignal;
  }): AsyncGenerator<ModelAdapterEvent> {
    yield {type: "delta", delta: "partial before interrupt"};
    this.markStarted();

    await new Promise<void>((_resolve, reject) => {
      const fallback = setTimeout(
        () => reject(new Error("adapter did not receive an active abort signal")),
        200
      );
      const rejectAsAborted = (): void => {
        clearTimeout(fallback);
        const error = new Error("request aborted");
        error.name = "AbortError";
        reject(error);
      };
      if (params.signal?.aborted) {
        rejectAsAborted();
      } else {
        params.signal?.addEventListener("abort", rejectAsAborted, {once: true});
      }
    });
  }
}

test("interrupting with no active model request is a harmless no-op", async () => {
  const cwd = await mkdtemp(join(tmpdir(), "minimax-runtime-interrupt-idle-"));
  const stateRoot = join(cwd, ".mini-codex");
  const configManager = new ConfigManager(stateRoot);
  const secretStore = new SecretStore(stateRoot, {
    userConfigDir: join(stateRoot, "user-config"),
    keytar: null
  });

  try {
    await configManager.save(DEFAULT_CONFIG);
    const runtime = new AgentRuntime(cwd, stateRoot, configManager, secretStore);
    await runtime.init();

    const event = runtime.interruptCurrentTurn();

    assert.deepEqual(event, {type: "turn.interrupt.ignored", reason: "no_active_request"});
  } finally {
    await rm(cwd, {recursive: true, force: true});
  }
});

test("interrupting an active request aborts the adapter and persists an interrupted turn", async () => {
  const cwd = await mkdtemp(join(tmpdir(), "minimax-runtime-interrupt-active-"));
  const stateRoot = join(cwd, ".mini-codex");
  const configManager = new ConfigManager(stateRoot);
  const secretStore = new SecretStore(stateRoot, {
    userConfigDir: join(stateRoot, "user-config"),
    keytar: null
  });
  const storage = new JsonlStorageProvider(stateRoot);
  const modelAdapter = new InterruptibleModelAdapter();

  try {
    await configManager.save(DEFAULT_CONFIG);
    await secretStore.setApiKey("fake-test-key", "minimax-official");
    const runtime = new AgentRuntime(
      cwd,
      stateRoot,
      configManager,
      secretStore,
      modelAdapter
    );
    await runtime.init();

    const events: RuntimeEvent[] = [];
    const consume = (async (): Promise<void> => {
      for await (const event of runtime.submitUserInput("cancel this response")) {
        events.push(event);
      }
    })();

    await modelAdapter.started;
    const requested = runtime.interruptCurrentTurn();
    await consume;

    await storage.init();
    const activeThread = (await storage.listThreads()).find((thread) => thread.status === "active");
    assert.ok(activeThread);
    const turns = await storage.readTurns(activeThread.id);
    const items = await storage.readThreadItems(activeThread.id);

    assert.equal(requested.type, "turn.interrupt.requested");
    assert.equal(events.some((event) => event.type === "turn.interrupted"), true);
    assert.equal(events.some((event) => event.type === "error"), false);
    const started = events.find((event) => event.type === "turn.started");
    const delta = events.find((event) => event.type === "assistant.delta");
    const interrupted = events.find((event) => event.type === "turn.interrupted");
    assert.equal(started?.type, "turn.started");
    assert.equal(delta?.type, "assistant.delta");
    assert.equal(interrupted?.type, "turn.interrupted");
    if (
      started?.type === "turn.started" &&
      delta?.type === "assistant.delta" &&
      interrupted?.type === "turn.interrupted"
    ) {
      assert.equal(started.input, "cancel this response");
      assert.equal(delta.turnId, started.turnId);
      assert.equal(interrupted.turnId, started.turnId);
    }
    assert.equal(turns[0]?.status, "interrupted");
    assert.equal(
      items.some(
        (item) =>
          item.type === "assistant_message" &&
          item.content === "partial before interrupt" &&
          item.metadata?.partial === true &&
          item.metadata?.interrupted === true
      ),
      true
    );
    assert.deepEqual(runtime.interruptCurrentTurn(), {
      type: "turn.interrupt.ignored",
      reason: "no_active_request"
    });
  } finally {
    await rm(cwd, {recursive: true, force: true});
  }
});
