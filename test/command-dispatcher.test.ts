import assert from "node:assert/strict";
import {readFile} from "node:fs/promises";
import {fileURLToPath} from "node:url";
import test from "node:test";
import type {Command, RuntimeEvent} from "../src/protocol.js";
import {CommandDispatcher} from "../src/runtime/command-dispatcher.js";
import type {RuntimeApplication} from "../src/runtime/runtime-application.js";

const READY: RuntimeEvent = {
  type: "runtime.ready",
  hasApiKey: false,
  providerSummary: "minimax-official | responses | model=MiniMax-M3",
  recoveredTurns: 0
};

class FakeApplication implements RuntimeApplication {
  readonly calls: string[] = [];
  failInit = false;

  async init(): Promise<RuntimeEvent[]> {
    this.calls.push("init");
    if (this.failInit) {
      throw new Error("init failed");
    }
    return [READY];
  }

  async *dispatch(command: Command): AsyncGenerator<RuntimeEvent> {
    this.calls.push(`dispatch:${command.type}`);
    yield {type: "trace.toggle.requested"};
  }

  async shutdown(reason: "user" | "signal" | "fatal"): Promise<void> {
    this.calls.push(`shutdown:${reason}`);
  }
}

test("dispatcher delegates the complete RuntimeApplication contract", async () => {
  const application = new FakeApplication();
  const dispatcher = new CommandDispatcher(application);

  assert.deepEqual(await dispatcher.init(), [READY]);
  const events: RuntimeEvent[] = [];
  for await (const event of dispatcher.dispatch({type: "thread.new"})) {
    events.push(event);
  }
  await dispatcher.shutdown("signal");

  assert.deepEqual(events, [{type: "trace.toggle.requested"}]);
  assert.deepEqual(application.calls, [
    "init",
    "dispatch:thread.new",
    "shutdown:signal"
  ]);
});

test("dispatcher converts initialization failures into typed redacted events", async () => {
  const application = new FakeApplication();
  application.failInit = true;
  const dispatcher = new CommandDispatcher(application);

  assert.deepEqual(await dispatcher.init(), [
    {type: "runtime.init_failed", message: "init failed"}
  ]);
});

test("dispatcher redacts credentials from initialization failures", async () => {
  class SecretFailureApplication extends FakeApplication {
    override async init(): Promise<RuntimeEvent[]> {
      throw new Error(
        "Could not open C:/workspace/config.json with Bearer sk-super-secret-token-1234567890"
      );
    }
  }
  const dispatcher = new CommandDispatcher(new SecretFailureApplication());

  const events = await dispatcher.init();

  assert.equal(events[0]?.type, "runtime.init_failed");
  assert.equal(JSON.stringify(events).includes("sk-super-secret"), false);
  assert.match(JSON.stringify(events), /C:\/workspace\/config\.json/);
});

test("dispatcher redacts high-risk standalone tokens from initialization failures", async () => {
  class TokenFailureApplication extends FakeApplication {
    override async init(): Promise<RuntimeEvent[]> {
      throw new Error(
        "Keychain rejected AKIAABCDEFGHIJKLMNOP and AbCdEf0123456789+/AbCdEf0123456789+/"
      );
    }
  }

  const events = await new CommandDispatcher(new TokenFailureApplication()).init();
  const rendered = JSON.stringify(events);

  assert.equal(rendered.includes("AKIAABCDEFGHIJKLMNOP"), false);
  assert.equal(rendered.includes("AbCdEf0123456789"), false);
});

test("dispatcher fails safe when initialization errors reject introspection", async () => {
  const marker = "HOSTILE_INIT_MARKER";
  const secret = "sk-hostile-secret-1234567890";
  const path = "C:/private/credentials.json";
  const instanceofTrap = new Proxy({}, {
    getPrototypeOf() {
      throw new Error(`${marker} ${secret} ${path}`);
    },
    get() {
      throw new Error(`${marker} ${secret} ${path}`);
    }
  });
  const messageGetterTrap = new Error("unused");
  Object.defineProperty(messageGetterTrap, "message", {
    get() {
      throw new Error(`${marker} ${secret} ${path}`);
    }
  });
  const toStringTrap = {
    toString() {
      throw new Error(`${marker} ${secret} ${path}`);
    }
  };
  class HostileFailureApplication extends FakeApplication {
    constructor(private readonly failure: unknown) {
      super();
    }

    override async init(): Promise<RuntimeEvent[]> {
      throw this.failure;
    }
  }

  for (const hostile of [instanceofTrap, messageGetterTrap, toStringTrap]) {
    const events = await new CommandDispatcher(
      new HostileFailureApplication(hostile)
    ).init();
    const rendered = JSON.stringify(events);

    assert.deepEqual(events, [{
      type: "runtime.init_failed",
      message: "Runtime initialization failed."
    }]);
    for (const sensitive of [marker, secret, path]) {
      assert.equal(rendered.includes(sensitive), false, sensitive);
    }
  }
});

test("dispatcher redacts complete labeled credential lines and preserves later diagnostics", async () => {
  class LabeledFailureApplication extends FakeApplication {
    override async init(): Promise<RuntimeEvent[]> {
      throw new Error([
        "Authorization: Basic first-token second-token",
        "Authorization =   Bearer bearer-token extra-token",
        "API_KEY: api first second",
        "PASSWORD = open sesame phrase",
        "diagnostic: workspace lease is held"
      ].join("\n"));
    }
  }

  const events = await new CommandDispatcher(new LabeledFailureApplication()).init();
  const message = events[0]?.type === "runtime.init_failed" ? events[0].message : "";

  for (const secret of [
    "first-token",
    "second-token",
    "bearer-token",
    "extra-token",
    "api first second",
    "open sesame phrase"
  ]) {
    assert.equal(message.includes(secret), false, secret);
  }
  assert.match(message, /Authorization=\[REDACTED\]/);
  assert.match(message, /API_KEY=\[REDACTED\]/);
  assert.match(message, /PASSWORD=\[REDACTED\]/);
  assert.match(message, /diagnostic: workspace lease is held/);
});

test("dispatcher depends only on RuntimeApplication and defaults to ApplicationKernel", async () => {
  const source = await readFile(
    fileURLToPath(new URL("../src/runtime/command-dispatcher.ts", import.meta.url)),
    "utf8"
  );

  assert.equal(source.includes("RuntimePort"), false);
  assert.equal(source.includes("AgentRuntime"), false);
  assert.equal(source.includes("new ApplicationKernel()"), true);
});
