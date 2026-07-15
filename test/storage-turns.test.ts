import assert from "node:assert/strict";
import {mkdtemp, readFile, rm} from "node:fs/promises";
import {tmpdir} from "node:os";
import {join} from "node:path";
import test from "node:test";
import {JsonlStorageProvider} from "../src/storage/jsonl-storage.js";
import type {ThreadRecord, TurnRecord} from "../src/types.js";

const NOW = "2026-07-10T00:00:00.000Z";

test("terminal checkpoint keeps one versioned snapshot per Turn and retains its draft", async () => {
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
    await storage.appendTurnDelta({
      threadId: thread.id,
      turnId: running.id,
      delta: "partial ",
      createdAt: NOW
    });
    await storage.appendTurnDelta({
      threadId: thread.id,
      turnId: running.id,
      delta: "answer",
      createdAt: NOW
    });
    await storage.appendTurn({...running, status: "completed", completedAt: NOW});
    await storage.checkpointTurns(thread.id);

    const turns = await storage.readTurns(thread.id);
    const rawLines = (await readFile(join(root, "turns", `${thread.id}.turns.jsonl`), "utf8"))
      .trim()
      .split("\n");

    assert.equal(rawLines.length, 1);
    assert.equal((JSON.parse(rawLines[0] ?? "null") as {schemaVersion?: number}).schemaVersion, 1);
    assert.equal(turns.length, 1);
    assert.equal(turns[0]?.status, "completed");
    assert.equal(turns[0]?.assistantDraft, "partial answer");
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("a checkpoint cannot overwrite concurrently queued Turn deltas", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-turn-checkpoint-race-"));
  const storage = new JsonlStorageProvider(root);
  const thread: ThreadRecord = {
    id: "thread_1",
    title: "Checkpoint race",
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
  const deltas = Array.from({length: 20}, (_, index) => `[${index}]`);

  try {
    await storage.init();
    await storage.createThread(thread);
    await storage.appendTurn(running);

    const results = await Promise.allSettled([
      ...deltas.map((delta) =>
        storage.appendTurnDelta({
          threadId: thread.id,
          turnId: running.id,
          delta,
          createdAt: NOW
        })
      ),
      storage.checkpointTurns(thread.id)
    ]);

    assert.ok(results.every((result) => result.status === "fulfilled"));
    const turns = await storage.readTurns(thread.id);
    assert.equal(turns[0]?.assistantDraft, deltas.join(""));
    const lines = (await readFile(join(root, "turns", `${thread.id}.turns.jsonl`), "utf8"))
      .trim()
      .split("\n");
    assert.equal(lines.length, 1);
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});
