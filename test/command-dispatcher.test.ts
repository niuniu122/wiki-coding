import assert from "node:assert/strict";
import test from "node:test";
import type {Command, RuntimeEvent} from "../src/protocol.js";
import {
  CommandDispatcher,
  type RuntimePort
} from "../src/runtime/command-dispatcher.js";
import type {ThreadRecord} from "../src/types.js";

const THREAD: ThreadRecord = {
  id: "thread_1",
  title: "Dispatcher test",
  createdAt: "2026-07-10T00:00:00.000Z",
  updatedAt: "2026-07-10T00:00:00.000Z",
  model: "MiniMax-M2.1",
  cwd: "C:/workspace",
  status: "active"
};

class FakeRuntime implements RuntimePort {
  calls: string[] = [];
  savedApiKey = "";
  failResume = false;
  failSetApiKey = false;

  async init(): Promise<RuntimeEvent[]> {
    this.calls.push("init");
    return [{type: "thread.loaded", thread: THREAD}, {type: "history.loaded", items: []}];
  }

  async hasApiKey(): Promise<boolean> {
    this.calls.push("hasApiKey");
    return false;
  }

  getProviderSummary(): string {
    this.calls.push("getProviderSummary");
    return "minimax-official | chat_completions | model=MiniMax-M2.1";
  }

  async setApiKey(apiKey: string): Promise<"keychain" | "user-file"> {
    this.calls.push("setApiKey");
    this.savedApiKey = apiKey;
    if (this.failSetApiKey) {
      throw new Error(`could not save ${apiKey}`);
    }
    return "user-file";
  }

  listProviderSummaries(): string[] {
    this.calls.push("listProviderSummaries");
    return ["minimax-official (active)", "hashsight (available)"];
  }

  async switchProvider(providerId: string): Promise<string> {
    this.calls.push(`switchProvider:${providerId}`);
    return `${providerId} selected`;
  }

  async newThread(): Promise<RuntimeEvent[]> {
    this.calls.push("newThread");
    return [{type: "thread.loaded", thread: THREAD}, {type: "history.loaded", items: []}];
  }

  async listThreads(): Promise<ThreadRecord[]> {
    this.calls.push("listThreads");
    return [THREAD];
  }

  async resumeThread(threadId: string): Promise<RuntimeEvent[]> {
    this.calls.push(`resumeThread:${threadId}`);
    if (this.failResume) {
      throw new Error("resume failed");
    }
    return [{type: "thread.loaded", thread: {...THREAD, id: threadId}}];
  }

  interruptCurrentTurn(): RuntimeEvent {
    this.calls.push("interruptCurrentTurn");
    return {type: "turn.interrupt.ignored", reason: "no_active_request"};
  }

  async compact(_reason: "manual"): Promise<RuntimeEvent[]> {
    this.calls.push("compact");
    return [
      {type: "compact.started", reason: "manual"},
      {
        type: "compact.completed",
        summary: "",
        compacted: false,
        beforeTokens: 1,
        afterTokens: 1
      }
    ];
  }

  async *submitUserInput(input: string): AsyncGenerator<RuntimeEvent> {
    this.calls.push(`submitUserInput:${input}`);
    yield {type: "turn.started", turnId: "turn_1", input};
    yield {type: "assistant.delta", turnId: "turn_1", delta: "ok"};
  }
}

async function collect(
  dispatcher: CommandDispatcher,
  command: Command
): Promise<RuntimeEvent[]> {
  const events: RuntimeEvent[] = [];
  for await (const event of dispatcher.dispatch(command)) {
    events.push(event);
  }
  return events;
}

test("dispatcher initializes the runtime and publishes a ready event", async () => {
  const runtime = new FakeRuntime();
  const dispatcher = new CommandDispatcher(runtime);

  const events = await dispatcher.init();

  assert.equal(events.at(-1)?.type, "runtime.ready");
  const ready = events.at(-1);
  if (ready?.type === "runtime.ready") {
    assert.equal(ready.hasApiKey, false);
    assert.equal(ready.providerSummary.includes("minimax-official"), true);
    assert.equal(ready.recoveredTurns, 0);
  }
});

test("dispatcher routes every Command and never echoes an API key", async () => {
  const runtime = new FakeRuntime();
  const dispatcher = new CommandDispatcher(runtime);
  const secret = "secret-key-must-not-appear";

  const results = await Promise.all([
    collect(dispatcher, {type: "thread.new"}),
    collect(dispatcher, {type: "thread.list"}),
    collect(dispatcher, {type: "thread.resume", threadId: "thread_2"}),
    collect(dispatcher, {type: "turn.submit", input: "hello"}),
    collect(dispatcher, {type: "turn.interrupt"}),
    collect(dispatcher, {type: "compact.manual"}),
    collect(dispatcher, {type: "config.api_key.request"}),
    collect(dispatcher, {type: "config.api_key.set", apiKey: secret}),
    collect(dispatcher, {type: "provider.list"}),
    collect(dispatcher, {type: "provider.switch", providerId: "hashsight"}),
    collect(dispatcher, {type: "trace.toggle"}),
    collect(dispatcher, {type: "app.exit"})
  ]);

  assert.equal(runtime.savedApiKey, secret);
  assert.equal(JSON.stringify(results).includes(secret), false);
  assert.equal(results[1]?.[0]?.type, "thread.listed");
  assert.equal(results[3]?.some((event) => event.type === "assistant.delta"), true);
  assert.equal(results[6]?.[0]?.type, "config.api_key.requested");
  assert.equal(results[7]?.[0]?.type, "config.api_key.saved");
  assert.equal(results[8]?.[0]?.type, "provider.listed");
  assert.equal(results[9]?.[0]?.type, "provider.changed");
  assert.equal(results[10]?.[0]?.type, "trace.toggle.requested");
  assert.equal(results[11]?.[0]?.type, "app.exit.requested");
});

test("dispatcher converts command failures into error events", async () => {
  const runtime = new FakeRuntime();
  runtime.failResume = true;
  const dispatcher = new CommandDispatcher(runtime);

  const events = await collect(dispatcher, {
    type: "thread.resume",
    threadId: "missing"
  });

  assert.deepEqual(events, [{type: "error", message: "resume failed"}]);
});

test("dispatcher redacts an API key even when secret storage echoes it in an error", async () => {
  const runtime = new FakeRuntime();
  runtime.failSetApiKey = true;
  const dispatcher = new CommandDispatcher(runtime);
  const secret = "super-secret-key";

  const events = await collect(dispatcher, {
    type: "config.api_key.set",
    apiKey: secret
  });

  assert.equal(JSON.stringify(events).includes(secret), false);
  assert.deepEqual(events, [
    {type: "error", message: "could not save [REDACTED]"}
  ]);
});
