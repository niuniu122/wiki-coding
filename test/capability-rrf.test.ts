import assert from "node:assert/strict";
import test from "node:test";
import {reciprocalRankFusion} from "../src/capabilities/search/rrf.js";

test("RRF is stable and deterministic across lexical and vector ranks", () => {
  assert.deepEqual(reciprocalRankFusion([["b", "a"], ["a", "b"]]).map((item) => item.id), ["a", "b"]);
});
