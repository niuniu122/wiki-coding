import assert from "node:assert/strict";
import {mkdtemp, rm} from "node:fs/promises";
import {tmpdir} from "node:os";
import {join} from "node:path";
import test from "node:test";
import {createCapabilitySnapshot} from "../src/capabilities/capability-snapshot.js";
import {CapabilityRefreshCoordinator} from "../src/capabilities/refresh-coordinator.js";
import {CapabilitySnapshotStore} from "../src/capabilities/snapshot-store.js";

test("refresh is single-flight and readers never observe a half-built snapshot", async () => {
  const root = await mkdtemp(join(tmpdir(), "capability-refresh-"));
  try {
    let builds = 0;
    let release!: () => void;
    const gate = new Promise<void>((resolve) => { release = resolve; });
    const coordinator = new CapabilityRefreshCoordinator(
      new CapabilitySnapshotStore(root),
      async () => {
        builds += 1;
        await gate;
        return createCapabilitySnapshot([], {version: `v${builds}`});
      }
    );
    assert.equal(coordinator.getSnapshot(), undefined);
    const first = coordinator.refresh("fingerprint-1");
    const second = coordinator.refresh("fingerprint-1");
    assert.equal(coordinator.getSnapshot(), undefined);
    release();
    assert.equal((await first).version, "v1");
    assert.equal((await second).version, "v1");
    assert.equal(builds, 1);
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("failed rebuild retains a complete last-known-good snapshot as stale", async () => {
  const root = await mkdtemp(join(tmpdir(), "capability-refresh-fail-"));
  try {
    let fail = false;
    const coordinator = new CapabilityRefreshCoordinator(
      new CapabilitySnapshotStore(root),
      async () => {
        if (fail) throw new Error("build failed");
        return createCapabilitySnapshot([], {version: "good"});
      }
    );
    await coordinator.refresh("one");
    fail = true;
    const stale = await coordinator.refresh("two");
    assert.equal(stale.version, "good");
    assert.deepEqual(stale.health, {status: "stale", reason: "refresh_failed"});
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});
