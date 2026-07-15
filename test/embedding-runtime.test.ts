import assert from "node:assert/strict";
import test from "node:test";
import {DeadlineEmbeddingProvider} from "../src/capabilities/embedding/embedding-worker.js";

test("embedding deadline aborts a stuck local provider", async () => {
  let aborted = false;
  const provider = new DeadlineEmbeddingProvider({
    dimensions: 2,
    async embed(_texts, signal) {
      await new Promise<void>((resolve, reject) => {
        signal?.addEventListener("abort", () => { aborted = true; reject(new Error("aborted")); }, {once: true});
        void resolve;
      });
      return [];
    },
    async dispose() {}
  }, 5);
  await assert.rejects(provider.embed(["query"]), /aborted/);
  assert.equal(aborted, true);
});
