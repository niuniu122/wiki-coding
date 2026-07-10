import assert from "node:assert/strict";
import {readFile} from "node:fs/promises";
import test from "node:test";

test("Ink App depends on CommandDispatcher and has no direct AgentRuntime workflow calls", async () => {
  const source = await readFile(new URL("../src/ui/App.tsx", import.meta.url), "utf8");

  assert.equal(source.includes("AgentRuntime"), false);
  assert.equal(source.includes("CommandDispatcher"), true);
  assert.equal(source.includes("dispatcher.dispatch"), true);

  for (const directCall of [
    "runtime.newThread(",
    "runtime.listThreads(",
    "runtime.resumeThread(",
    "runtime.interruptCurrentTurn(",
    "runtime.compact(",
    "runtime.submitUserInput(",
    "runtime.setApiKey(",
    "runtime.switchProvider("
  ]) {
    assert.equal(source.includes(directCall), false, `found forbidden UI call: ${directCall}`);
  }
});
