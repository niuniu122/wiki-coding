import assert from "node:assert/strict";
import {mkdtemp, rm} from "node:fs/promises";
import {tmpdir} from "node:os";
import {join} from "node:path";
import test from "node:test";
import {ConfigManager, DEFAULT_CONFIG} from "../src/config/config-manager.js";
import {SecretStore} from "../src/config/secret-store.js";
import {AgentRuntime} from "../src/runtime/agent-runtime.js";
import type {ModelAdapter, ModelAdapterEvent} from "../src/runtime/model-adapter.js";
import {JsonlStorageProvider} from "../src/storage/jsonl-storage.js";
import type {AppConfig, ModelContextMessage, ThreadItem, ThreadRecord, TurnRecord} from "../src/types.js";

const NOW = "2026-07-10T00:00:00.000Z";

class TwoChunkModelAdapter implements ModelAdapter {
  async *streamResponse(_params: {
    config: AppConfig;
    apiKey: string;
    messages: ModelContextMessage[];
  }): AsyncGenerator<ModelAdapterEvent> {
    yield {type: "delta", delta: "hello"};
    yield {type: "delta", delta: " world"};
    yield {type: "completed"};
  }
}

class FailingAfterDeltaModelAdapter implements ModelAdapter {
  async *streamResponse(_params: {
    config: AppConfig;
    apiKey: string;
    messages: ModelContextMessage[];
  }): AsyncGenerator<ModelAdapterEvent> {
    yield {type: "delta", delta: "recoverable partial"};
    throw new Error("simulated provider failure");
  }
}

function thread(cwd: string, model: string): ThreadRecord {
  return {
    id: "thread_1",
    title: "Recovery test",
    createdAt: NOW,
    updatedAt: NOW,
    model,
    cwd,
    status: "active"
  };
}

function userItem(content: string): ThreadItem {
  return {
    id: "user_1",
    threadId: "thread_1",
    turnId: "turn_1",
    type: "user_message",
    role: "user",
    content,
    createdAt: NOW
  };
}

test("startup recovers a stale running turn and hydrates its saved partial reply once", async () => {
  const cwd = await mkdtemp(join(tmpdir(), "minimax-runtime-recovery-"));
  const stateRoot = join(cwd, ".mini-codex");
  const configManager = new ConfigManager(stateRoot);
  const secretStore = new SecretStore(stateRoot, {
    userConfigDir: join(stateRoot, "user-config"),
    keytar: null
  });
  const storage = new JsonlStorageProvider(stateRoot);
  const config: AppConfig = {
    ...DEFAULT_CONFIG,
    modelProviders: {...DEFAULT_CONFIG.modelProviders},
    api: {...DEFAULT_CONFIG.api},
    storage: {...DEFAULT_CONFIG.storage},
    context: {...DEFAULT_CONFIG.context}
  };
  const staleTurn: TurnRecord = {
    id: "turn_1",
    threadId: "thread_1",
    userInput: "saved question",
    status: "running",
    startedAt: NOW
  };

  try {
    await configManager.save(config);
    await storage.init();
    await storage.createThread(thread(cwd, config.model));
    await storage.appendTurn(staleTurn);
    await storage.appendTurnDelta("thread_1", "turn_1", "saved partial reply", NOW);
    await storage.appendItem(userItem("saved question"));

    const firstRuntime = new AgentRuntime(cwd, stateRoot, configManager, secretStore);
    const firstEvents = await firstRuntime.init();
    const recovered = firstEvents.find((event) => event.type === "turn.recovered");
    const history = firstEvents.find((event) => event.type === "history.loaded");
    const recoveredTurns = await storage.readTurns("thread_1");

    assert.equal(recovered?.type, "turn.recovered");
    assert.equal(recoveredTurns[0]?.status, "interrupted");
    assert.equal(history?.type, "history.loaded");
    if (history?.type === "history.loaded") {
      assert.equal(
        history.items.some(
          (item) =>
            item.type === "assistant_message" &&
            item.content === "saved partial reply" &&
            item.metadata?.partial === true &&
            item.metadata?.interrupted === true
        ),
        true
      );
    }

    const secondRuntime = new AgentRuntime(cwd, stateRoot, configManager, secretStore);
    const secondEvents = await secondRuntime.init();
    const itemsAfterSecondStart = await storage.readThreadItems("thread_1");

    assert.equal(secondEvents.some((event) => event.type === "turn.recovered"), false);
    assert.equal(
      itemsAfterSecondStart.filter(
        (item) => item.type === "assistant_message" && item.metadata?.interrupted === true
      ).length,
      1
    );
  } finally {
    await rm(cwd, {recursive: true, force: true});
  }
});

test("a normal streamed response persists a completed turn and its draft", async () => {
  const cwd = await mkdtemp(join(tmpdir(), "minimax-runtime-turn-"));
  const stateRoot = join(cwd, ".mini-codex");
  const configManager = new ConfigManager(stateRoot);
  const secretStore = new SecretStore(stateRoot, {
    userConfigDir: join(stateRoot, "user-config"),
    keytar: null
  });
  const storage = new JsonlStorageProvider(stateRoot);

  try {
    await configManager.save(DEFAULT_CONFIG);
    await secretStore.setApiKey("fake-test-key", "minimax-official");
    const runtime = new AgentRuntime(
      cwd,
      stateRoot,
      configManager,
      secretStore,
      new TwoChunkModelAdapter()
    );
    const initEvents = await runtime.init();
    const loaded = initEvents.find((event) => event.type === "thread.loaded");
    assert.equal(loaded?.type, "thread.loaded");

    for await (const _event of runtime.submitUserInput("new question")) {
      // Consume the stream so the Turn reaches its terminal state.
    }

    await storage.init();
    const threads = await storage.listThreads();
    const activeThread = threads.find((entry) => entry.status === "active");
    assert.ok(activeThread);
    const turns = await storage.readTurns(activeThread.id);

    assert.equal(turns.length, 1);
    assert.equal(turns[0]?.status, "completed");
    assert.equal(turns[0]?.assistantDraft, "hello world");
  } finally {
    await rm(cwd, {recursive: true, force: true});
  }
});

test("a failed streamed response keeps its partial assistant output for the next startup", async () => {
  const cwd = await mkdtemp(join(tmpdir(), "minimax-runtime-failed-draft-"));
  const stateRoot = join(cwd, ".mini-codex");
  const configManager = new ConfigManager(stateRoot);
  const secretStore = new SecretStore(stateRoot, {
    userConfigDir: join(stateRoot, "user-config"),
    keytar: null
  });
  const storage = new JsonlStorageProvider(stateRoot);

  try {
    await configManager.save(DEFAULT_CONFIG);
    await secretStore.setApiKey("fake-test-key", "minimax-official");
    const runtime = new AgentRuntime(
      cwd,
      stateRoot,
      configManager,
      secretStore,
      new FailingAfterDeltaModelAdapter()
    );
    await runtime.init();
    for await (const _event of runtime.submitUserInput("question before failure")) {
      // Consume the stream and its error event.
    }

    await storage.init();
    const activeThread = (await storage.listThreads()).find((entry) => entry.status === "active");
    assert.ok(activeThread);
    const items = await storage.readThreadItems(activeThread.id);
    const turns = await storage.readTurns(activeThread.id);

    assert.equal(turns[0]?.status, "failed");
    assert.equal(
      items.some(
        (item) =>
          item.type === "assistant_message" &&
          item.content === "recoverable partial" &&
          item.metadata?.partial === true &&
          item.metadata?.failed === true
      ),
      true
    );
  } finally {
    await rm(cwd, {recursive: true, force: true});
  }
});
