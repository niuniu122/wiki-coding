import assert from "node:assert/strict";
import test from "node:test";
import {ExactCapabilityIndex} from "../src/capabilities/search/exact-index.js";
import {capabilityFixtures} from "./support/capability-fixtures.js";

test("fully-qualified IDs, slash commands, and aliases resolve exactly", () => {
  const index = new ExactCapabilityIndex(capabilityFixtures());
  assert.equal(index.resolve("/READ")?.id, "capability:minimax/read-file");
  assert.equal(index.resolve("查看文件")?.id, "capability:minimax/read-file");
  assert.equal(index.resolve("CAPABILITY:MINIMAX/SEARCH-CODE")?.id, "capability:minimax/search-code");
  assert.equal(index.resolve("unknown"), undefined);
});
