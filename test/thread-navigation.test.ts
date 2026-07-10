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
import type {
  AppConfig,
  ModelContextMessage,
  ThreadItem,
  ThreadRecord,
  TurnRecord
} from "../src/types.js";

const NOW = "2026-07-10T00:00:00.000Z";
const LATER = "2026-07-10T01:00:00.000Z";

class CapturingModelAdapter implements ModelAdapter {
  messages: ModelContextMessage[] = [];

  async *streamResponse(params: {
    config: AppConfig;
    apiKey: string;
    messages: ModelContextMessage[];
    signal?: AbortSignal;
  }): AsyncGenerator<ModelAdapterEvent> {
    this.messages = params.messages;
    yield {type: "delta", delta: "ok"};
    yield {type: "completed"};
  }
}

function thread(
  id: string,
  title: string,
  status: ThreadRecord["status"],
  cwd: string
): ThreadRecord {
  return {
    id,
    title,
    createdAt: NOW,
    updatedAt: NOW,
    model: DEFAULT_CONFIG.model,
    cwd,
    status
  };
}

function userItem(threadId: string, id: string, content: string): ThreadItem {
  return {
    id,
    threadId,
    turnId: `${id}_turn`,
    type: "user_message",
    role: "user",
    content,
    createdAt: NOW
  };
}

test("activating a stored thread leaves exactly one active record in one index transition", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-thread-activate-"));
  const storage = new JsonlStorageProvider(root);

  try {
    await storage.init();
    await storage.createThread(thread("thread_current", "Current", "active", root));
    await storage.createThread(thread("thread_target", "Target", "archived", root));

    const activated = await storage.activateThread("thread_target", LATER);
    const afterActivation = await storage.listThreads();

    assert.equal(activated?.id, "thread_target");
    assert.equal(afterActivation[0]?.id, "thread_target");
    assert.equal(afterActivation[0]?.updatedAt, LATER);
    assert.deepEqual(
      afterActivation.filter((entry) => entry.status === "active").map((entry) => entry.id),
      ["thread_target"]
    );
    assert.equal(
      afterActivation.find((entry) => entry.id === "thread_current")?.status,
      "archived"
    );

    const beforeMissing = JSON.stringify(afterActivation);
    assert.equal(await storage.activateThread("missing_thread", LATER), null);
    assert.equal(JSON.stringify(await storage.listThreads()), beforeMissing);
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("creating a new active thread archives the previous active thread in the same index update", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-thread-create-active-"));
  const storage = new JsonlStorageProvider(root);

  try {
    await storage.init();
    await storage.createThread(thread("thread_old", "Old", "active", root));
    await storage.createThread(thread("thread_new", "New", "active", root));

    const threads = await storage.listThreads();
    assert.deepEqual(
      threads.filter((entry) => entry.status === "active").map((entry) => entry.id),
      ["thread_new"]
    );
    assert.equal(threads.find((entry) => entry.id === "thread_old")?.status, "archived");
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("resuming a thread hydrates only its history, recovers stale turns, and scopes the next model call", async () => {
  const cwd = await mkdtemp(join(tmpdir(), "minimax-thread-resume-"));
  const stateRoot = join(cwd, ".mini-codex");
  const storage = new JsonlStorageProvider(stateRoot);
  const configManager = new ConfigManager(stateRoot);
  const secretStore = new SecretStore(stateRoot, {
    userConfigDir: join(stateRoot, "user-config"),
    keytar: null
  });
  const modelAdapter = new CapturingModelAdapter();
  const staleTargetTurn: TurnRecord = {
    id: "target_stale_turn",
    threadId: "thread_target",
    userInput: "unfinished target question",
    status: "running",
    startedAt: NOW
  };

  try {
    await configManager.save(DEFAULT_CONFIG);
    await secretStore.setApiKey("fake-test-key", "minimax-official");
    await storage.init();
    await storage.createThread(thread("thread_current", "Current", "active", cwd));
    await storage.createThread(thread("thread_target", "Target", "archived", cwd));
    await storage.appendItem(
      userItem("thread_current", "current_user", "history from original thread")
    );
    await storage.appendItem(
      userItem("thread_target", "target_user", "history from target thread")
    );
    await storage.appendTurn(staleTargetTurn);
    await storage.appendTurnDelta(
      "thread_target",
      staleTargetTurn.id,
      "unfinished target answer",
      NOW
    );

    const runtime = new AgentRuntime(
      cwd,
      stateRoot,
      configManager,
      secretStore,
      modelAdapter
    );
    const initialEvents = await runtime.init();
    const initialHistory = initialEvents.find((event) => event.type === "history.loaded");
    assert.equal(initialHistory?.type, "history.loaded");
    if (initialHistory?.type === "history.loaded") {
      assert.equal(initialHistory.items.some((item) => item.threadId === "thread_target"), false);
    }

    const resumeEvents = await runtime.resumeThread("thread_target");
    const loaded = resumeEvents.find((event) => event.type === "thread.loaded");
    const history = resumeEvents.find((event) => event.type === "history.loaded");

    assert.equal(loaded?.type, "thread.loaded");
    if (loaded?.type === "thread.loaded") {
      assert.equal(loaded.thread.id, "thread_target");
    }
    assert.equal(resumeEvents.some((event) => event.type === "turn.recovered"), true);
    assert.equal(history?.type, "history.loaded");
    if (history?.type === "history.loaded") {
      assert.equal(history.items.every((item) => item.threadId === "thread_target"), true);
      assert.equal(
        history.items.some((item) => item.content === "unfinished target answer"),
        true
      );
    }

    for await (const _event of runtime.submitUserInput("new target question")) {
      // Consume the model stream after switching threads.
    }
    const modelText = modelAdapter.messages.map((message) => message.content).join("\n");
    assert.equal(modelText.includes("history from target thread"), true);
    assert.equal(modelText.includes("history from original thread"), false);

    const targetTurns = await storage.readTurns("thread_target");
    assert.equal(
      targetTurns.find((entry) => entry.id === staleTargetTurn.id)?.status,
      "interrupted"
    );
    assert.deepEqual(
      (await runtime.listThreads())
        .filter((entry) => entry.status === "active")
        .map((entry) => entry.id),
      ["thread_target"]
    );

    await assert.rejects(runtime.resumeThread("missing_thread"), /不存在/);
    assert.deepEqual(
      (await runtime.listThreads())
        .filter((entry) => entry.status === "active")
        .map((entry) => entry.id),
      ["thread_target"]
    );
  } finally {
    await rm(cwd, {recursive: true, force: true});
  }
});

test("creating a new runtime thread starts empty and preserves the previous thread for resume", async () => {
  const cwd = await mkdtemp(join(tmpdir(), "minimax-runtime-new-thread-"));
  const stateRoot = join(cwd, ".mini-codex");
  const storage = new JsonlStorageProvider(stateRoot);
  const configManager = new ConfigManager(stateRoot);
  const secretStore = new SecretStore(stateRoot, {
    userConfigDir: join(stateRoot, "user-config"),
    keytar: null
  });
  const oldThread = thread("thread_old", "Old conversation", "active", cwd);

  try {
    await configManager.save(DEFAULT_CONFIG);
    await storage.init();
    await storage.createThread(oldThread);
    await storage.appendItem(userItem(oldThread.id, "old_user", "old conversation history"));

    const runtime = new AgentRuntime(cwd, stateRoot, configManager, secretStore);
    await runtime.init();
    const newEvents = await runtime.newThread();
    const loaded = newEvents.find((event) => event.type === "thread.loaded");
    const emptyHistory = newEvents.find((event) => event.type === "history.loaded");

    assert.equal(loaded?.type, "thread.loaded");
    if (loaded?.type === "thread.loaded") {
      assert.notEqual(loaded.thread.id, oldThread.id);
      assert.equal(loaded.thread.status, "active");
    }
    assert.equal(emptyHistory?.type, "history.loaded");
    if (emptyHistory?.type === "history.loaded") {
      assert.deepEqual(emptyHistory.items, []);
    }

    const threadsAfterNew = await runtime.listThreads();
    assert.equal(threadsAfterNew.length, 2);
    assert.equal(threadsAfterNew.find((entry) => entry.id === oldThread.id)?.status, "archived");
    assert.equal(threadsAfterNew.filter((entry) => entry.status === "active").length, 1);

    const resumeEvents = await runtime.resumeThread(oldThread.id);
    const oldHistory = resumeEvents.find((event) => event.type === "history.loaded");
    assert.equal(oldHistory?.type, "history.loaded");
    if (oldHistory?.type === "history.loaded") {
      assert.equal(oldHistory.items.some((item) => item.content === "old conversation history"), true);
    }
  } finally {
    await rm(cwd, {recursive: true, force: true});
  }
});
