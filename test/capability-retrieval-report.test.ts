import assert from "node:assert/strict";
import test from "node:test";
import {runCapabilityRetrievalReport} from "../src/eval/capability-retrieval-report.js";

test("expanded offline retrieval report gates at least 150 curated cases in every mode", async () => {
  const report = await runCapabilityRetrievalReport();
  assert.ok(report.cases >= 150);
  assert.equal(report.passed, true);
  for (const metrics of [report.lexical, report.embedding, report.fused, report.noResourceFallback]) {
    assert.equal(metrics.idValidity, 1);
    assert.ok(metrics.recallAt5 >= 0.95);
    assert.ok(metrics.top1 >= 0.85);
    assert.ok(metrics.mrr >= 0.9);
    assert.ok(metrics.noMatchPrecision >= 0.95);
  }
  assert.deepEqual(report.disabledPath, {remoteRequests: 0, catalogInitializations: 0});
});
