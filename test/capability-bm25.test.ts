import assert from "node:assert/strict";
import test from "node:test";
import {Bm25CapabilityIndex} from "../src/capabilities/search/bm25-index.js";
import {capabilityFixtures} from "./support/capability-fixtures.js";

test("BM25 recalls Chinese and English intent without inventing no-match results", () => {
  const index = new Bm25CapabilityIndex(capabilityFixtures());
  assert.equal(index.search("我想检查项目测试")[0]?.descriptor.id, "capability:minimax/npm-test");
  assert.equal(index.search("find symbol in source")[0]?.descriptor.id, "capability:minimax/search-code");
  assert.deepEqual(index.search("quantum banana telescope"), []);
});
