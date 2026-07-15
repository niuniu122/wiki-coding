import assert from "node:assert/strict";
import test from "node:test";
import {CapabilityFacetIndex} from "../src/capabilities/search/facet-index.js";
import {capabilityFixtures} from "./support/capability-fixtures.js";

test("domain action object facets reference the same valid catalog IDs", () => {
  const index = new CapabilityFacetIndex(capabilityFixtures());
  assert.deepEqual(index.filter("action", "search"), ["capability:minimax/search-code"]);
  assert.deepEqual(index.filter("object", "file"), ["capability:minimax/read-file"]);
});
