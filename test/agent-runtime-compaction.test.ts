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
import type {AppConfig, ModelContextMessage, ThreadItem, ThreadRecord} from "../src/types.js";

const NOW = "2026-07-10T00:00:00.000Z";

class CapturingModelAdapter implements ModelAdapter {
  messages: ModelContextMessage[] = [];

  async *streamResponse(params: {
    config: AppConfig;
    apiKey: string;
    messages: ModelContextMessage[];
  }): AsyncGenerator<ModelAdapterEvent> {
    this.messages = params.messages;
    yield {type: "delta", delta: "ok"};
    yield {type: "completed"};
  }
}

function item(
  id: string,
  turnId: string,
  role: "user" | "assistant",
  content: string
): ThreadItem {
  return {
    id,
    threadId: "thread_1",
    turnId,
    type: role === "user" ? "user_message" : "assistant_message",
    role,
    content,
    createdAt: NOW
  };
}

test("automatic compaction rebuilds the model request from the persisted summary", async () => {
  const cwd = await mkdtemp(join(tmpdir(), "minimax-codex-compaction-"));
  const stateRoot = join(cwd, ".mini-codex");

  try {
    const configManager = new ConfigManager(stateRoot);
    const secretStore = new SecretStore(stateRoot, {
      userConfigDir: join(stateRoot, "user-config"),
      keytar: null
    });
    const storage = new JsonlStorageProvider(stateRoot);
    const modelAdapter = new CapturingModelAdapter();
    const config: AppConfig = {
      ...DEFAULT_CONFIG,
      modelProviders: {...DEFAULT_CONFIG.modelProviders},
      api: {...DEFAULT_CONFIG.api},
      storage: {...DEFAULT_CONFIG.storage},
      context: {
        ...DEFAULT_CONFIG.context,
        workingContextLimit: 2_000,
        maxCompletionTokens: 200,
        autoCompactRatio: 0.9
      }
    };
    const thread: ThreadRecord = {
      id: "thread_1",
      title: "Compaction test",
      createdAt: NOW,
      updatedAt: NOW,
      model: config.model,
      cwd,
      status: "active"
    };

    await configManager.save(config);
    await secretStore.setApiKey("fake-test-key", "minimax-official");
    await storage.init();
    await storage.createThread(thread);
    await storage.appendItem(
      item("old_user", "turn_old", "user", `old oversized user ${"x".repeat(8_000)}`)
    );
    await storage.appendItem(
      item("old_assistant", "turn_old", "assistant", `old oversized assistant ${"y".repeat(8_000)}`)
    );

    const runtime = new AgentRuntime(cwd, stateRoot, configManager, secretStore, modelAdapter);
    await runtime.init();

    const events = [];
    for await (const event of runtime.submitUserInput("current exact question")) {
      events.push(event);
    }

    const modelText = modelAdapter.messages.map((entry) => entry.content).join("\n");
    const summaries = await storage.readSummaries("thread_1");
    const compacted = events.find((event) => event.type === "compact.completed");
    const firstUsage = events.find((event) => event.type === "token.usage");

    assert.equal(modelText.includes("x".repeat(1_000)), false);
    assert.equal(modelText.includes("y".repeat(1_000)), false);
    assert.equal(modelText.includes("以下内容代表已覆盖的旧会话"), true);
    assert.equal(
      modelAdapter.messages.some(
        (entry) => entry.role === "user" && entry.content === "current exact question"
      ),
      true
    );
    assert.equal(summaries.length, 1);
    assert.equal(summaries[0]?.coveredThroughItemId, "old_assistant");
    assert.equal(compacted?.type, "compact.completed");
    assert.equal(firstUsage?.type, "token.usage");
    if (firstUsage?.type === "token.usage") {
      assert.equal(firstUsage.limit, 1_800);
    }
  } finally {
    await rm(cwd, {recursive: true, force: true});
  }
});

test("an oversized current input fails clearly after compaction without calling the model", async () => {
  const cwd = await mkdtemp(join(tmpdir(), "minimax-codex-input-limit-"));
  const stateRoot = join(cwd, ".mini-codex");

  try {
    const configManager = new ConfigManager(stateRoot);
    const secretStore = new SecretStore(stateRoot, {
      userConfigDir: join(stateRoot, "user-config"),
      keytar: null
    });
    const storage = new JsonlStorageProvider(stateRoot);
    const modelAdapter = new CapturingModelAdapter();
    const config: AppConfig = {
      ...DEFAULT_CONFIG,
      modelProviders: {...DEFAULT_CONFIG.modelProviders},
      api: {...DEFAULT_CONFIG.api},
      storage: {...DEFAULT_CONFIG.storage},
      context: {
        ...DEFAULT_CONFIG.context,
        workingContextLimit: 2_000,
        maxCompletionTokens: 200,
        autoCompactRatio: 0.9
      }
    };
    const thread: ThreadRecord = {
      id: "thread_1",
      title: "Input limit test",
      createdAt: NOW,
      updatedAt: NOW,
      model: config.model,
      cwd,
      status: "active"
    };

    await configManager.save(config);
    await secretStore.setApiKey("fake-test-key", "minimax-official");
    await storage.init();
    await storage.createThread(thread);
    await storage.appendItem(
      item("old_user", "turn_old", "user", `old oversized user ${"x".repeat(8_000)}`)
    );
    await storage.appendItem(
      item("old_assistant", "turn_old", "assistant", `old oversized assistant ${"y".repeat(8_000)}`)
    );

    const runtime = new AgentRuntime(cwd, stateRoot, configManager, secretStore, modelAdapter);
    await runtime.init();

    const events = [];
    for await (const event of runtime.submitUserInput(`current oversized ${"z".repeat(10_000)}`)) {
      events.push(event);
    }

    const error = events.find((event) => event.type === "error");
    assert.equal(modelAdapter.messages.length, 0);
    assert.equal(error?.type, "error");
    if (error?.type === "error") {
      assert.match(error.message, /压缩后仍超过上下文安全上限/);
    }
  } finally {
    await rm(cwd, {recursive: true, force: true});
  }
});
