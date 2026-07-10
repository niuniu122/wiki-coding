import assert from "node:assert/strict";
import test from "node:test";
import {classifyChatInput} from "../src/ui/chat-input-policy.js";

test("chat input parser emits Commands and only allows interrupt while busy", () => {
  assert.deepEqual(classifyChatInput(" /interrupt ", true), {
    type: "command",
    command: {type: "turn.interrupt"}
  });
  assert.deepEqual(classifyChatInput("second question", true), {type: "busy"});
  assert.deepEqual(classifyChatInput("second question", false), {
    type: "command",
    command: {type: "turn.submit", input: "second question"}
  });
  assert.deepEqual(classifyChatInput("   ", false), {type: "empty"});
});

test("chat input parser covers every supported slash command", () => {
  const cases = [
    ["/new", {type: "thread.new"}],
    ["/threads", {type: "thread.list"}],
    ["/resume thread_123", {type: "thread.resume", threadId: "thread_123"}],
    ["/interrupt", {type: "turn.interrupt"}],
    ["/compact", {type: "compact.manual"}],
    ["/api", {type: "config.api_key.request"}],
    ["/provider", {type: "provider.list"}],
    ["/provider minimax-official", {type: "provider.switch", providerId: "minimax-official"}],
    ["/trace", {type: "trace.toggle"}],
    ["/exit", {type: "app.exit"}],
    ["/quit", {type: "app.exit"}]
  ] as const;

  for (const [input, command] of cases) {
    assert.deepEqual(classifyChatInput(input, false), {type: "command", command});
  }

  assert.deepEqual(classifyChatInput("/resume", false), {
    type: "invalid",
    message: "用法：/resume <threadId>；先输入 /threads 查看 ID"
  });
});
