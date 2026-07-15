import assert from "node:assert/strict";
import test from "node:test";
import {DEFAULT_CONFIG} from "../src/config/config-manager.js";
import type {RuntimeEvent} from "../src/protocol.js";
import {ApplicationKernel} from "../src/runtime/application-kernel.js";
import {CommandArbiter} from "../src/runtime/command-arbiter.js";
import {classifyChatInput} from "../src/ui/chat-input-policy.js";

const ACTIVE = Object.freeze({
  adapterId: "adapter:minimax/builtin",
  providerProfileId: "provider:minimax/official",
  modelProfileId: "model:minimax/official/MiniMax-M3",
  providerDisplayName: "MiniMax Official",
  modelDisplayName: "MiniMax Official / MiniMax-M3",
  model: "MiniMax-M3",
  protocol: "responses",
  source: "builtin",
  contextWindow: 200_000,
  maxOutputTokens: 8_192,
  autoCompactRatio: 0.9,
  supportsNativeToolCalls: true
});

test("model slash commands remain distinct from provider compatibility syntax", () => {
  assert.deepEqual(classifyChatInput("/models"), {
    type: "command",
    command: {type: "model.list"}
  });
  assert.deepEqual(classifyChatInput(`/model ${ACTIVE.modelProfileId}`), {
    type: "command",
    command: {type: "model.switch", modelProfileId: ACTIVE.modelProfileId}
  });
  assert.deepEqual(classifyChatInput("/provider minimax-official"), {
    type: "command",
    command: {type: "provider.switch", providerId: "minimax-official"}
  });
});

test("model listing stays read-only while model switching is rejected during a Turn", () => {
  const arbiter = new CommandArbiter();
  arbiter.markReady();
  const turn = arbiter.begin({type: "turn.submit", input: "hello"});
  assert.equal(arbiter.canDispatch({type: "model.list"} as never), true);
  assert.equal(
    arbiter.canDispatch({
      type: "model.switch",
      modelProfileId: ACTIVE.modelProfileId
    } as never),
    false
  );
  turn.finish();
});

test("kernel routes model and provider commands through one transactional selector", async () => {
  const calls: string[] = [];
  const services = {
    cwd: "C:/workspace",
    lease: {
      async acquire() {},
      async release() {}
    },
    arbiter: new CommandArbiter(),
    providerService: {
      config: DEFAULT_CONFIG,
      async init() {},
      async inspectCredential() {
        return {
          hasCredential: true,
          backend: "environment" as const,
          userFilePath: "C:/user/credentials.json"
        };
      },
      async saveApiKey() {
        return "os-keyring" as const;
      },
      list() {
        return [];
      },
      async switch() {
        calls.push("legacy-config-switch");
        return "legacy";
      },
      getActiveModelSelection() {
        return ACTIVE;
      },
      async listModels() {
        calls.push("model.list");
        return [{selection: ACTIVE, availability: "active" as const}];
      },
      async switchModel(modelProfileId: string) {
        calls.push(`model.switch:${modelProfileId}`);
        return ACTIVE;
      },
      async switchProvider(providerId: string) {
        calls.push(`provider.compat:${providerId}`);
        return ACTIVE;
      }
    },
    sessionService: {
      async init() {
        return [];
      },
      async newThread() {
        return {thread: {id: "thread", status: "active"}, events: []};
      },
      async listThreads() {
        return [];
      },
      async resumeThread() {
        return {thread: {id: "thread", status: "active"}, events: []};
      }
    },
    turnEngine: {
      async *submit(): AsyncGenerator<RuntimeEvent> {},
      interrupt(): RuntimeEvent {
        return {type: "turn.interrupt.ignored", reason: "no_active_request"};
      },
      async compact() {
        return [];
      },
      async shutdown() {}
    },
    credentialStore: {
      createPlaintextConsent() {
        throw new Error("unused");
      }
    }
  };
  const kernel = new ApplicationKernel({services: services as never});
  await kernel.init();

  const listed = await collect(kernel.dispatch({type: "model.list"} as never));
  const switched = await collect(
    kernel.dispatch({
      type: "model.switch",
      modelProfileId: ACTIVE.modelProfileId
    } as never)
  );
  const compatible = await collect(
    kernel.dispatch({
      type: "provider.switch",
      providerId: "minimax-official"
    })
  );

  assert.equal(listed[0]?.type, "model.listed");
  assert.equal(switched[0]?.type, "model.changed");
  assert.equal(compatible[0]?.type, "model.changed");
  assert.deepEqual(calls, [
    "model.list",
    `model.switch:${ACTIVE.modelProfileId}`,
    "provider.compat:minimax-official"
  ]);
  assert.equal(calls.includes("legacy-config-switch"), false);
  assert.doesNotMatch(JSON.stringify([...listed, ...switched, ...compatible]), /secret|api[_-]?key/i);
});

async function collect(events: AsyncGenerator<RuntimeEvent>): Promise<RuntimeEvent[]> {
  const result: RuntimeEvent[] = [];
  for await (const event of events) {
    result.push(event);
  }
  return result;
}
