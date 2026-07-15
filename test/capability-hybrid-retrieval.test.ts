import assert from "node:assert/strict";
import test from "node:test";
import {HybridCapabilityRetriever} from "../src/capabilities/search/hybrid-retriever.js";
import {CapabilityVectorIndex} from "../src/capabilities/search/vector-index.js";
import {capabilityFixtures} from "./support/capability-fixtures.js";

test("exact wins, fusion is stable, and embedding failures fall back to BM25", async () => {
  const descriptors = capabilityFixtures();
  const vectors = new CapabilityVectorIndex(descriptors.map((descriptor, index) => ({descriptor, vector: index === 1 ? [1, 0] : [0, 1]})));
  const provider = {dimensions: 2, async embed() { return [[1, 0]]; }, async dispose() {}};
  const retriever = new HybridCapabilityRetriever(descriptors, {provider, vectorIndex: vectors});
  assert.equal((await retriever.retrieve("/read")).path, "exact");
  assert.equal((await retriever.retrieve("find source symbol")).descriptors[0]?.id, "capability:minimax/search-code");

  const failing = new HybridCapabilityRetriever(descriptors, {provider: {...provider, async embed() { throw new Error("offline"); }}, vectorIndex: vectors});
  const fallback = await failing.retrieve("检查项目测试");
  assert.equal(fallback.path, "lexical");
  assert.equal(fallback.fallbackReason, "embedding_timeout");
  assert.equal(fallback.descriptors[0]?.id, "capability:minimax/npm-test");
  assert.ok(fallback.cards.length <= 5);
});
