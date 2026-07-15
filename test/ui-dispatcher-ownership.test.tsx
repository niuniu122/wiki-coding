import assert from "node:assert/strict";
import {PassThrough} from "node:stream";
import test from "node:test";
import React from "react";
import {render} from "ink";
import type {Command, RuntimeEvent} from "../src/protocol.js";
import {runCli, type RuntimeDispatcher} from "../src/cli.js";
import type {RuntimeSignal, SignalSource} from "../src/runtime/shutdown-coordinator.js";
import {App} from "../src/ui/App.js";

class FakeSignalSource implements SignalSource {
  private readonly listeners = new Map<RuntimeSignal, Set<() => void>>();

  on(signal: RuntimeSignal, listener: () => void): void {
    const listeners = this.listeners.get(signal) ?? new Set<() => void>();
    listeners.add(listener);
    this.listeners.set(signal, listeners);
  }

  off(signal: RuntimeSignal, listener: () => void): void {
    this.listeners.get(signal)?.delete(listener);
  }

  emit(signal: RuntimeSignal): void {
    for (const listener of this.listeners.get(signal) ?? []) {
      listener();
    }
  }
}

class CountingDispatcher implements RuntimeDispatcher {
  shutdownCount = 0;
  initCount = 0;

  constructor(
    private readonly initResult: Promise<RuntimeEvent[]> = Promise.resolve([{
      type: "runtime.ready",
      hasApiKey: true,
      providerSummary: "fake",
      recoveredTurns: 0
    }])
  ) {}

  async init(): Promise<RuntimeEvent[]> {
    this.initCount += 1;
    return this.initResult;
  }

  async *dispatch(command: Command): AsyncGenerator<RuntimeEvent> {
    if (command.type === "app.exit") {
      yield {type: "app.exit.requested"};
    }
  }

  async shutdown(): Promise<void> {
    this.shutdownCount += 1;
  }
}

class RetryDispatcher implements RuntimeDispatcher {
  initCount = 0;
  shutdownCount = 0;
  private finishRetry!: (events: RuntimeEvent[]) => void;
  readonly retryGate = new Promise<RuntimeEvent[]>((resolve) => {
    this.finishRetry = resolve;
  });

  async init(): Promise<RuntimeEvent[]> {
    this.initCount += 1;
    if (this.initCount === 1) {
      return [{type: "runtime.init_failed", message: "first boot failed"}];
    }
    return this.retryGate;
  }

  resolveRetry(): void {
    this.finishRetry([{
      type: "runtime.ready",
      hasApiKey: true,
      providerSummary: "recovered",
      recoveredTurns: 0
    }]);
  }

  async *dispatch(command: Command): AsyncGenerator<RuntimeEvent> {
    if (command.type === "app.exit") {
      yield {type: "app.exit.requested"};
    }
  }

  async shutdown(): Promise<void> {
    this.shutdownCount += 1;
  }
}

function createInkHarness() {
  const options = () => {
    const stdout = new PassThrough() as PassThrough & NodeJS.WriteStream;
    const stderr = new PassThrough() as PassThrough & NodeJS.WriteStream;
    const stdin = new PassThrough() as PassThrough &
      NodeJS.ReadStream & {
        isTTY: boolean;
        setRawMode(value: boolean): void;
        ref(): void;
        unref(): void;
      };
    stdin.isTTY = true;
    stdin.setRawMode = () => undefined;
    stdin.ref = () => undefined;
    stdin.unref = () => undefined;
    return {stdin, stdout, stderr, exitOnCtrlC: false, patchConsole: false};
  };
  return {
    renderApp(dispatcher: RuntimeDispatcher) {
      return render(<App dispatcher={dispatcher} />, options());
    },
    renderOwned(createDispatcher: () => RuntimeDispatcher) {
      return render(<App createDispatcher={createDispatcher} />, options());
    }
  };
}

function createInteractiveHarness(dispatcher: RuntimeDispatcher) {
  const stdin = new PassThrough() as PassThrough &
    NodeJS.ReadStream & {
      isTTY: boolean;
      setRawMode(value: boolean): void;
      ref(): void;
      unref(): void;
    };
  const stdout = new PassThrough() as PassThrough & NodeJS.WriteStream;
  const output: string[] = [];
  stdout.on("data", (chunk) => output.push(String(chunk)));
  const stderr = new PassThrough() as PassThrough & NodeJS.WriteStream;
  stdin.isTTY = true;
  stdin.setRawMode = () => undefined;
  stdin.ref = () => undefined;
  stdin.unref = () => undefined;
  const instance = render(<App dispatcher={dispatcher} />, {
    stdin,
    stdout,
    stderr,
    exitOnCtrlC: false,
    patchConsole: false
  });
  return {instance, stdin, output: () => output.join("")};
}

async function flushInk(): Promise<void> {
  for (let index = 0; index < 5; index++) {
    await new Promise<void>((resolve) => setImmediate(resolve));
  }
}

async function submitInput(stdin: PassThrough, value: string): Promise<void> {
  stdin.write(value);
  await flushInk();
  stdin.write("\r");
}

test("a supplied dispatcher has one shutdown owner across signal and App unmount", async () => {
  const source = new FakeSignalSource();
  const dispatcher = new CountingDispatcher();
  const harness = createInkHarness();
  const running = runCli({
    dispatcher,
    signalSource: source,
    renderApp: harness.renderApp
  });

  await new Promise<void>((resolve) => setImmediate(resolve));
  source.emit("SIGINT");
  await running;
  await new Promise<void>((resolve) => setImmediate(resolve));

  assert.equal(dispatcher.shutdownCount, 1);
});

test("standalone App shuts down its internally created dispatcher once on unmount", async () => {
  const dispatcher = new CountingDispatcher();
  const harness = createInkHarness();
  const instance = harness.renderOwned(() => dispatcher);
  await new Promise<void>((resolve) => setImmediate(resolve));

  const exited = instance.waitUntilExit();
  instance.unmount();
  await exited;
  await new Promise<void>((resolve) => setImmediate(resolve));

  assert.equal(dispatcher.shutdownCount, 1);
});

test("owned dispatcher cleanup remains single-flight when init resolves after unmount", async () => {
  let finishInit!: (events: RuntimeEvent[]) => void;
  const initGate = new Promise<RuntimeEvent[]>((resolve) => {
    finishInit = resolve;
  });
  const dispatcher = new CountingDispatcher(initGate);
  const harness = createInkHarness();
  const instance = harness.renderOwned(() => dispatcher);
  await new Promise<void>((resolve) => setImmediate(resolve));

  const exited = instance.waitUntilExit();
  instance.unmount();
  await exited;
  await new Promise<void>((resolve) => setImmediate(resolve));
  assert.equal(dispatcher.shutdownCount, 1);

  finishInit([{
    type: "runtime.ready",
    hasApiKey: true,
    providerSummary: "late",
    recoveredTurns: 0
  }]);
  await new Promise<void>((resolve) => setImmediate(resolve));
  assert.equal(dispatcher.shutdownCount, 1);
});

test("mounted App retries the same dispatcher single-flight", async () => {
  const dispatcher = new RetryDispatcher();
  const {instance, stdin} = createInteractiveHarness(dispatcher);
  await flushInk();
  assert.equal(dispatcher.initCount, 1);

  await submitInput(stdin, "/retry");
  await flushInk();
  assert.equal(dispatcher.initCount, 2);

  await submitInput(stdin, "/retry");
  await flushInk();
  assert.equal(dispatcher.initCount, 2);

  dispatcher.resolveRetry();
  await flushInk();
  instance.unmount();
});

test("mounted App exposes exit from initialization recovery", async () => {
  const source = new FakeSignalSource();
  const dispatcher = new RetryDispatcher();
  const {instance, stdin} = createInteractiveHarness(dispatcher);
  const running = runCli({
    dispatcher,
    signalSource: source,
    renderApp: () => instance
  });
  await flushInk();

  await submitInput(stdin, "/exit");
  await running;

  assert.equal(dispatcher.shutdownCount, 1);
});

test("mounted App contains hostile initialization rejection without an unhandled failure", async () => {
  const marker = "HOSTILE_APP_INIT_MARKER";
  const secret = "sk-hostile-app-secret-1234567890";
  const path = "C:/private/app-credentials.json";
  const hostile = new Proxy({}, {
    getPrototypeOf() {
      throw new Error(`${marker} ${secret} ${path}`);
    },
    get() {
      throw new Error(`${marker} ${secret} ${path}`);
    }
  });
  class HostileDispatcher extends CountingDispatcher {
    exitCount = 0;

    override async init(): Promise<RuntimeEvent[]> {
      throw hostile;
    }

    override async *dispatch(command: Command): AsyncGenerator<RuntimeEvent> {
      if (command.type === "app.exit") {
        this.exitCount += 1;
        yield {type: "app.exit.requested"};
      }
    }
  }
  const dispatcher = new HostileDispatcher();
  const {instance, stdin, output} = createInteractiveHarness(dispatcher);

  await flushInk();
  await submitInput(stdin, "/exit");
  await flushInk();
  const frame = output();

  assert.equal(dispatcher.exitCount, 1);
  for (const sensitive of [marker, secret, path]) {
    assert.equal(frame.includes(sensitive), false, sensitive);
  }
  instance.unmount();
});

test("mounted App forwards explicit Agent and forced-chat commands only through dispatcher", async () => {
  class CaptureDispatcher extends CountingDispatcher {
    readonly commands: Command[] = [];
    override async *dispatch(command: Command): AsyncGenerator<RuntimeEvent> {
      this.commands.push(command);
    }
  }
  const dispatcher = new CaptureDispatcher();
  const {instance, stdin} = createInteractiveHarness(dispatcher);
  await flushInk();
  await submitInput(stdin, "/agent inspect project");
  await flushInk();
  await submitInput(stdin, "/chat explain project");
  await flushInk();
  assert.deepEqual(dispatcher.commands, [
    {type: "agent.submit", input: "inspect project"},
    {type: "turn.submit", input: "explain project"}
  ]);
  instance.unmount();
});
