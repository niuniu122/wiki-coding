import assert from "node:assert/strict";
import test from "node:test";
import {LocalSummaryGenerator} from "../src/runtime/summary-generator.js";
import type {ThreadItem} from "../src/types.js";

const NOW = "2026-07-10T00:00:00.000Z";

function item(index: number, role: "user" | "assistant", content: string): ThreadItem {
  return {
    id: `item_${index}`,
    threadId: "thread_1",
    turnId: `turn_${Math.floor(index / 2)}`,
    type: role === "user" ? "user_message" : "assistant_message",
    role,
    content,
    createdAt: NOW
  };
}

test("local summaries are bounded and contain the latest covered goal", async () => {
  const oldItems = Array.from({length: 8}, (_, index) =>
    item(index, index % 2 === 0 ? "user" : "assistant", `old-${index}-${"x".repeat(800)}`)
  );
  const items = [
    ...oldItems,
    item(8, "user", "latest covered goal"),
    item(9, "assistant", "latest covered answer")
  ];
  const generator = new LocalSummaryGenerator();

  const content = await generator.generate(items, "manual");

  assert.equal(content.includes("压缩原因：manual"), true);
  assert.equal(content.includes("latest covered goal"), true);
  assert.equal(content.includes("latest covered answer"), true);
  assert.equal(content.length <= 2400, true);
});

test("trace and error items are excluded from local summaries", async () => {
  const generator = new LocalSummaryGenerator();
  const items: ThreadItem[] = [
    item(1, "user", "visible request"),
    {
      id: "trace_1",
      threadId: "thread_1",
      turnId: "turn_1",
      type: "trace_event",
      content: "private trace",
      createdAt: NOW
    },
    {
      id: "error_1",
      threadId: "thread_1",
      turnId: "turn_1",
      type: "error",
      content: "secret error payload",
      createdAt: NOW
    }
  ];

  const content = await generator.generate(items, "auto");

  assert.equal(content.includes("visible request"), true);
  assert.equal(content.includes("private trace"), false);
  assert.equal(content.includes("secret error payload"), false);
});

test("interrupted partial assistant replies are excluded from local summaries", async () => {
  const generator = new LocalSummaryGenerator();
  const partial: ThreadItem = {
    ...item(2, "assistant", "unfinished answer"),
    metadata: {partial: true, interrupted: true}
  };

  const content = await generator.generate(
    [item(1, "user", "visible request"), partial],
    "resume"
  );

  assert.equal(content.includes("visible request"), true);
  assert.equal(content.includes("unfinished answer"), false);
});
