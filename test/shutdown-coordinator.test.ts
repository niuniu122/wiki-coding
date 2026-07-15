import assert from "node:assert/strict";
import test from "node:test";
import {
  ShutdownCoordinator,
  type SignalSource
} from "../src/runtime/shutdown-coordinator.js";

class FakeSignalSource implements SignalSource {
  private readonly listeners = new Map<string, Set<() => void>>();

  on(signal: "SIGINT" | "SIGTERM", listener: () => void): void {
    const listeners = this.listeners.get(signal) ?? new Set<() => void>();
    listeners.add(listener);
    this.listeners.set(signal, listeners);
  }

  off(signal: "SIGINT" | "SIGTERM", listener: () => void): void {
    this.listeners.get(signal)?.delete(listener);
  }

  emit(signal: "SIGINT" | "SIGTERM"): void {
    for (const listener of this.listeners.get(signal) ?? []) {
      listener();
    }
  }

  count(signal: "SIGINT" | "SIGTERM"): number {
    return this.listeners.get(signal)?.size ?? 0;
  }
}

test("signals flush runtime state once before UI teardown and listener removal", async () => {
  const source = new FakeSignalSource();
  const calls: string[] = [];
  let releaseShutdown!: () => void;
  const shutdownGate = new Promise<void>((resolve) => {
    releaseShutdown = resolve;
  });
  const coordinator = new ShutdownCoordinator({
    source,
    async shutdown(reason) {
      calls.push(`shutdown:${reason}`);
      await shutdownGate;
      calls.push("flush/checkpoint/release");
    },
    afterShutdown() {
      calls.push("unmount");
    }
  });
  coordinator.install();

  source.emit("SIGINT");
  source.emit("SIGTERM");
  assert.deepEqual(calls, ["shutdown:signal"]);
  assert.equal(source.count("SIGINT"), 1);

  releaseShutdown();
  await coordinator.pending;

  assert.deepEqual(calls, ["shutdown:signal", "flush/checkpoint/release", "unmount"]);
  assert.equal(source.count("SIGINT"), 0);
  assert.equal(source.count("SIGTERM"), 0);
});

test("manual and signal shutdown share the same idempotent operation", async () => {
  const source = new FakeSignalSource();
  let shutdownCount = 0;
  const coordinator = new ShutdownCoordinator({
    source,
    async shutdown() {
      shutdownCount += 1;
    }
  });
  coordinator.install();

  const first = coordinator.request("user");
  source.emit("SIGINT");
  await Promise.all([first, coordinator.pending]);

  assert.equal(shutdownCount, 1);
});
