import assert from "node:assert/strict";
import test from "node:test";
import {
  LocalSummaryGenerator,
  StructuredLocalSummaryGenerator
} from "../src/runtime/summary-generator.js";
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

test("summary preserves original goal, constraints, decisions, open items, and recent exchanges", async () => {
  const oldItems = Array.from({length: 8}, (_, index) =>
    item(index, index % 2 === 0 ? "user" : "assistant", `old-${index}-${"x".repeat(800)}`)
  );
  const items = [
    item(20, "user", "Original goal is to build a deterministic context engine."),
    item(21, "assistant", "I understand the original goal."),
    item(22, "user", "Constraint: it must stay offline and never expose secrets."),
    item(23, "assistant", "Decision: use a replaceable conservative estimator."),
    item(24, "user", "Open item: which code-density heuristic should we use?"),
    item(25, "assistant", "That heuristic remains unresolved."),
    ...oldItems,
    item(8, "user", "latest covered goal"),
    item(9, "assistant", "latest covered answer")
  ];
  const generator = new StructuredLocalSummaryGenerator();

  const content = await generator.generate(items, "manual");

  assert.match(content, /Original goal:/);
  assert.match(content, /Constraints:/);
  assert.match(content, /Decisions:/);
  assert.match(content, /Open items:/);
  assert.match(content, /Recent exchanges:/);
  assert.equal(content.includes("Original goal is to build a deterministic context engine."), true);
  assert.equal(content.includes("must stay offline and never expose secrets"), true);
  assert.equal(content.includes("use a replaceable conservative estimator"), true);
  assert.equal(content.includes("which code-density heuristic should we use"), true);
  assert.equal(content.includes("latest covered goal"), true);
  assert.equal(content.includes("latest covered answer"), true);
  assert.equal(content.length <= 4096, true);
  assert.equal(content.split("\n").every((line) => line.length <= 480), true);
});

test("trace, error payload bodies, secrets, and raw reasoning are excluded", async () => {
  const generator = new StructuredLocalSummaryGenerator();
  const items: ThreadItem[] = [
    item(1, "user", "Use API_KEY=sk-supersecret for the visible request"),
    item(2, "assistant", "<think>private chain of thought</think>Visible answer"),
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
  assert.equal(content.includes("Visible answer"), true);
  assert.equal(content.includes("private trace"), false);
  assert.equal(content.includes("secret error payload"), false);
  assert.equal(content.includes("sk-supersecret"), false);
  assert.equal(content.includes("private chain of thought"), false);
  assert.equal(content.includes("An error occurred during turn turn_1."), true);
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

test("LocalSummaryGenerator remains a structured compatibility export", async () => {
  const generator = new LocalSummaryGenerator();

  const content = await generator.generate(
    [item(1, "user", "compatibility goal"), item(2, "assistant", "compatibility answer")],
    "manual"
  );

  assert.match(content, /Original goal:/);
  assert.match(content, /Recent exchanges:/);
});

test("a saturated summary ends on an entry boundary instead of slicing an entry", async () => {
  const generator = new StructuredLocalSummaryGenerator();
  const items = Array.from({length: 8}, (_, index) => [
    item(
      index * 2,
      "user",
      index < 4
        ? `Constraint: must retain ${index} ${"c".repeat(800)}`
        : `Open item: which option ${index}? ${"o".repeat(800)}`
    ),
    item(
      index * 2 + 1,
      "assistant",
      index < 4
        ? `Decision: use option ${index} ${"d".repeat(800)}`
        : `Long completed answer ${index} ${"a".repeat(800)}`
    )
  ]).flat();

  const content = await generator.generate(items, "manual");

  assert.equal(content.length <= 4096, true);
  assert.equal(content.endsWith("…"), true);
});

test("structured categories recognize explicit requirement and choice statements", async () => {
  const generator = new StructuredLocalSummaryGenerator();
  const content = await generator.generate(
    [
      item(0, "user", "Requirements: keep callers connected. Prohibited: trace payloads."),
      item(1, "assistant", "We chose the local estimator.")
    ],
    "manual"
  );
  const constraints = content.match(/Constraints:\n([\s\S]*?)\n\nDecisions:/u)?.[1] ?? "";
  const decisions = content.match(/Decisions:\n([\s\S]*?)\n\nOpen items:/u)?.[1] ?? "";

  assert.equal(constraints.includes("keep callers connected"), true);
  assert.equal(decisions.includes("chose the local estimator"), true);
});

test("standalone high-risk credential families are redacted without exposing values", async () => {
  const generator = new StructuredLocalSummaryGenerator();
  const credentialValues = [
    "ghp_1234567890abcdefghijklmnopqrstuv",
    "github_pat_11AA22bb33CC44dd55EE66ff77GG88hh",
    "glpat-AbCdEfGhIjKlMnOpQrStUvWx",
    "AKIAIOSFODNN7EXAMPLE",
    "Qm9vdHN0cmFwLXNlY3JldC0xMjM0NTY3ODkwQUJDREVGRw"
  ];
  const items = credentialValues.flatMap((credential, index) => [
    item(index * 2, "user", `Credential ${credential} is configured.`),
    item(index * 2 + 1, "assistant", `Credential ${index} acknowledged.`)
  ]);

  const content = await generator.generate(items, "manual");

  assert.equal(credentialValues.some((credential) => content.includes(credential)), false);
  assert.equal(content.includes("[REDACTED]"), true);
});

test("open items prioritize explicit user questions and error signals over assistant text", async () => {
  const generator = new StructuredLocalSummaryGenerator();
  const items: ThreadItem[] = [
    item(0, "user", "Which deployment blocker remains unresolved?"),
    item(1, "assistant", "Acknowledged."),
    ...Array.from({length: 4}, (_, index) =>
      item(index * 2 + 3, "assistant", `Assistant unresolved note ${index}.`)
    ),
    {
      id: "error_priority",
      threadId: "thread_1",
      turnId: "turn_error",
      type: "error",
      content: "provider payload body",
      createdAt: NOW
    }
  ];

  const content = await generator.generate(items, "auto");
  const openItems = content.match(/Open items:\n([\s\S]*?)\n\nRecent exchanges:/u)?.[1] ?? "";

  assert.equal(openItems.includes("Which deployment blocker remains unresolved?"), true);
  assert.equal(openItems.includes("An error occurred during turn turn_error."), true);
  assert.equal((openItems.match(/Assistant unresolved note/gu) ?? []).length <= 2, true);
});
