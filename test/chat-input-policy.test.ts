import assert from "node:assert/strict";
import test from "node:test";
import {
  classifyChatInput,
  classifyPlaintextConfirmation,
  classifyUiInput
} from "../src/ui/chat-input-policy.js";
import {initialUiState, reduceRuntimeEvent} from "../src/ui/ui-state.js";

test("chat input parser emits Commands without owning core concurrency", () => {
  assert.deepEqual(classifyChatInput(" /interrupt "), {
    type: "command",
    command: {type: "turn.interrupt"}
  });
  assert.deepEqual(classifyChatInput("second question"), {
    type: "command",
    command: {type: "turn.submit", input: "second question"}
  });
  assert.deepEqual(classifyChatInput("   "), {type: "empty"});
});

test("explicit Agent and chat routes do not change ordinary input routing", () => {
  assert.deepEqual(classifyChatInput("ordinary request"), {type: "command", command: {type: "turn.submit", input: "ordinary request"}});
  assert.deepEqual(classifyChatInput("/agent inspect project"), {type: "command", command: {type: "agent.submit", input: "inspect project"}});
  assert.deepEqual(classifyChatInput("/chat explain this"), {type: "command", command: {type: "turn.submit", input: "explain this"}});
  assert.equal(classifyChatInput("/agent").type, "invalid");
  assert.equal(classifyChatInput("/chat").type, "invalid");
});

test("UI enables continue only after a recoverable checkpoint event", () => {
  const ready = reduceRuntimeEvent(reduceRuntimeEvent(initialUiState(), {type: "runtime.ready", hasApiKey: true, providerSummary: "test", recoveredTurns: 0}), {type: "thread.loaded", thread: {id: "thread-1", title: "t", createdAt: "2026-07-14T00:00:00.000Z", updatedAt: "2026-07-14T00:00:00.000Z", model: "m", cwd: "C:/workspace", status: "active"}});
  assert.equal(classifyUiInput(ready, "/continue").type, "invalid");
  const recoverable = reduceRuntimeEvent(ready, {type: "agent.recovery.available", turnId: "turn-1", checkpointId: "checkpoint-1"});
  assert.deepEqual(classifyUiInput(recoverable, "/continue"), {type: "command", command: {type: "agent.continue"}});
  const resumed = reduceRuntimeEvent(recoverable, {type: "agent.continued", turnId: "turn-1", checkpointId: "checkpoint-1"});
  assert.equal(classifyUiInput(resumed, "/continue").type, "invalid");
});

test("chat input parser covers every supported slash command", () => {
  const cases = [
    ["/new", {type: "thread.new"}],
    ["/threads", {type: "thread.list"}],
    ["/resume thread_123", {type: "thread.resume", threadId: "thread_123"}],
    ["/interrupt", {type: "turn.interrupt"}],
    ["/compact", {type: "compact.manual"}],
    ["/continue", {type: "agent.continue"}],
    ["/api", {type: "config.api_key.request"}],
    ["/provider", {type: "provider.list"}],
    ["/provider minimax-official", {type: "provider.switch", providerId: "minimax-official"}],
    ["/trace", {type: "trace.toggle"}],
    ["/exit", {type: "app.exit"}],
    ["/quit", {type: "app.exit"}]
  ] as const;

  for (const [input, command] of cases) {
    assert.deepEqual(classifyChatInput(input), {type: "command", command});
  }

  const invalidResume = classifyChatInput("/resume");
  assert.equal(invalidResume.type, "invalid");
  if (invalidResume.type === "invalid") {
    assert.match(invalidResume.message, /resume <threadId>/);
  }
});

test("chat input parser gives unknown slash commands unified feedback", () => {
  const action = classifyChatInput("/not-a-command");

  assert.equal(action.type, "invalid");
  if (action.type === "invalid") {
    assert.match(action.message, /unknown|未知/i);
  }
});

test("plaintext confirmation accepts only explicit YES", () => {
  assert.deepEqual(classifyPlaintextConfirmation(" YES "), {
    type: "command",
    command: {type: "config.api_key.plaintext.confirm"}
  });

  for (const input of ["yes", "Y", "NO", "", "YES please"]) {
    assert.deepEqual(classifyPlaintextConfirmation(input), {type: "cancel"});
  }
});

test("fresh missing-key state preserves slash routing including exit", () => {
  const state = reduceRuntimeEvent(initialUiState(), {
    type: "runtime.ready",
    hasApiKey: false,
    providerSummary: "minimax-official",
    recoveredTurns: 0
  });

  assert.equal(classifyUiInput(state, "hello").type, "invalid");
  for (const input of [
    " /api ",
    "/provider",
    "/provider minimax-official",
    "/exit",
    "/quit",
    "/threads",
    "/not-a-command"
  ]) {
    assert.deepEqual(classifyUiInput(state, input), classifyChatInput(input));
  }
  assert.deepEqual(classifyUiInput(state, "/exit"), {
    type: "command",
    command: {type: "app.exit"}
  });
});

test("provider without a key can switch back through the slash parser", () => {
  const ready = reduceRuntimeEvent(initialUiState(), {
    type: "runtime.ready",
    hasApiKey: true,
    providerSummary: "minimax-official",
    recoveredTurns: 0
  });
  const missingProvider = reduceRuntimeEvent(ready, {
    type: "provider.changed",
    summary: "hashsight",
    hasApiKey: false
  });

  assert.equal(missingProvider.inputMode, "api_setup_required");
  assert.deepEqual(classifyUiInput(missingProvider, "/provider minimax-official"), {
    type: "command",
    command: {type: "provider.switch", providerId: "minimax-official"}
  });
  assert.equal(classifyUiInput(missingProvider, "ordinary prompt").type, "invalid");
});

test("booting chat state cannot create a turn command", () => {
  const action = classifyUiInput(initialUiState(), "hello before ready");

  assert.equal(action.type, "invalid");
});

test("initialization recovery accepts only retry and exit", () => {
  const failed = reduceRuntimeEvent(initialUiState(), {
    type: "runtime.init_failed",
    message: "failed"
  });

  assert.deepEqual(classifyUiInput(failed, " /retry "), {type: "retry_init"});
  assert.deepEqual(classifyUiInput(failed, "/exit"), {
    type: "command",
    command: {type: "app.exit"}
  });
  for (const input of ["hello", "/new", "/api", "/threads", "/quit"]) {
    assert.equal(classifyUiInput(failed, input).type, "invalid", input);
  }
});
