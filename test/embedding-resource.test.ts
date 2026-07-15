import assert from "node:assert/strict";
import {createHash} from "node:crypto";
import {mkdir, mkdtemp, rm, writeFile} from "node:fs/promises";
import {tmpdir} from "node:os";
import {join} from "node:path";
import test from "node:test";
import {EmbeddingResourceLocator} from "../src/capabilities/embedding/embedding-resource-locator.js";
import {GRANITE_EMBEDDING_MODEL_ID, GRANITE_RESOURCE_PACKAGE_ID} from "../src/capabilities/embedding/embedding-resource-manifest.js";

test("only an explicit hash-valid Granite qint8 AVX2 resource is accepted", async () => {
  const root = await mkdtemp(join(tmpdir(), "embedding-resource-"));
  try {
    const content = "tiny fake vector resource";
    await writeFile(join(root, "model.fake"), content);
    const manifest = {
      schemaVersion: 1,
      packageId: GRANITE_RESOURCE_PACKAGE_ID,
      modelId: GRANITE_EMBEDDING_MODEL_ID,
      modelRevision: "explicit-test-revision",
      runtimeAbi: "fake-v1",
      architecture: "x64-avx2",
      quantization: "qint8",
      license: "Apache-2.0",
      tokenizerVersion: "test-v1",
      files: {"model.fake": createHash("sha256").update(content).digest("hex")}
    };
    await writeFile(join(root, "manifest.json"), JSON.stringify(manifest));
    const ready = await new EmbeddingResourceLocator(root, () => true).locate();
    assert.equal(ready.status, "ready");
    await writeFile(join(root, "model.fake"), "tampered");
    assert.deepEqual(await new EmbeddingResourceLocator(root, () => true).locate(), {status: "unavailable", reason: "hash_mismatch"});
    assert.deepEqual(await new EmbeddingResourceLocator(root, () => false).locate(), {status: "unavailable", reason: "incompatible_cpu"});
  } finally { await rm(root, {recursive: true, force: true}); }
});

test("missing resources degrade without creating a directory", async () => {
  const root = join(tmpdir(), `embedding-missing-${Date.now()}`);
  assert.deepEqual(await new EmbeddingResourceLocator(root, () => true).locate(), {status: "unavailable", reason: "missing"});
});
