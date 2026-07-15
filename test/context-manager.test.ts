import assert from "node:assert/strict";
import test from "node:test";
import {DEFAULT_CONFIG} from "../src/config/config-manager.js";
import {ContextManager} from "../src/runtime/context-manager.js";
import {ContextEngine} from "../src/runtime/context-engine.js";
import type {TokenEstimator} from "../src/runtime/token-estimator.js";
import type {ModelContextMessage} from "../src/types.js";
import type {AppConfig, ContextSummary, ThreadItem} from "../src/types.js";

const NOW = "2026-07-10T00:00:00.000Z";
const manager = new ContextManager();
const config: AppConfig = {
  ...DEFAULT_CONFIG,
  context: {...DEFAULT_CONFIG.context}
};

function message(
  id: string,
  turnId: string,
  role: "user" | "assistant",
  content: string
): ThreadItem {
  return {
    id,
    threadId: "thread_1",
    turnId,
    type: role === "user" ? "user_message" : "assistant_message",
    role,
    content,
    createdAt: NOW
  };
}

const items: ThreadItem[] = [
  message("old_user", "turn_old", "user", "old user"),
  message("old_assistant", "turn_old", "assistant", "old assistant"),
  message("new_user", "turn_new", "user", "new user"),
  message("new_assistant", "turn_new", "assistant", "new assistant")
];

const validSummary: ContextSummary = {
  id: "summary_1",
  threadId: "thread_1",
  createdAt: NOW,
  content: "old history summary",
  tokenEstimate: 5,
  coveredThroughItemId: "old_assistant"
};

test("a valid summary replaces covered history but keeps later messages", () => {
  const built = manager.buildContext({config, items, summaries: [validSummary]});
  const text = built.messages.map((entry) => entry.content).join("\n");

  assert.equal(text.includes("old user"), false);
  assert.equal(text.includes("old assistant"), false);
  assert.equal(text.includes("old history summary"), true);
  assert.equal(text.includes("new user"), true);
  assert.equal(text.includes("new assistant"), true);
});

test("an invalid summary boundary never drops history", () => {
  const invalidSummary: ContextSummary = {
    ...validSummary,
    id: "summary_invalid",
    coveredThroughItemId: "missing_item"
  };
  const built = manager.buildContext({config, items, summaries: [invalidSummary]});
  const text = built.messages.map((entry) => entry.content).join("\n");

  assert.equal(text.includes("old user"), true);
  assert.equal(text.includes("old assistant"), true);
});

test("trace events never enter model context", () => {
  const traceItem: ThreadItem = {
    id: "trace_1",
    threadId: "thread_1",
    turnId: "turn_new",
    type: "trace_event",
    content: "private trace",
    createdAt: NOW
  };
  const built = manager.buildContext({
    config,
    items: [message("current_user", "turn_new", "user", "current exact question"), traceItem],
    summaries: [],
    userInput: "current exact question"
  });

  assert.equal(
    built.messages.some((entry) => entry.content.includes("private trace")),
    false
  );
});

test("an interrupted partial assistant reply never enters model context or becomes a boundary", () => {
  const completed = message("completed_assistant", "turn_old", "assistant", "completed answer");
  const partial: ThreadItem = {
    ...message("partial_assistant", "turn_partial", "assistant", "unfinished answer"),
    metadata: {partial: true, interrupted: true}
  };
  const current = message("current_user", "turn_current", "user", "next question");
  const built = manager.buildContext({config, items: [completed, partial, current], summaries: []});

  assert.equal(built.messages.some((entry) => entry.content === "unfinished answer"), false);
  assert.equal(manager.findCompactionBoundary([completed, partial, current], "turn_current"), 0);
});

test("building stored context without a new input never appends undefined content", () => {
  const built = manager.buildContext({config, items, summaries: []});

  assert.equal(built.messages.every((entry) => typeof entry.content === "string"), true);
});

test("effective input budget reserves completion tokens", () => {
  const budgetConfig: AppConfig = {
    ...config,
    context: {
      ...config.context,
      workingContextLimit: 128_000,
      maxCompletionTokens: 8_000,
      autoCompactRatio: 0.9
    }
  };
  const built = manager.buildContext({config: budgetConfig, items: [], summaries: []});

  assert.equal(built.inputLimit, 120_000);
  assert.equal(built.autoCompactAt, 108_000);
});

test("the explicit current user input is not duplicated", () => {
  const built = manager.buildContext({
    config,
    items: [message("current_user", "turn_current", "user", "current exact question")],
    summaries: [],
    userInput: "current exact question"
  });

  assert.equal(
    built.messages.filter((entry) => entry.role === "user" && entry.content === "current exact question").length,
    1
  );
});

test("auto compaction stops at the last assistant message before the preserved turn", () => {
  const itemsWithCurrentTurn = [
    message("old_user", "turn_old", "user", "old user"),
    message("old_assistant", "turn_old", "assistant", "old assistant"),
    message("current_user", "turn_current", "user", "current exact question")
  ];

  assert.equal(manager.findCompactionBoundary(itemsWithCurrentTurn, "turn_current"), 1);
});

test("manual compaction uses the last completed assistant message", () => {
  assert.equal(manager.findCompactionBoundary(items), 3);
  assert.equal(
    manager.findCompactionBoundary([message("only_user", "turn_only", "user", "unfinished")]),
    -1
  );
});

test("summary records persist their coverage boundary", () => {
  const summary = manager.createSummaryRecord(
    "thread_1",
    "bounded summary",
    "manual",
    "old_assistant"
  );

  assert.equal(summary.coveredThroughItemId, "old_assistant");
  assert.equal(summary.content, "bounded summary");
  assert.equal(summary.tokenEstimate > 0, true);
});

test("ContextEngine exposes the new API while ContextManager keeps legacy callers connected", () => {
  const engine = new ContextEngine();
  const built = engine.build({config, items, summaries: [validSummary]});

  assert.equal(built.messages.some((entry) => entry.content.includes("old history summary")), true);
  assert.equal(engine.compactionBoundary(items), 3);
  assert.equal(
    engine.createSummary("thread_1", "new API summary", "old_assistant").coveredThroughItemId,
    "old_assistant"
  );
  assert.equal(manager.buildContext({config, items, summaries: []}).messages.length > 0, true);
});

test("ContextEngine delegates message and summary estimates to an injected estimator", () => {
  class RecordingEstimator implements TokenEstimator {
    messages: ModelContextMessage[] = [];

    estimateText(_text: string): number {
      return 17;
    }

    estimateMessages(messages: ModelContextMessage[]): number {
      this.messages = messages;
      return 321;
    }
  }

  const estimator = new RecordingEstimator();
  const engine = new ContextEngine(estimator);
  const built = engine.build({config, items, summaries: []});
  const summary = engine.createSummary("thread_1", "summary", "old_assistant");

  assert.equal(built.tokenEstimate, 321);
  assert.deepEqual(estimator.messages, built.messages);
  assert.equal(summary.tokenEstimate, 17);
});
