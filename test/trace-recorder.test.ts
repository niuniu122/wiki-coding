import assert from "node:assert/strict";
import test from "node:test";
import {SafeTraceRecorder} from "../src/runtime/trace-recorder.js";

test("trace recorder builds messages from known codes and drops non-whitelisted facts", () => {
  const recorder = new SafeTraceRecorder();
  const event = recorder.create("thread_1", "provider.reasoning.filtered", {
    turnId: "turn_1",
    providerId: "hashsight",
    hiddenCharacters: 42,
    rawReasoning: "DO_NOT_PERSIST_THIS",
    apiKey: "DO_NOT_PERSIST_THIS_KEY"
  });

  assert.deepEqual(event, {
    id: event.id,
    threadId: "thread_1",
    turnId: "turn_1",
    category: "provider",
    code: "provider.reasoning.filtered",
    message: "模型返回的隐藏推理已过滤，不会写入聊天或持久化 trace。",
    createdAt: event.createdAt,
    facts: {providerId: "hashsight", hiddenCharacters: 42}
  });
  assert.equal(JSON.stringify(event).includes("DO_NOT_PERSIST_THIS"), false);
});
