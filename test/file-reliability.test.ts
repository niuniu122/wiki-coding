import assert from "node:assert/strict";
import {mkdtemp, readFile, rm, writeFile} from "node:fs/promises";
import {tmpdir} from "node:os";
import {join} from "node:path";
import test from "node:test";
import {ModelStateStore} from "../src/config/model-state-store.js";
import {
  appendJsonl,
  readJsonFile,
  readJsonl,
  writeJsonFile
} from "../src/utils/jsonl.js";

function isVersionFile(value: unknown): value is {version: number} {
  return (
    typeof value === "object" &&
    value !== null &&
    typeof (value as {version?: unknown}).version === "number"
  );
}

test("JSON files recover the last valid backup and restore the primary file", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-json-recovery-"));
  const path = join(root, "config.json");

  try {
    await writeJsonFile(path, {version: 1});
    await writeJsonFile(path, {version: 2});
    await writeFile(path, "{\"version\":", "utf8");

    const recovered = await readJsonFile(path, {version: 0}, {validate: isVersionFile});
    const restored = JSON.parse(await readFile(path, "utf8")) as {version: number};

    assert.deepEqual(recovered, {version: 1});
    assert.deepEqual(restored, {version: 1});
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("JSON files fail clearly when neither primary nor backup is valid", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-json-corrupt-"));
  const path = join(root, "config.json");

  try {
    await writeFile(path, "{broken", "utf8");
    await writeFile(`${path}.bak`, "[]", "utf8");

    await assert.rejects(
      readJsonFile(path, {version: 0}, {validate: isVersionFile}),
      /no valid primary or backup/i
    );
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("model state recovers its last valid backup and restores the primary", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-model-state-recovery-"));
  const store = new ModelStateStore({userConfigDir: root});

  try {
    await store.save("model:minimax/minimax-text-01");
    await store.save("model:openai/gpt-5");
    await writeFile(store.statePath, "{broken", "utf8");

    assert.deepEqual(await store.load(), {
      status: "selected",
      state: {
        schemaVersion: 1,
        lastSelectedModelProfileId: "model:minimax/minimax-text-01"
      }
    });
    assert.deepEqual(JSON.parse(await readFile(store.statePath, "utf8")), {
      schemaVersion: 1,
      lastSelectedModelProfileId: "model:minimax/minimax-text-01"
    });
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("JSONL repairs only an interrupted final record and remains appendable", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-jsonl-tail-"));
  const path = join(root, "turns.jsonl");

  try {
    await writeFile(path, '{"id":1}\n{"id":', "utf8");
    const recovered = await readJsonl<{id: number}>(path);
    await appendJsonl(path, {id: 2});
    const afterAppend = await readJsonl<{id: number}>(path);

    assert.deepEqual(recovered, [{id: 1}]);
    assert.deepEqual(afterAppend, [{id: 1}, {id: 2}]);
    assert.equal(await readFile(path, "utf8"), '{"id":1}\n{"id":2}\n');
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("JSONL reports middle corruption instead of silently dropping history", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-jsonl-middle-"));
  const path = join(root, "turns.jsonl");

  try {
    await writeFile(path, '{"id":1}\n{broken}\n{"id":3}\n', "utf8");
    await assert.rejects(readJsonl(path), /line 2/i);
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});
