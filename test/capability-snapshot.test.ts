import assert from "node:assert/strict";
import {mkdtemp, readFile, rm, writeFile} from "node:fs/promises";
import {tmpdir} from "node:os";
import {join} from "node:path";
import test from "node:test";
import {createCapabilitySnapshot} from "../src/capabilities/capability-snapshot.js";
import {CapabilitySnapshotStore} from "../src/capabilities/snapshot-store.js";

test("snapshot store publishes complete immutable metadata and recovers its backup", async () => {
  const root = await mkdtemp(join(tmpdir(), "capability-snapshot-"));
  try {
    const store = new CapabilitySnapshotStore(root);
    const first = createCapabilitySnapshot([], {version: "v1", now: "2026-07-14T00:00:00.000Z"});
    const second = createCapabilitySnapshot([], {version: "v2", now: "2026-07-14T00:00:01.000Z"});
    await store.save(first);
    await store.save(second);
    await writeFile(store.path, "{broken");
    const loaded = await store.load();
    assert.equal(loaded.status, "loaded");
    if (loaded.status === "loaded") assert.equal(loaded.snapshot.version, "v1");
    assert.match(await readFile(store.path, "utf8"), /"version": "v1"/);
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});
