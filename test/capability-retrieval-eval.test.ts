import assert from "node:assert/strict";
import test from "node:test";
import {evaluateRetrieval} from "../src/capabilities/eval/retrieval-evaluator.js";
import {HybridCapabilityRetriever} from "../src/capabilities/search/hybrid-retriever.js";
import {capabilityFixtures} from "./support/capability-fixtures.js";

test("the 60-case mixed-language lexical baseline meets initial report gates", async () => {
  const descriptors = capabilityFixtures();
  const cases = Array.from({length: 60}, (_, index) => {
    const kind = index % 4;
    if (kind === 0) return {query: index % 8 ? "查看文件" : "/read", expectedIds: [descriptors[0]!.id]};
    if (kind === 1) return {query: index % 8 ? "find source symbol" : "/search", expectedIds: [descriptors[1]!.id]};
    if (kind === 2) return {query: index % 8 ? "检查项目测试" : "/test", expectedIds: [descriptors[2]!.id]};
    return {query: `unrelated quantum banana ${index}`, expectedIds: [], noMatch: true};
  });
  const metrics = await evaluateRetrieval(new HybridCapabilityRetriever(descriptors), cases, new Set(descriptors.map((item) => item.id)));
  assert.equal(metrics.cases, 60);
  assert.equal(metrics.idValidity, 1);
  assert.ok(metrics.recallAt5 >= 0.95);
  assert.ok(metrics.top1 >= 0.85);
  assert.ok(metrics.mrr >= 0.9);
  assert.ok(metrics.noMatchPrecision >= 0.95);
});
