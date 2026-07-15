import assert from "node:assert/strict";
import test from "node:test";
import type {PlaintextConsent} from "../src/config/credential-store.js";
import type {Command, RuntimeEvent} from "../src/protocol.js";
import {
  ApplicationKernel,
  type ApplicationKernelServices
} from "../src/runtime/application-kernel.js";
import {CommandArbiter} from "../src/runtime/command-arbiter.js";
import type {ActiveModelSelection} from "../src/runtime/model-selection-service.js";
import {PermissionService} from "../src/runtime/permission-service.js";
import type {AppConfig, ThreadRecord} from "../src/types.js";
import {readFile} from "node:fs/promises";

const CONFIG: AppConfig = {
  schemaVersion: 1,
  modelProvider: "minimax-official",
  modelProviders: {
    "minimax-official": {
      name: "MiniMax Official",
      baseUrl: "https://example.test/v1",
      protocol: "responses",
      envKey: "MINIMAX_API_KEY",
      defaultModel: "MiniMax-M3"
    }
  },
  model: "MiniMax-M3",
  context: {
    workingContextLimit: 128_000,
    autoCompactRatio: 0.9,
    maxCompletionTokens: 8_192
  }
};

const ACTIVE_MODEL = Object.freeze({
  adapterId: "adapter:minimax/builtin",
  providerProfileId: "provider:minimax/official",
  modelProfileId: "model:minimax/official/MiniMax-M3",
  providerDisplayName: "MiniMax Official",
  modelDisplayName: "MiniMax Official / MiniMax-M3",
  model: "MiniMax-M3",
  protocol: "responses",
  source: "builtin",
  contextWindow: 128_000,
  maxOutputTokens: 8_192,
  autoCompactRatio: 0.9,
  supportsNativeToolCalls: true
}) as ActiveModelSelection;

const THREAD: ThreadRecord = {
  id: "thread_kernel",
  title: "Kernel test",
  createdAt: "2026-07-10T00:00:00.000Z",
  updatedAt: "2026-07-10T00:00:00.000Z",
  model: CONFIG.model,
  cwd: "C:/workspace",
  status: "active"
};

function createLifecycleKernel(options: {
  failSessionInit?: boolean;
  failSessionInitOnce?: boolean;
  failLeaseReleaseOnce?: boolean;
  gateStage?: "lease" | "provider" | "session";
  gateProviderSwitch?: boolean;
  failProviderSwitch?: boolean;
  enableAgent?: boolean;
} = {}): {
  kernel: ApplicationKernel;
  calls: string[];
  gateEntered: Promise<void>;
  releaseGate(): void;
  providerSwitchEntered: Promise<void>;
  releaseProviderSwitch(): void;
  permissionService: PermissionService;
} {
  const calls: string[] = [];
  const permissionService = new PermissionService();
  let releaseTurn!: () => void;
  const turnReleased = new Promise<void>((resolve) => {
    releaseTurn = resolve;
  });
  let releaseAttempts = 0;
  let sessionInitAttempts = 0;
  let markGateEntered!: () => void;
  const gateEntered = new Promise<void>((resolve) => {
    markGateEntered = resolve;
  });
  let releaseGate!: () => void;
  const gateReleased = new Promise<void>((resolve) => {
    releaseGate = resolve;
  });
  let markProviderSwitchEntered!: () => void;
  const providerSwitchEntered = new Promise<void>((resolve) => {
    markProviderSwitchEntered = resolve;
  });
  let releaseProviderSwitch!: () => void;
  const providerSwitchReleased = new Promise<void>((resolve) => {
    releaseProviderSwitch = resolve;
  });
  const waitAtGate = async (stage: "lease" | "provider" | "session") => {
    if (options.gateStage !== stage) {
      return;
    }
    markGateEntered();
    await gateReleased;
  };

  const services: ApplicationKernelServices = {
    cwd: THREAD.cwd,
    lease: {
      async acquire() {
        calls.push("lease.acquire");
        await waitAtGate("lease");
      },
      async release() {
        calls.push("lease.release");
        releaseAttempts += 1;
        if (options.failLeaseReleaseOnce && releaseAttempts === 1) {
          throw new Error("lease release failed once");
        }
      }
    },
    arbiter: new CommandArbiter(),
    providerService: {
      async init() {
        calls.push("provider.init");
        await waitAtGate("provider");
      },
      async inspectCredential() {
        return {
          backend: "os-keyring" as const,
          hasCredential: false,
          userFilePath: "C:/user/credentials.json"
        };
      },
      async saveApiKey(_value: string, _consent?: PlaintextConsent) {
        return "os-keyring" as const;
      },
      list() {
        return ["minimax-official | responses | model=MiniMax-M3 | active"];
      },
      async switch(providerId: string) {
        calls.push("legacy-provider.switch");
        return `${providerId} selected`;
      },
      getActiveModelSelection() {
        return ACTIVE_MODEL;
      },
      async switchProvider(providerId: string) {
        calls.push("provider.switch");
        if (options.gateProviderSwitch) {
          markProviderSwitchEntered();
          await providerSwitchReleased;
        }
        if (options.failProviderSwitch) {
          throw new Error("provider switch failed");
        }
        return {...ACTIVE_MODEL, providerDisplayName: providerId};
      },
      get config() {
        return CONFIG;
      }
    },
    sessionService: {
      async init() {
        calls.push("session.init");
        await waitAtGate("session");
        sessionInitAttempts += 1;
        if (
          options.failSessionInit ||
          (options.failSessionInitOnce && sessionInitAttempts === 1)
        ) {
          throw new Error("session init failed");
        }
        return [{type: "thread.loaded", thread: THREAD}];
      },
      async newThread() {
        calls.push("session.new");
        return {thread: THREAD, events: [{type: "thread.loaded", thread: THREAD}]};
      },
      async listThreads() {
        calls.push("session.list");
        return [THREAD];
      },
      async resumeThread() {
        calls.push("session.resume");
        return {thread: THREAD, events: [{type: "thread.loaded", thread: THREAD}]};
      }
    },
    turnEngine: {
      async *submit(input: string) {
        calls.push("turn.submit");
        yield {type: "turn.started", turnId: "turn_kernel", input};
        await turnReleased;
        yield {type: "turn.interrupted", turnId: "turn_kernel"};
      },
      interrupt() {
        calls.push("turn.interrupt");
        releaseTurn();
        return {type: "turn.interrupt.requested", turnId: "turn_kernel"};
      },
      async compact() {
        calls.push("turn.compact");
        return [];
      },
      async shutdown() {
        calls.push("turn.shutdown");
        releaseTurn();
      }
    },
    credentialStore: {
      createPlaintextConsent() {
        throw new Error("not used by this fixture");
      }
    },
    permissionService,
    ...(options.enableAgent ? {
      agentRunEngine: {
        get hasActiveRun() { return false; },
        async *submit(input: string) {
          calls.push("agent.submit");
          yield {type: "agent.started" as const, turnId: "turn_agent", input};
          yield {type: "agent.stopped" as const, turnId: "turn_agent", reason: "fixture_complete"};
        },
        async *continue() {
          calls.push("agent.continue");
          yield {type: "agent.stopped" as const, turnId: "unavailable", reason: "no_recoverable_checkpoint"};
        },
        interrupt() {
          calls.push("agent.interrupt");
          return {type: "turn.interrupt.ignored" as const, reason: "no_active_request" as const};
        },
        async shutdown() { calls.push("agent.shutdown"); }
      }
    } : {})
  };

  return {
    kernel: new ApplicationKernel({services}),
    calls,
    gateEntered,
    releaseGate,
    providerSwitchEntered,
    releaseProviderSwitch,
    permissionService
  };
}

async function collect(
  application: ApplicationKernel,
  command: Command
): Promise<RuntimeEvent[]> {
  const events: RuntimeEvent[] = [];
  for await (const event of application.dispatch(command)) {
    events.push(event);
  }
  return events;
}

test("kernel acquires the lease before initializing services and releases it on shutdown", async () => {
  const {kernel, calls} = createLifecycleKernel();

  const events = await kernel.init();

  assert.equal(events.at(-1)?.type, "runtime.ready");
  assert.deepEqual(calls.slice(0, 3), [
    "lease.acquire",
    "provider.init",
    "session.init"
  ]);
  await kernel.shutdown("user");
  assert.deepEqual(calls.slice(-2), ["turn.shutdown", "lease.release"]);
});

test("kernel releases its lease when service initialization fails", async () => {
  const {kernel, calls} = createLifecycleKernel({failSessionInit: true});

  await assert.rejects(() => kernel.init(), /session init failed/);

  assert.deepEqual(calls, [
    "lease.acquire",
    "provider.init",
    "session.init",
    "lease.release"
  ]);
});

test("kernel can retry initialization after releasing a failed attempt", async () => {
  const {kernel, calls} = createLifecycleKernel({failSessionInitOnce: true});

  await assert.rejects(() => kernel.init(), /session init failed/);
  const events = await kernel.init();

  assert.equal(events.at(-1)?.type, "runtime.ready");
  assert.equal(calls.filter((call) => call === "lease.acquire").length, 2);
  assert.equal(calls.filter((call) => call === "lease.release").length, 1);
  await kernel.shutdown("user");
});

test("kernel retries a transient workspace lease release failure", async () => {
  const {kernel, calls} = createLifecycleKernel({failLeaseReleaseOnce: true});
  await kernel.init();

  await assert.rejects(() => kernel.shutdown("signal"), /lease release failed once/);
  await kernel.shutdown("signal");

  assert.equal(calls.filter((call) => call === "turn.shutdown").length, 1);
  assert.equal(calls.filter((call) => call === "lease.release").length, 2);
});

test("repeated successful shutdown preserves stopped state without repeating cleanup", async () => {
  const {kernel, calls} = createLifecycleKernel();
  await kernel.init();

  await kernel.shutdown("user");
  const firstRejection = await collect(kernel, {type: "thread.list"});
  const cleanupCalls = calls.filter(
    (call) => call === "turn.shutdown" || call === "lease.release"
  );
  await kernel.shutdown("user");
  const secondRejection = await collect(kernel, {type: "thread.list"});

  assert.deepEqual(cleanupCalls, ["turn.shutdown", "lease.release"]);
  assert.deepEqual(
    calls.filter((call) => call === "turn.shutdown" || call === "lease.release"),
    cleanupCalls
  );
  assert.equal(firstRejection[0]?.type, "command.rejected");
  assert.equal(secondRejection[0]?.type, "command.rejected");
  if (firstRejection[0]?.type === "command.rejected") {
    assert.equal(firstRejection[0].phase, "stopped");
  }
  if (secondRejection[0]?.type === "command.rejected") {
    assert.equal(secondRejection[0].phase, "stopped");
  }
});

test("shutdown waits for pending lease acquisition and cancels initialization safely", async () => {
  const {kernel, calls, gateEntered, releaseGate} = createLifecycleKernel({
    gateStage: "lease"
  });
  const initialization = kernel.init().then(
    (events) => ({status: "fulfilled" as const, events}),
    (error: unknown) => ({status: "rejected" as const, error})
  );
  await gateEntered;
  let shutdownSettled = false;
  const shutdown = kernel.shutdown("signal").then(() => {
    shutdownSettled = true;
  });

  await new Promise<void>((resolve) => setImmediate(resolve));
  assert.equal(shutdownSettled, false);
  releaseGate();
  const result = await initialization;
  await shutdown;

  assert.equal(result.status, "rejected");
  if (result.status === "rejected") {
    assert.match(String(result.error), /shutdown|cancel/i);
  }
  assert.deepEqual(calls.filter((call) => call === "provider.init"), []);
  assert.deepEqual(calls.filter((call) => call === "session.init"), []);
  assert.equal(calls.filter((call) => call === "lease.release").length, 1);
  const rejected = await collect(kernel, {type: "thread.list"});
  assert.equal(rejected[0]?.type, "command.rejected");
  if (rejected[0]?.type === "command.rejected") {
    assert.equal(rejected[0].phase, "stopped");
  }
});

test("app exit bypasses booting and shutdown still owns initialization cleanup", async () => {
  const {kernel, calls, gateEntered, releaseGate} = createLifecycleKernel({
    gateStage: "provider"
  });
  const initialization = kernel.init().then(
    (events) => ({status: "fulfilled" as const, events}),
    (error: unknown) => ({status: "rejected" as const, error})
  );
  await gateEntered;

  assert.deepEqual(await collect(kernel, {type: "app.exit"}), [
    {type: "app.exit.requested"}
  ]);
  const shutdown = kernel.shutdown("user");
  releaseGate();
  const result = await initialization;
  await shutdown;

  assert.equal(result.status, "rejected");
  assert.equal(calls.filter((call) => call === "lease.release").length, 1);
  assert.equal(calls.filter((call) => call === "turn.shutdown").length, 1);
});

test("shutdown cancels initialization during Provider and Session startup", async () => {
  for (const gateStage of ["provider", "session"] as const) {
    const {kernel, calls, gateEntered, releaseGate} = createLifecycleKernel({
      gateStage
    });
    const initialization = kernel.init().then(
      (events) => ({status: "fulfilled" as const, events}),
      (error: unknown) => ({status: "rejected" as const, error})
    );
    await gateEntered;
    let shutdownSettled = false;
    const shutdown = kernel.shutdown("signal").then(() => {
      shutdownSettled = true;
    });

    await new Promise<void>((resolve) => setImmediate(resolve));
    assert.equal(shutdownSettled, false, gateStage);
    releaseGate();
    const result = await initialization;
    await shutdown;

    assert.equal(result.status, "rejected", gateStage);
    assert.equal(
      result.status === "fulfilled" &&
        result.events.some((event) => event.type === "runtime.ready"),
      false,
      gateStage
    );
    assert.equal(calls.filter((call) => call === "lease.release").length, 1);
    assert.equal(
      calls.filter((call) => call === "session.init").length,
      gateStage === "provider" ? 0 : 1
    );
    const rejected = await collect(kernel, {type: "thread.list"});
    assert.equal(rejected[0]?.type, "command.rejected", gateStage);
    if (rejected[0]?.type === "command.rejected") {
      assert.equal(rejected[0].phase, "stopped", gateStage);
    }
  }
});

test("kernel rejects a second mutating command while a Turn runs", async () => {
  const {kernel} = createLifecycleKernel();
  await kernel.init();
  let markTurnStarted!: () => void;
  const turnStarted = new Promise<void>((resolve) => {
    markTurnStarted = resolve;
  });
  const running = (async () => {
    const events: RuntimeEvent[] = [];
    for await (const event of kernel.dispatch({type: "turn.submit", input: "hello"})) {
      events.push(event);
      if (event.type === "turn.started") {
        markTurnStarted();
      }
    }
    return events;
  })();

  await turnStarted;
  const rejected = await collect(kernel, {type: "thread.new"});
  assert.equal(rejected[0]?.type, "command.rejected");
  const interrupted = await collect(kernel, {type: "turn.interrupt"});
  assert.equal(interrupted[0]?.type, "turn.interrupt.requested");
  await running;
  await kernel.shutdown("user");
});

test("kernel rejects a second idle mutation until the first completes", async () => {
  const {kernel, providerSwitchEntered, releaseProviderSwitch} = createLifecycleKernel({
    gateProviderSwitch: true
  });
  await kernel.init();

  const switching = collect(kernel, {
    type: "provider.switch",
    providerId: "minimax-official"
  });
  await providerSwitchEntered;
  const rejected = await collect(kernel, {type: "thread.new"});

  assert.equal(rejected[0]?.type, "command.rejected");
  releaseProviderSwitch();
  await switching;
  assert.equal((await collect(kernel, {type: "thread.new"}))[0]?.type, "thread.loaded");
  await kernel.shutdown("user");
});

test("kernel releases idle mutation ownership after failure", async () => {
  const {kernel} = createLifecycleKernel({failProviderSwitch: true});
  await kernel.init();

  const failed = await collect(kernel, {
    type: "provider.switch",
    providerId: "minimax-official"
  });

  assert.deepEqual(failed, [{type: "model.change_failed", code: "selection_failed"}]);
  assert.equal((await collect(kernel, {type: "thread.new"}))[0]?.type, "thread.loaded");
  await kernel.shutdown("user");
});

test("session permission applies until a new or resumed session resets it", async () => {
  const {kernel, permissionService} = createLifecycleKernel();
  await kernel.init();

  assert.deepEqual(await collect(kernel, {type: "permission.show"}), [
    {type: "permission.current", mode: "confirm"}
  ]);
  assert.deepEqual(
    await collect(kernel, {type: "permission.set", mode: "full_access"}),
    [{type: "permission.changed", mode: "full_access"}]
  );
  assert.equal(permissionService.current, "full_access");

  await collect(kernel, {type: "model.switch", modelProfileId: "model:unchanged"});
  assert.equal(permissionService.current, "full_access");

  await collect(kernel, {type: "thread.new"});
  assert.equal(permissionService.current, "confirm");
  permissionService.set("workspace_read");
  await collect(kernel, {type: "thread.resume", threadId: THREAD.id});
  assert.equal(permissionService.current, "confirm");
  await kernel.shutdown("user");
});

test("kernel keeps explicit Agent routing separate from chat and fails closed when disabled", async () => {
  const enabled = createLifecycleKernel({enableAgent: true});
  await enabled.kernel.init();
  const agentEvents = await collect(enabled.kernel, {type: "agent.submit", input: "inspect"});
  assert.deepEqual(agentEvents.map((event) => event.type), ["agent.started", "agent.stopped"]);
  assert.equal(enabled.calls.includes("agent.submit"), true);
  assert.equal(enabled.calls.includes("turn.submit"), false);
  const continued = await collect(enabled.kernel, {type: "agent.continue"});
  assert.equal(continued[0]?.type, "agent.stopped");
  assert.equal(enabled.calls.includes("agent.continue"), true);
  await enabled.kernel.shutdown("user");
  assert.equal(enabled.calls.includes("agent.shutdown"), true);

  const disabled = createLifecycleKernel();
  await disabled.kernel.init();
  assert.deepEqual(await collect(disabled.kernel, {type: "agent.submit", input: "inspect"}), [
    {type: "agent.stopped", turnId: "unavailable", reason: "agent_disabled"}
  ]);
  await disabled.kernel.shutdown("user");
});

test("shutdown bypass waits for an active idle mutation before releasing the lease", async () => {
  const {
    kernel,
    calls,
    providerSwitchEntered,
    releaseProviderSwitch
  } = createLifecycleKernel({gateProviderSwitch: true});
  await kernel.init();

  const switching = collect(kernel, {
    type: "provider.switch",
    providerId: "minimax-official"
  });
  await providerSwitchEntered;
  const exiting = collect(kernel, {type: "app.exit"});
  await new Promise<void>((resolve) => setImmediate(resolve));

  assert.equal(calls.includes("lease.release"), false);
  releaseProviderSwitch();
  await Promise.all([switching, exiting]);
  await kernel.shutdown("user");
  assert.equal(calls.filter((call) => call === "lease.release").length, 1);
});

test("kernel command routing is compile-time exhaustive", async () => {
  const source = await readFile(
    new URL("../src/runtime/application-kernel.ts", import.meta.url),
    "utf8"
  );

  assert.match(source, /default:\s*return assertNever\(command\)/);
  assert.match(source, /function assertNever\(value: never\): never/);
});

test("app.exit requests external shutdown without becoming a second shutdown owner", async () => {
  const {kernel, calls} = createLifecycleKernel();
  await kernel.init();

  const events = await collect(kernel, {type: "app.exit"});

  assert.deepEqual(events, [{type: "app.exit.requested"}]);
  assert.equal(calls.includes("lease.release"), false);
  await kernel.shutdown("user");
  assert.equal(calls.filter((call) => call === "lease.release").length, 1);
});
