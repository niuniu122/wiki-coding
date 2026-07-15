import assert from "node:assert/strict";
import {mkdtemp, rm} from "node:fs/promises";
import {tmpdir} from "node:os";
import {join} from "node:path";
import test from "node:test";
import {DEFAULT_CONFIG} from "../src/config/config-manager.js";
import {ContextEngine} from "../src/runtime/context-engine.js";
import {SessionService} from "../src/runtime/session-service.js";
import {JsonlStorageProvider} from "../src/storage/jsonl-storage.js";

test("ordered AgentItems round-trip inside the existing Turn truth source", async () => {
  const root = await mkdtemp(join(tmpdir(), "agent-items-"));
  try {
    const repository = new JsonlStorageProvider(root);
    const session = new SessionService(repository);
    await session.init(DEFAULT_CONFIG.model, root);
    const turn = await session.createTurn("inspect project");
    const items = [
      session.createAgentItem({turnId: turn.id, sequence: 0, payload: {kind: "user", text: "inspect project"}}),
      session.createAgentItem({turnId: turn.id, sequence: 1, payload: {kind: "assistant", text: "I will read a file"}}),
      session.createAgentItem({turnId: turn.id, sequence: 2, payload: {kind: "tool_request", invocationId: "inv-1", capabilityId: "capability:minimax/read-file", arguments: {path: "README.md"}}}),
      session.createAgentItem({turnId: turn.id, sequence: 3, payload: {kind: "tool_result", invocationId: "inv-1", status: "completed", output: "private-large-tool-output"}}),
      session.createAgentItem({turnId: turn.id, sequence: 4, payload: {kind: "checkpoint", checkpointId: "cp-1", turnId: turn.id, lastSequence: 3, continuationGeneration: 0, snapshotVersion: "snapshot-1", modelProfileId: "model:minimax/official/MiniMax-M3", step: 1, toolCalls: 1, tokensUsed: 20, remainingToolCalls: 2}}),
      session.createAgentItem({turnId: turn.id, sequence: 5, payload: {kind: "final", text: "done"}})
    ];
    for (const item of items) await repository.appendItem(item);
    await session.completeTurn(turn, "completed");
    const snapshot = await repository.readThread(session.activeThread.id);
    assert.deepEqual(snapshot.items.map((item) => item.agent?.sequence), [0, 1, 2, 3, 4, 5]);
    assert.equal(snapshot.items[2]?.agent?.payload.kind, "tool_request");
    assert.equal(snapshot.items[3]?.agent?.payload.kind, "tool_result");
    const context = new ContextEngine().build({config: DEFAULT_CONFIG, items: snapshot.items, summaries: [], currentUserInput: "next"});
    assert.doesNotMatch(JSON.stringify(context.messages), /private-large-tool-output/);
  } finally { await rm(root, {recursive: true, force: true}); }
});
