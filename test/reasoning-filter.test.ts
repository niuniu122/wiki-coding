import assert from "node:assert/strict";
import test from "node:test";
import {ReasoningFilter} from "../src/providers/reasoning-filter.js";

test("reasoning filter removes split think blocks without exposing their content", () => {
  const filter = new ReasoningFilter();
  const visible = [
    ...filter.process("before <thi"),
    ...filter.process("nk>PRIVATE_THOUGHT</think> after"),
    ...filter.flush()
  ].join("");

  assert.equal(visible, "before after");
  assert.equal(filter.hiddenCharacters, "PRIVATE_THOUGHT".length);
  assert.equal(visible.includes("PRIVATE_THOUGHT"), false);
});

test("explicit reasoning fields increase only the hidden character count", () => {
  const filter = new ReasoningFilter();
  filter.hide("RAW_REASONING_FIELD");

  assert.equal(filter.hiddenCharacters, "RAW_REASONING_FIELD".length);
  assert.equal(JSON.stringify(filter), "{\"hiddenCharacters\":19}");
});
