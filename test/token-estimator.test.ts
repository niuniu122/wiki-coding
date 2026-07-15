import assert from "node:assert/strict";
import test from "node:test";
import {ConservativeTokenEstimator} from "../src/runtime/token-estimator.js";

test("Chinese and emoji are not divided by four", () => {
  const estimator = new ConservativeTokenEstimator();

  assert.equal(estimator.estimateText("你好世界"), 5);
  assert.equal(estimator.estimateText("😄😄"), 3);
});

test("code receives a tighter estimate than plain Latin prose", () => {
  const estimator = new ConservativeTokenEstimator();

  assert.ok(
    estimator.estimateText("const value = foo.bar(baz);") >
      estimator.estimateText("a calm ordinary sentence here")
  );
});

test("message estimates include per-message overhead before the safety margin", () => {
  const estimator = new ConservativeTokenEstimator();

  assert.equal(
    estimator.estimateMessages([
      {role: "system", content: "你好世界"},
      {role: "user", content: ""}
    ]),
    14
  );
});

test("compound emoji components are counted conservatively with one safety margin", () => {
  const estimator = new ConservativeTokenEstimator();
  const flag = "\u{1F1E8}\u{1F1F3}";
  const keycap = "1\uFE0F\u20E3";
  const skinTone = "\u{1F44D}\u{1F3FD}";
  const joinedEmoji = "\u{1F469}\u200D\u{1F4BB}";

  assert.equal(estimator.estimateText(flag), 3);
  assert.equal(estimator.estimateText(keycap), 4);
  assert.equal(estimator.estimateText(skinTone), 3);
  assert.equal(estimator.estimateText(joinedEmoji), 4);
  assert.equal(
    estimator.estimateMessages([{role: "user", content: joinedEmoji}]),
    9
  );
});
