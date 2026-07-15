import assert from "node:assert/strict";
import {createHash} from "node:crypto";
import {mkdtemp, mkdir, readFile, symlink, writeFile} from "node:fs/promises";
import {tmpdir} from "node:os";
import {join} from "node:path";
import test from "node:test";
import {createCapabilityInvocation} from "../src/capabilities/capability-invocation.js";
import {WorkspaceReadExecutor} from "../src/capabilities/executors/workspace-read-executor.js";
import {parseCapabilityManifest} from "../src/capabilities/capability-manifest.js";

const limits = {timeoutMs: 1_000, maxOutputBytes: 1_024, maxDirectoryEntries: 10};

function descriptor(operation: "read_file" | "list_files") {
  return parseCapabilityManifest({
    schemaVersion: 1,
    id: `capability:test/${operation}`,
    name: operation,
    description: "read fixture",
    safetyClass: "workspace_read",
    execution: {kind: "workspace_read", operation}
  }, {kind: "minimax", scope: "builtin", root: "builtin", file: `${operation}.json`});
}

function invocation(capabilityId: string, path: string) {
  return createCapabilityInvocation({capabilityId, snapshotVersion: "v1", arguments: {path}, approved: false});
}

test("workspace reader reads bounded files and lists only local metadata", async () => {
  const root = await mkdtemp(join(tmpdir(), "capability-read-"));
  await writeFile(join(root, "hello.txt"), "hello");
  await mkdir(join(root, "folder"));
  const executor = new WorkspaceReadExecutor(root, limits);
  assert.deepEqual(await executor.execute(descriptor("read_file"), invocation("capability:test/read_file", "hello.txt")), {status: "succeeded", output: "hello"});
  const listed = await executor.execute(descriptor("list_files"), invocation("capability:test/list_files", "."));
  assert.equal(listed.status, "succeeded");
  if (listed.status === "succeeded") {
    assert.match(listed.output, /directory\tfolder/);
    assert.match(listed.output, /file\thello\.txt/);
  }
});

test("workspace reader rejects absolute paths, traversal, escaped links, binary and oversized files", async () => {
  const parent = await mkdtemp(join(tmpdir(), "capability-boundary-"));
  const root = join(parent, "root");
  const outside = join(parent, "outside");
  await mkdir(root);
  await mkdir(outside);
  await writeFile(join(root, "binary.bin"), Buffer.from([1, 0, 2]));
  await writeFile(join(root, "large.txt"), "x".repeat(2_000));
  await writeFile(join(outside, "secret.txt"), "secret");
  await symlink(outside, join(root, "escape"), process.platform === "win32" ? "junction" : "dir");
  const executor = new WorkspaceReadExecutor(root, limits);
  const read = descriptor("read_file");
  for (const path of [join(root, "binary.bin"), "../outside/secret.txt", "escape/secret.txt"]) {
    const result = await executor.execute(read, invocation(read.id, path));
    assert.equal(result.status, "failed", path);
  }
  assert.deepEqual(await executor.execute(read, invocation(read.id, "binary.bin")), {status: "failed", code: "binary_file"});
  assert.deepEqual(await executor.execute(read, invocation(read.id, "large.txt")), {status: "failed", code: "output_limit"});
});

test("workspace reader has no write side effect", async () => {
  const root = await mkdtemp(join(tmpdir(), "capability-hash-"));
  const file = join(root, "stable.txt");
  await writeFile(file, "unchanged");
  const before = createHash("sha256").update(await readFile(file)).digest("hex");
  const read = descriptor("read_file");
  await new WorkspaceReadExecutor(root, limits).execute(read, invocation(read.id, "stable.txt"));
  const after = createHash("sha256").update(await readFile(file)).digest("hex");
  assert.equal(after, before);
});
