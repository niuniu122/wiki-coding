import assert from "node:assert/strict";
import test from "node:test";
import {DEFAULT_CONFIG} from "../src/config/config-manager.js";
import type {RuntimeEvent} from "../src/protocol.js";
import {ApplicationKernel} from "../src/runtime/application-kernel.js";
import {CommandArbiter} from "../src/runtime/command-arbiter.js";
import {classifyChatInput} from "../src/ui/chat-input-policy.js";

test("capability slash commands are read-only reports", () => {
  assert.deepEqual(classifyChatInput("/capabilities"), {type: "command", command: {type: "capability.list"}});
  assert.deepEqual(classifyChatInput("/capabilities search 查看文件"), {type: "command", command: {type: "capability.search", query: "查看文件"}});
  const arbiter = new CommandArbiter(); arbiter.markReady();
  const turn = arbiter.begin({type: "turn.submit", input: "hello"});
  assert.equal(arbiter.canDispatch({type: "capability.list"}), true);
  assert.equal(arbiter.canDispatch({type: "capability.search", query: "file"}), true);
  turn.finish();
});

test("kernel reports candidates without creating an invocation or Provider request", async () => {
  const calls: string[] = [];
  const kernel = new ApplicationKernel({services: {
    cwd: "C:/workspace",
    lease: {async acquire() {}, async release() {}},
    arbiter: new CommandArbiter(),
    providerService: {
      config: DEFAULT_CONFIG,
      async init() {}, async inspectCredential() { return {hasCredential: true, backend: "environment", userFilePath: "none"}; },
      async saveApiKey() { return "os-keyring"; }, list() { return []; }, async switch() { return "unused"; }
    },
    sessionService: {
      async init() { return []; }, async newThread() { throw new Error("unused"); }, async listThreads() { return []; }, async resumeThread() { throw new Error("unused"); }
    },
    turnEngine: {
      async *submit() { calls.push("provider.request"); }, interrupt() { return {type: "turn.interrupt.ignored", reason: "no_active_request"} as RuntimeEvent; }, async compact() { return []; }, async shutdown() {}
    },
    credentialStore: {createPlaintextConsent() { throw new Error("unused"); }},
    capabilityService: {
      list() { calls.push("catalog.list"); return {snapshotVersion: "v1", health: "fresh", mode: "exact+bm25" as const, capabilities: []}; },
      async search() { calls.push("catalog.search"); return {snapshotVersion: "v1", health: "fresh", mode: "exact", candidates: []}; }
    }
  } as never});
  await kernel.init();
  assert.equal((await collect(kernel.dispatch({type: "capability.list"})))[0]?.type, "capability.listed");
  assert.equal((await collect(kernel.dispatch({type: "capability.search", query: "file"})))[0]?.type, "capability.searched");
  assert.deepEqual(calls, ["catalog.list", "catalog.search"]);
});

async function collect(events: AsyncGenerator<RuntimeEvent>): Promise<RuntimeEvent[]> {
  const result: RuntimeEvent[] = []; for await (const event of events) result.push(event); return result;
}
