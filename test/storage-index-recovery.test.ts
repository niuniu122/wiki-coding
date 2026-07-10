import assert from "node:assert/strict";
import {mkdtemp, rm, writeFile} from "node:fs/promises";
import {tmpdir} from "node:os";
import {join} from "node:path";
import test from "node:test";
import {JsonlStorageProvider} from "../src/storage/jsonl-storage.js";
import type {ThreadRecord} from "../src/types.js";

test("thread index restores the previous valid snapshot after corruption", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-index-recovery-"));
  const storage = new JsonlStorageProvider(root);
  const thread: ThreadRecord = {
    id: "thread_1",
    title: "original title",
    createdAt: "2026-07-10T00:00:00.000Z",
    updatedAt: "2026-07-10T00:00:00.000Z",
    model: "MiniMax-M3",
    cwd: root,
    status: "active"
  };

  try {
    await storage.init();
    await storage.createThread(thread);
    await storage.updateThread({...thread, title: "new title"});
    await writeFile(join(root, "indexes", "threads.json"), '{"threads":"broken"}', "utf8");

    const restarted = new JsonlStorageProvider(root);
    await restarted.init();
    const threads = await restarted.listThreads();

    assert.equal(threads.length, 1);
    assert.equal(threads[0]?.id, "thread_1");
    assert.equal(threads[0]?.title, "original title");
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});
