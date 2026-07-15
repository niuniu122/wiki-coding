import assert from "node:assert/strict";
import test from "node:test";
import {classifyChatInput, classifyUiInput} from "../src/ui/chat-input-policy.js";
import {initialUiState, reduceRuntimeEvent} from "../src/ui/ui-state.js";

test("ordinary input changes route only when the gated default-route flag is effective", () => {
  assert.deepEqual(classifyChatInput("inspect"), {type: "command", command: {type: "turn.submit", input: "inspect"}});
  assert.deepEqual(classifyChatInput("inspect", {agentDefaultRoute: true}), {type: "command", command: {type: "agent.submit", input: "inspect"}});
  assert.deepEqual(classifyChatInput("/chat inspect", {agentDefaultRoute: true}), {type: "command", command: {type: "turn.submit", input: "inspect"}});
});

test("runtime feature diagnostics drive UI routing without migrating any session state", () => {
  const ready = reduceRuntimeEvent(initialUiState(), {
    type: "runtime.ready",
    hasApiKey: true,
    providerSummary: "test",
    recoveredTurns: 0,
    features: {capabilityCatalog: true, capabilityEmbedding: false, agentExecution: true, agentDefaultRoute: true, diagnostics: ["embedding_disabled"]}
  });
  assert.equal(ready.agentDefaultRoute, true);
  assert.deepEqual(classifyUiInput(ready, "inspect"), {type: "command", command: {type: "agent.submit", input: "inspect"}});
  assert.deepEqual(classifyUiInput(ready, "/chat inspect"), {type: "command", command: {type: "turn.submit", input: "inspect"}});
  assert.equal(ready.recoverableAgentTurnId, null);
});

test("disabled or legacy runtime flags preserve the chat route", () => {
  const legacyReady = reduceRuntimeEvent(initialUiState(), {type: "runtime.ready", hasApiKey: true, providerSummary: "legacy", recoveredTurns: 0});
  assert.equal(legacyReady.agentDefaultRoute, false);
  assert.deepEqual(classifyUiInput(legacyReady, "hello"), {type: "command", command: {type: "turn.submit", input: "hello"}});
});
