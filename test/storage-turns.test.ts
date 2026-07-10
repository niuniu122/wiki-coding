import assert from "node:assert/strict";
import {mkdtemp, rm} from "node:fs/promises";
import {tmpdir} from "node:os";
import {join} from "node:path";
import test from "node:test";
import {JsonlStorageProvider} from "../src/storage/jsonl-storage.js";
import type {ThreadRecord, TurnRecord} from "../src/types.js";

const NOW = "2026-07-10T00:00:00.000Z";

test("turn snapshots and assistant deltas replay into the latest durable turn state", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-turn-storage-"));
  const storage = new JsonlStorageProvider(root);
  const thread: ThreadRecord = {
    id: "thread_1",
    title: "Turn storage",
    createdAt: NOW,
    updatedAt: NOW,
    model: "MiniMax-M2.1",
    cwd: root,
    status: "active"
  };
  const running: TurnRecord = {
    id: "turn_1",
    threadId: thread.id,
    userInput: "question",
    status: "running",
    startedAt: NOW
  };

  try {
    await storage.init();
    await storage.createThread(thread);
    await storage.appendTurn(running);
    await storage.appendTurnDelta(thread.id, running.id, "partial ", NOW);
    await storage.appendTurnDelta(thread.id, running.id, "answer", NOW);
    await storage.appendTurn({...running, status: "interrupted", completedAt: NOW});

    const turns = await storage.readTurns(thread.id);

    assert.equal(turns.length, 1);
    assert.equal(turns[0]?.status, "interrupted");
    assert.equal(turns[0]?.assistantDraft, "partial answer");
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});
