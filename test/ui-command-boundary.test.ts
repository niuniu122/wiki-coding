import assert from "node:assert/strict";
import {readFile} from "node:fs/promises";
import test from "node:test";

test("App contains no RuntimeEvent branch table or command concurrency policy", async () => {
  const source = await readFile(new URL("../src/ui/App.tsx", import.meta.url), "utf8");

  assert.equal(source.includes("AgentRuntime"), false);
  assert.equal(source.includes("CommandDispatcher"), true);
  assert.equal(source.includes("dispatcher.dispatch"), true);
  assert.equal(source.includes("function applyRuntimeEvent"), false);
  assert.equal(source.includes("function isBlockingCommand"), false);
  assert.equal(source.includes("event.type ==="), false);
  assert.equal(source.includes("classifyUiInput"), true);
  assert.equal(source.includes('state.inputMode === "api_setup_required"'), true);
  assert.equal(source.includes("CapabilityDispatcher"), false);
  assert.equal(source.includes("WorkspaceReadExecutor"), false);
  assert.equal(source.includes("NpmDiagnosticExecutor"), false);

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

test("README describes the current kernel and provider architecture", async () => {
  const source = await readFile(new URL("../README.md", import.meta.url), "utf8");

  assert.equal(source.includes("AgentRuntime"), false);
  assert.equal(source.includes("supportsThinkTags"), false);
  assert.match(source, /ApplicationKernel/);
  assert.match(source, /StrictProviderGateway/);
});

test("dead storage and compatibility entry points are removed after the import scan", async () => {
  const [types, config] = await Promise.all([
    readFile(new URL("../src/types.ts", import.meta.url), "utf8"),
    readFile(new URL("../src/config/config-manager.ts", import.meta.url), "utf8")
  ]);

  assert.equal(types.includes("supportsThinkTags"), false);
  assert.equal(config.includes("supportsThinkTags"), false);
  for (const unusedKind of [
    "context_summary",
    "compaction",
    "api_request",
    "api_response"
  ]) {
    assert.equal(types.includes(unusedKind), false, `found dead item kind: ${unusedKind}`);
  }

  await assert.rejects(readFile(new URL("../src/storage/sqlite-storage.ts", import.meta.url)));
  await assert.rejects(readFile(new URL("../src/runtime/agent-runtime.ts", import.meta.url)));
});

test("CLI entrypoint is import-safe and App releases its dispatcher on unmount", async () => {
  const [cli, app, coordinator] = await Promise.all([
    readFile(new URL("../src/cli.tsx", import.meta.url), "utf8"),
    readFile(new URL("../src/ui/App.tsx", import.meta.url), "utf8"),
    readFile(new URL("../src/runtime/shutdown-coordinator.ts", import.meta.url), "utf8")
  ]);

  assert.equal(cli.includes("render(<App />);"), false);
  assert.match(coordinator, /SIGINT/);
  assert.match(coordinator, /SIGTERM/);
  assert.match(app, /dispatcher\.shutdown\("user"\)/);
});
