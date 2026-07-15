import assert from "node:assert/strict";
import {mkdir, mkdtemp, rm, symlink, writeFile} from "node:fs/promises";
import {tmpdir} from "node:os";
import {join} from "node:path";
import test from "node:test";
import {CapabilityCatalog} from "../src/capabilities/capability-catalog.js";
import {parseCapabilityManifest} from "../src/capabilities/capability-manifest.js";
import {resolveManagedPath} from "../src/capabilities/path-policy.js";

function descriptor(scope: "builtin" | "project_native" | "user_native" | "project_compat" | "user_compat", file: string) {
  return parseCapabilityManifest({
    schemaVersion: 1,
    id: "capability:minimax/read-file",
    name: scope,
    description: scope,
    safetyClass: "catalog_read",
    execution: {kind: "metadata_only"}
  }, {kind: "minimax", scope, root: "root", file});
}

test("catalog precedence is deterministic and losers remain diagnostic-only", () => {
  const catalog = CapabilityCatalog.build([
    descriptor("user_compat", "e"),
    descriptor("user_native", "c"),
    descriptor("project_compat", "d"),
    descriptor("project_native", "b")
  ]);
  assert.equal(catalog.get("CAPABILITY:MINIMAX/READ-FILE")?.name, "project_native");
  assert.equal(catalog.candidates().length, 1);
  assert.equal(catalog.entries().filter((entry) => entry.descriptor.availability === "shadowed").length, 3);

  const protectedCatalog = CapabilityCatalog.build([
    descriptor("project_native", "override"),
    descriptor("builtin", "builtin")
  ]);
  assert.equal(protectedCatalog.candidates()[0]?.source.scope, "builtin");
});

test("managed paths reject symlink or junction escape", async (t) => {
  const root = await mkdtemp(join(tmpdir(), "capability-path-root-"));
  const outside = await mkdtemp(join(tmpdir(), "capability-path-outside-"));
  try {
    await mkdir(join(root, "inside"));
    await writeFile(join(outside, "secret.txt"), "secret");
    try {
      await symlink(outside, join(root, "escape"), process.platform === "win32" ? "junction" : "dir");
    } catch {
      t.skip("symlink creation is unavailable on this host");
      return;
    }
    await assert.rejects(resolveManagedPath(root, join(root, "escape", "secret.txt")), /outside_managed_root/);
  } finally {
    await rm(root, {recursive: true, force: true});
    await rm(outside, {recursive: true, force: true});
  }
});
