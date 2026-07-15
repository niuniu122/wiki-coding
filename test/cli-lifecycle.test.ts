import assert from "node:assert/strict";
import {execFile} from "node:child_process";
import {promisify} from "node:util";
import test from "node:test";
import type {Command, RuntimeEvent} from "../src/protocol.js";
import {
  runCli,
  runCliMain,
  type CliRenderInstance,
  type RuntimeDispatcher
} from "../src/cli.js";
import type {RuntimeSignal, SignalSource} from "../src/runtime/shutdown-coordinator.js";

const execFileAsync = promisify(execFile);

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

class FakeDispatcher implements RuntimeDispatcher {
  shutdownCount = 0;

  constructor(private readonly shutdownFailure?: Error) {}

  async init(): Promise<RuntimeEvent[]> {
    return [];
  }

  async *dispatch(_command: Command): AsyncGenerator<RuntimeEvent> {}

  async shutdown(_reason: "user" | "signal" | "fatal"): Promise<void> {
    this.shutdownCount += 1;
    if (this.shutdownFailure) {
      throw this.shutdownFailure;
    }
  }
}

function fakeRender(): {
  instance: CliRenderInstance;
  unmounted: Promise<void>;
  exitUi(): void;
} {
  let finish!: () => void;
  const unmounted = new Promise<void>((resolve) => {
    finish = resolve;
  });
  return {
    instance: {
      waitUntilExit: () => unmounted,
      unmount: finish
    },
    unmounted,
    exitUi: finish
  };
}

test("importing the CLI module does not render or keep a child process alive", async () => {
  const cliUrl = new URL("../src/cli.tsx", import.meta.url).href;
  const {stdout, stderr} = await execFileAsync(
    process.execPath,
    [
      "--import",
      "tsx",
      "--input-type=module",
      "--eval",
      `await import(${JSON.stringify(cliUrl)}); process.stdout.write("imported")`
    ],
    {cwd: process.cwd(), timeout: 5_000}
  );

  assert.equal(stdout, "imported");
  assert.equal(stderr, "");
});

test("SIGINT and SIGTERM share one successful shutdown before clean exit", async () => {
  const source = new FakeSignalSource();
  const dispatcher = new FakeDispatcher();
  const rendered = fakeRender();
  const running = runCli({
    dispatcher,
    signalSource: source,
    renderApp: () => rendered.instance
  });

  source.emit("SIGINT");
  source.emit("SIGTERM");
  await running;

  assert.equal(dispatcher.shutdownCount, 1);
  await rendered.unmounted;
});

test("shutdown rejection reports failure and sets a nonzero exit status", async () => {
  const source = new FakeSignalSource();
  const dispatcher = new FakeDispatcher(new Error("injected shutdown failure"));
  const rendered = fakeRender();
  const errors: string[] = [];
  let exitCode = 0;
  const running = runCliMain({
    dispatcher,
    signalSource: source,
    renderApp: () => rendered.instance,
    writeError: (message) => errors.push(message),
    setExitCode: (code) => {
      exitCode = code;
    }
  });

  source.emit("SIGTERM");
  await running;

  assert.equal(dispatcher.shutdownCount, 1);
  assert.equal(exitCode, 1);
  assert.deepEqual(errors, ["injected shutdown failure\n"]);
});

test("normal UI exit delegates exactly one shutdown to the coordinator", async () => {
  const source = new FakeSignalSource();
  const dispatcher = new FakeDispatcher();
  const rendered = fakeRender();
  const running = runCli({
    dispatcher,
    signalSource: source,
    renderApp: () => rendered.instance
  });

  rendered.exitUi();
  await running;

  assert.equal(dispatcher.shutdownCount, 1);
});
