import assert from "node:assert/strict";
import {access, mkdir, mkdtemp, readFile, rm, writeFile} from "node:fs/promises";
import {tmpdir} from "node:os";
import {join} from "node:path";
import test from "node:test";
import {JsonlStorageProvider} from "../src/storage/jsonl-storage.js";
import type {ThreadItem, ThreadRecord} from "../src/types.js";
import {writeJsonFile, type WriteJsonFileOptions} from "../src/utils/jsonl.js";

const NOW = "2026-07-10T00:00:00.000Z";

function threadFor(root: string): ThreadRecord {
  return {
    id: "thread_1",
    title: "Versioned storage",
    createdAt: NOW,
    updatedAt: NOW,
    model: "MiniMax-M2.1",
    cwd: root,
    status: "active"
  };
}

function itemFor(content: string, id = "item_1"): ThreadItem {
  return {
    id,
    threadId: "thread_1",
    turnId: "turn_1",
    type: "user_message",
    role: "user",
    content,
    createdAt: NOW
  };
}

function sessionPath(root: string): string {
  return join(root, "sessions", "2026", "07", "10", "thread_1.jsonl");
}

async function writeLegacyWorkspace(root: string, records: unknown[]): Promise<string> {
  const path = sessionPath(root);
  await mkdir(join(root, "indexes"), {recursive: true});
  await mkdir(join(root, "sessions", "2026", "07", "10"), {recursive: true});
  await writeFile(
    join(root, "indexes", "threads.json"),
    `${JSON.stringify({threads: [threadFor(root)]}, null, 2)}\n`,
    "utf8"
  );
  await writeFile(path, records.map((record) => JSON.stringify(record)).join("\n") + "\n", "utf8");
  return path;
}

test("new records are versioned and legacy records remain readable", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-storage-version-"));
  const storage = new JsonlStorageProvider(root);
  const legacyItem = itemFor("legacy", "item_legacy");
  const newItem = itemFor("new", "item_new");

  try {
    await storage.init();
    await storage.createThread(threadFor(root));
    await mkdir(join(root, "sessions", "2026", "07", "10"), {recursive: true});
    await writeFile(sessionPath(root), `${JSON.stringify(legacyItem)}\n`, "utf8");

    await storage.appendItem(newItem);

    const raw = await readFile(sessionPath(root), "utf8");
    assert.match(raw, /"schemaVersion":1/);
    assert.deepEqual(await storage.readThreadItems("thread_1"), [legacyItem, newItem]);
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("concurrent appends allocate one strictly increasing sequence per file", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-storage-concurrent-"));
  const storage = new JsonlStorageProvider(root);
  const items = Array.from({length: 24}, (_, index) =>
    itemFor(`concurrent ${index}`, `item_${String(index).padStart(2, "0")}`)
  );

  try {
    await storage.init();
    await storage.createThread(threadFor(root));

    await Promise.all(items.map((item) => storage.appendItem(item)));

    const envelopes = (await readFile(sessionPath(root), "utf8"))
      .trim()
      .split("\n")
      .map((line) => JSON.parse(line) as {sequence: number});
    assert.deepEqual(
      envelopes.map((envelope) => envelope.sequence),
      Array.from({length: items.length}, (_, index) => index + 1)
    );
    assert.deepEqual(
      (await storage.readThreadItems("thread_1")).map((item) => item.id).sort(),
      items.map((item) => item.id).sort()
    );
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("legacy startup migrates JSONL only after validation and retains byte-exact backups", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-storage-migration-"));
  const legacyItem = itemFor("legacy");

  try {
    const path = await writeLegacyWorkspace(root, [legacyItem]);
    const original = await readFile(path, "utf8");
    const storage = new JsonlStorageProvider(root);

    await storage.init();

    const manifest = JSON.parse(await readFile(join(root, "manifest.json"), "utf8")) as {
      schemaVersion: number;
      storage: string;
    };
    assert.deepEqual(manifest.schemaVersion, 1);
    assert.equal(manifest.storage, "jsonl");
    assert.equal(await readFile(`${path}.v0.bak`, "utf8"), original);
    assert.match(await readFile(path, "utf8"), /"schemaVersion":1/);
    assert.deepEqual(await storage.readThreadItems("thread_1"), [legacyItem]);
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("structurally invalid legacy records stop migration before mutation", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-storage-invalid-"));
  const invalidItem = {...itemFor("invalid")};
  delete (invalidItem as Partial<ThreadItem>).content;

  try {
    const path = await writeLegacyWorkspace(root, [invalidItem]);
    const original = await readFile(path, "utf8");
    const storage = new JsonlStorageProvider(root);

    await assert.rejects(storage.init(), /invalid.*thread item|structurally invalid/i);

    assert.equal(await readFile(path, "utf8"), original);
    await assert.rejects(access(`${path}.v0.bak`));
    await assert.rejects(access(join(root, "manifest.json")));
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("legacy migration fails closed on a truncated final JSONL record", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-storage-truncated-tail-"));
  const legacyItem = itemFor("legacy before truncated tail");

  try {
    const path = await writeLegacyWorkspace(root, [legacyItem]);
    const original = `${await readFile(path, "utf8")}{"content":"SECRET_TRUNCATED_TAIL"`;
    await writeFile(path, original, "utf8");
    const storage = new JsonlStorageProvider(root);

    await assert.rejects(
      storage.init(),
      (error: unknown) =>
        error instanceof Error &&
        /corruption/i.test(error.message) &&
        /repair/i.test(error.message) &&
        !error.message.includes("SECRET_TRUNCATED_TAIL")
    );

    assert.equal(await readFile(path, "utf8"), original);
    await assert.rejects(access(`${path}.v0.bak`));
    await assert.rejects(access(join(root, "manifest.json")));
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("migration commit failure restores the legacy file and leaves no manifest", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-storage-rollback-"));
  const legacyItem = itemFor("legacy");

  try {
    const path = await writeLegacyWorkspace(root, [legacyItem]);
    const original = await readFile(path, "utf8");
    await mkdir(`${path}.v0.bak`);
    const storage = new JsonlStorageProvider(root);

    await assert.rejects(storage.init());

    assert.equal(await readFile(path, "utf8"), original);
    await assert.rejects(access(join(root, "manifest.json")));
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("rollback after one installed replacement retains canonical legacy data and its backup", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-storage-mid-rollback-"));
  const legacyItem = itemFor("legacy");

  try {
    const session = await writeLegacyWorkspace(root, [legacyItem]);
    const sessionRaw = await readFile(session, "utf8");
    const history = join(root, "history", "input-history.jsonl");
    const historyRaw = `${JSON.stringify({
      threadId: "thread_1",
      turnId: "turn_1",
      text: "legacy",
      ts: NOW
    })}\n`;
    await mkdir(join(root, "history"), {recursive: true});
    await writeFile(history, historyRaw, "utf8");
    await mkdir(`${session}.v0.bak`);
    const storage = new JsonlStorageProvider(root);

    await assert.rejects(storage.init());

    assert.equal(await readFile(history, "utf8"), historyRaw);
    assert.equal(await readFile(`${history}.v0.bak`, "utf8"), historyRaw);
    assert.equal(await readFile(session, "utf8"), sessionRaw);
    await assert.rejects(access(join(root, "manifest.json")));
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("manifest write failure restores canonical legacy data while retaining its backup", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-storage-manifest-rollback-"));
  const legacyItem = itemFor("legacy");

  try {
    const path = await writeLegacyWorkspace(root, [legacyItem]);
    const original = await readFile(path, "utf8");
    const manifestPath = join(root, "manifest.json");
    const storage = new JsonlStorageProvider(root, {
      writeJsonFile: async <T>(
        filePath: string,
        value: T,
        options: WriteJsonFileOptions = {}
      ): Promise<void> => {
        if (filePath === manifestPath) {
          throw new Error("injected manifest write failure");
        }
        await writeJsonFile(filePath, value, options);
      }
    });

    await assert.rejects(storage.init(), /injected manifest write failure/i);

    assert.equal(await readFile(path, "utf8"), original);
    assert.equal(await readFile(`${path}.v0.bak`, "utf8"), original);
    await assert.rejects(access(manifestPath));
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("startup recovers the legacy backup left by an interrupted pre-manifest migration", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-storage-interrupted-"));
  const legacyItem = itemFor("legacy authority", "legacy_authority");

  try {
    const path = await writeLegacyWorkspace(root, [legacyItem]);
    const original = await readFile(path, "utf8");
    await writeFile(`${path}.v0.bak`, original, "utf8");
    await writeFile(
      path,
      `${JSON.stringify({
        schemaVersion: 1,
        sequence: 1,
        kind: "thread.item",
        payload: itemFor("uncommitted replacement", "uncommitted"),
        createdAt: NOW
      })}\n`,
      "utf8"
    );
    const storage = new JsonlStorageProvider(root);

    await storage.init();

    assert.deepEqual(await storage.readThreadItems("thread_1"), [legacyItem]);
    assert.equal(await readFile(`${path}.v0.bak`, "utf8"), original);
    assert.equal(
      (JSON.parse(await readFile(join(root, "manifest.json"), "utf8")) as {schemaVersion: number})
        .schemaVersion,
      1
    );
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("an unknown manifest version fails before creating or migrating storage paths", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-storage-manifest-"));

  try {
    await writeFile(
      join(root, "manifest.json"),
      `${JSON.stringify({schemaVersion: 2, storage: "jsonl", createdAt: NOW})}\n`,
      "utf8"
    );
    const storage = new JsonlStorageProvider(root);

    await assert.rejects(storage.init(), /unknown.*manifest.*2|manifest.*version.*2/i);
    await assert.rejects(access(join(root, "sessions")));
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("unknown envelope versions fail startup closed", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-storage-envelope-version-"));
  const storage = new JsonlStorageProvider(root);

  try {
    await storage.init();
    await storage.createThread(threadFor(root));
    await mkdir(join(root, "sessions", "2026", "07", "10"), {recursive: true});
    await writeFile(
      sessionPath(root),
      `${JSON.stringify({
        schemaVersion: 99,
        sequence: 1,
        kind: "thread.item",
        payload: itemFor("future"),
        createdAt: NOW
      })}\n`,
      "utf8"
    );

    const restarted = new JsonlStorageProvider(root);
    await assert.rejects(restarted.init(), /unknown.*schema.*99/i);
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("non-monotonic versioned sequences fail startup closed", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-storage-sequence-"));
  const storage = new JsonlStorageProvider(root);

  try {
    await storage.init();
    await storage.createThread(threadFor(root));
    await mkdir(join(root, "sessions", "2026", "07", "10"), {recursive: true});
    const envelope = (sequence: number, id: string) => ({
      schemaVersion: 1,
      sequence,
      kind: "thread.item",
      payload: itemFor(id, id),
      createdAt: NOW
    });
    await writeFile(
      sessionPath(root),
      `${JSON.stringify(envelope(2, "first"))}\n${JSON.stringify(envelope(1, "second"))}\n`,
      "utf8"
    );

    const restarted = new JsonlStorageProvider(root);
    await assert.rejects(restarted.init(), /non-monotonic.*sequence/i);
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("versioned payloads receive domain validation during startup", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-storage-payload-"));
  const storage = new JsonlStorageProvider(root);
  const invalidItem = {...itemFor("invalid")};
  delete (invalidItem as Partial<ThreadItem>).content;

  try {
    await storage.init();
    await storage.createThread(threadFor(root));
    await mkdir(join(root, "sessions", "2026", "07", "10"), {recursive: true});
    await writeFile(
      sessionPath(root),
      `${JSON.stringify({
        schemaVersion: 1,
        sequence: 1,
        kind: "thread.item",
        payload: invalidItem,
        createdAt: NOW
      })}\n`,
      "utf8"
    );

    const restarted = new JsonlStorageProvider(root);
    await assert.rejects(restarted.init(), /invalid.*thread item|structurally invalid/i);
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("manifest-backed storage repairs only an interrupted final record and remains appendable", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-storage-v1-tail-repair-"));
  const storage = new JsonlStorageProvider(root);
  const first = itemFor("before interrupted tail", "item_before_tail");
  const after = itemFor("after repair", "item_after_repair");

  try {
    await storage.init();
    await storage.createThread(threadFor(root));
    await storage.appendItem(first);
    const path = sessionPath(root);
    const authoritative = await readFile(path, "utf8");
    await writeFile(path, `${authoritative}{"schemaVersion":1`, "utf8");

    const restarted = new JsonlStorageProvider(root);
    await restarted.init();
    assert.equal(await readFile(path, "utf8"), authoritative);
    await restarted.appendItem(after);
    assert.deepEqual(await restarted.readThreadItems("thread_1"), [first, after]);
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("manifest-backed storage still rejects middle JSONL corruption", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-storage-v1-middle-corruption-"));
  const storage = new JsonlStorageProvider(root);

  try {
    await storage.init();
    await storage.createThread(threadFor(root));
    await storage.appendItem(itemFor("valid", "item_valid"));
    const path = sessionPath(root);
    const valid = await readFile(path, "utf8");
    await writeFile(path, `${valid}{not-json}\n${valid}`, "utf8");

    await assert.rejects(new JsonlStorageProvider(root).init(), /corruption.*line 2/i);
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});
