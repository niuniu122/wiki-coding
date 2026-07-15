import assert from "node:assert/strict";
import {readFile, mkdtemp, rm} from "node:fs/promises";
import {tmpdir} from "node:os";
import {join} from "node:path";
import test from "node:test";
import {DEFAULT_CONFIG} from "../src/config/config-manager.js";
import {SessionService} from "../src/runtime/session-service.js";
import {SafeTraceRecorder} from "../src/runtime/trace-recorder.js";
import {JsonlSessionRepository} from "../src/storage/jsonl-storage.js";
import type {ThreadRecord, TurnRecord} from "../src/types.js";

const NOW = "2026-07-10T00:00:00.000Z";

function thread(id: string, cwd: string, status: ThreadRecord["status"]): ThreadRecord {
  return {
    id,
    title: id,
    createdAt: NOW,
    updatedAt: NOW,
    model: DEFAULT_CONFIG.model,
    cwd,
    status
  };
}

test("session service restores one active thread and recovers running Turns", async () => {
  const cwd = await mkdtemp(join(tmpdir(), "minimax-session-service-recovery-"));
  const repository = new JsonlSessionRepository(join(cwd, ".mini-codex"));
  const staleTurn: TurnRecord = {
    id: "turn_stale",
    threadId: "thread_active",
    userInput: "unfinished question",
    status: "running",
    startedAt: NOW
  };

  try {
    await repository.init();
    await repository.createThread(thread("thread_active", cwd, "active"));
    await repository.appendTurnSnapshot(staleTurn);
    await repository.appendTurnDelta({
      threadId: staleTurn.threadId,
      turnId: staleTurn.id,
      delta: "saved partial reply",
      createdAt: NOW
    });

    const service = new SessionService(repository, new SafeTraceRecorder());
    const events = await service.init(DEFAULT_CONFIG.model, cwd);
    const snapshot = await repository.readThread("thread_active");

    assert.equal(events[0]?.type, "thread.loaded");
    assert.equal(events.some((event) => event.type === "turn.recovered"), true);
    assert.equal(service.activeThread.id, "thread_active");
    assert.equal(snapshot.turns[0]?.status, "interrupted");
    assert.equal(
      snapshot.items.some(
        (item) =>
          item.type === "assistant_message" &&
          item.content === "saved partial reply" &&
          item.metadata?.partial === true &&
          item.metadata?.interrupted === true
      ),
      true
    );
  } finally {
    await rm(cwd, {recursive: true, force: true});
  }
});

test("session service creates and resumes threads without Provider dependencies", async () => {
  const cwd = await mkdtemp(join(tmpdir(), "minimax-session-service-navigation-"));
  const repository = new JsonlSessionRepository(join(cwd, ".mini-codex"));

  try {
    await repository.init();
    const previous = thread("thread_previous", cwd, "active");
    await repository.createThread(previous);
    const service = new SessionService(repository);
    await service.init(DEFAULT_CONFIG.model, cwd);

    const created = await service.newThread("MiniMax-M3", cwd);
    const resumed = await service.resumeThread(previous.id);

    assert.equal(created.thread.id === resumed.thread.id, false);
    assert.equal(created.thread.model, "MiniMax-M3");
    assert.equal(resumed.thread.id, previous.id);
    assert.deepEqual(
      (await service.listThreads())
        .filter((entry) => entry.status === "active")
        .map((entry) => entry.id),
      [previous.id]
    );
  } finally {
    await rm(cwd, {recursive: true, force: true});
  }
});

test("session service source has no Provider dependency", async () => {
  const source = await readFile(
    new URL("../src/runtime/session-service.ts", import.meta.url),
    "utf8"
  );

  assert.doesNotMatch(source, /providers\/|provider-service/iu);
});
