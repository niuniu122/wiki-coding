import assert from "node:assert/strict";
import test from "node:test";
import {normalizeQuery, tokenizeQuery} from "../src/capabilities/search/query-normalizer.js";

test("mixed Chinese, English, paths, and punctuation normalize deterministically", () => {
  assert.equal(normalizeQuery("  /READ   File  "), "/read file");
  const tokens = tokenizeQuery("帮我查看 project/src 文件");
  assert.equal(tokens.includes("查看"), true);
  assert.equal(tokens.includes("read"), true);
  assert.equal(tokens.includes("project/src"), true);
  assert.equal(tokens.includes("project"), true);
});
