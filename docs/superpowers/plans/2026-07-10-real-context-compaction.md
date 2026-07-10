# Real Context Compaction Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make manual and automatic compaction actually replace covered model-visible history while preserving the original JSONL transcript and the current user message.

**Architecture:** `ContextManager` owns model-visible history selection and coverage-boundary validation. A focused `SummaryGenerator` owns summary text generation. `AgentRuntime` coordinates persistence and must rebuild context after auto-compaction before calling the model. Tests use in-memory storage and a fake model adapter, so no real API key or network is required.

**Tech Stack:** Node.js 20+, TypeScript 5.8, `node:test`, `node:assert/strict`, existing `tsx`, Ink 5.

## Global Constraints

- Do not add `read_file`, shell, MCP, multi-agent, plugin, or SQLite functionality.
- Do not delete or rewrite existing `.mini-codex` JSONL history.
- Keep new JSONL fields optional for backward compatibility.
- Do not read, print, or persist real API keys in tests.
- Full trace remains local and never enters model context.
- The subproject has no independent Git repository; do not create commits in the unrelated `E:/Agenc` root repository.

---

## File Map

- Create `test/run-tests.ts`: stable entrypoint for the built-in Node test runner.
- Create `test/context-manager.test.ts`: coverage-boundary, token-budget, and trace-exclusion tests.
- Create `test/agent-runtime-compaction.test.ts`: automatic compaction context-rebuild test with fakes.
- Create `src/runtime/summary-generator.ts`: `SummaryGenerator` interface and bounded local implementation.
- Modify `src/types.ts`: optional `coveredThroughItemId` on `ContextSummary`.
- Modify `src/protocol.ts`: richer `compact.completed` event payload.
- Modify `src/runtime/context-manager.ts`: boundary selection, summary record creation, and coverage-aware context building.
- Modify `src/runtime/agent-runtime.ts`: injected summary generator/storage, real compaction, and post-compaction rebuild.
- Modify `src/ui/App.tsx`: display compact/no-op token change without owning compaction logic.
- Modify `package.json`: add the test command.

### Task 1: Establish the test harness and coverage-aware context selection

**Files:**
- Create: `test/run-tests.ts`
- Create: `test/context-manager.test.ts`
- Modify: `package.json`
- Modify: `src/types.ts`
- Modify: `src/runtime/context-manager.ts`

**Interfaces:**
- Produces: `ContextSummary.coveredThroughItemId?: string`
- Produces: `ContextManager.buildContext({ config, items, summaries, userInput? }): BuiltContext`
- Produces: `ContextManager.findCompactionBoundary(items, preserveTurnId?): number`

- [ ] **Step 1: Add a test entrypoint and failing behavior tests**

`test/run-tests.ts`:

```ts
import "./context-manager.test.js";
```

Add to `package.json` scripts:

```json
"test": "tsx test/run-tests.ts"
```

Create `test/context-manager.test.ts` with tests that construct deterministic ThreadItems and assert:

```ts
test("a valid summary replaces covered history but keeps later messages", () => {
  const summary: ContextSummary = {
    id: "summary_1",
    threadId: "thread_1",
    createdAt: NOW,
    content: "old history summary",
    tokenEstimate: 5,
    coveredThroughItemId: "old_assistant"
  };
  const built = manager.buildContext({config, items, summaries: [summary]});
  const text = built.messages.map((message) => message.content).join("\n");
  assert.equal(text.includes("old user"), false);
  assert.equal(text.includes("old assistant"), false);
  assert.equal(text.includes("new user"), true);
  assert.equal(text.includes("new assistant"), true);
});

test("an invalid summary boundary never drops history", () => {
  const summary = {...validSummary, coveredThroughItemId: "missing"};
  const built = manager.buildContext({config, items, summaries: [summary]});
  const text = built.messages.map((message) => message.content).join("\n");
  assert.equal(text.includes("old user"), true);
  assert.equal(text.includes("old assistant"), true);
});

test("trace events never enter model context", () => {
  const built = manager.buildContext({config, items: [...items, traceItem], summaries: []});
  assert.equal(built.messages.some((message) => message.content.includes("private trace")), false);
});

test("effective input budget reserves completion tokens", () => {
  const budgetConfig = {
    ...config,
    context: {...config.context, workingContextLimit: 128_000, maxCompletionTokens: 8_000}
  };
  const built = manager.buildContext({config: budgetConfig, items: [], summaries: []});
  assert.equal(built.inputLimit, 120_000);
  assert.equal(built.autoCompactAt, 108_000);
});
```

- [ ] **Step 2: Run the tests and verify the new contract fails**

Run: `npm test`

Expected: FAIL because `coveredThroughItemId`, `inputLimit`, optional `userInput`, and boundary filtering do not exist.

- [ ] **Step 3: Add the optional summary boundary and implement history selection**

Update `ContextSummary`:

```ts
export interface ContextSummary {
  id: string;
  threadId: string;
  createdAt: string;
  content: string;
  tokenEstimate: number;
  coveredThroughItemId?: string;
}
```

Update `BuiltContext`:

```ts
export interface BuiltContext {
  messages: ModelContextMessage[];
  tokenEstimate: number;
  inputLimit: number;
  autoCompactAt: number;
  shouldCompact: boolean;
  summaryBoundaryValid: boolean;
}
```

Implement coverage-aware selection:

```ts
const latestSummary = params.summaries.at(-1);
const boundaryIndex = latestSummary?.coveredThroughItemId
  ? params.items.findIndex((item) => item.id === latestSummary.coveredThroughItemId)
  : -1;
const summaryBoundaryValid = Boolean(latestSummary?.coveredThroughItemId) && boundaryIndex >= 0;
const visibleItems = summaryBoundaryValid ? params.items.slice(boundaryIndex + 1) : params.items;
const recentMessages = visibleItems
  .filter((item) => item.type === "user_message" || item.type === "assistant_message")
  .map((item) => ({role: item.role ?? "user", content: item.content}));
```

Use the effective input budget:

```ts
const inputLimit = Math.max(
  1,
  params.config.context.workingContextLimit - params.config.context.maxCompletionTokens
);
const autoCompactAt = Math.max(
  1,
  Math.floor(inputLimit * params.config.context.autoCompactRatio)
);
```

Only append `userInput` when provided and not already the latest user message.

- [ ] **Step 4: Run focused tests**

Run: `npm test`

Expected: context-manager tests PASS; runtime test may still fail because Task 3 is not implemented.

- [ ] **Step 5: Run the type checker**

Run: `npm run check`

Expected: PASS after all call sites accept the optional `userInput` and `BuiltContext.inputLimit`.

### Task 2: Separate bounded summary generation from context selection

**Files:**
- Create: `src/runtime/summary-generator.ts`
- Modify: `src/runtime/context-manager.ts`
- Modify: `test/context-manager.test.ts`

**Interfaces:**
- Produces: `CompactReason = "manual" | "auto" | "resume"`
- Produces: `SummaryGenerator.generate(items, reason): Promise<string>`
- Produces: `LocalSummaryGenerator`
- Produces: `ContextManager.createSummaryRecord(threadId, content, reason, coveredThroughItemId)`

- [ ] **Step 1: Write failing tests for boundary selection and bounded summary content**

```ts
test("auto compaction stops before the preserved current turn", () => {
  assert.equal(manager.findCompactionBoundary(itemsWithCurrentTurn, "turn_current"), 1);
});

test("local summaries are bounded and contain the latest covered goal", async () => {
  const generator = new LocalSummaryGenerator();
  const content = await generator.generate(largeItems, "manual");
  assert.equal(content.includes("latest covered goal"), true);
  assert.equal(content.length <= 2400, true);
});
```

- [ ] **Step 2: Run the focused test and verify failure**

Run: `npm test`

Expected: FAIL because `SummaryGenerator`, `LocalSummaryGenerator`, and `findCompactionBoundary` are missing.

- [ ] **Step 3: Implement the summary generator**

```ts
export type CompactReason = "manual" | "auto" | "resume";

export interface SummaryGenerator {
  generate(items: ThreadItem[], reason: CompactReason): Promise<string>;
}

export class LocalSummaryGenerator implements SummaryGenerator {
  async generate(items: ThreadItem[], reason: CompactReason): Promise<string> {
    const exchanges = items
      .filter((item) => item.type === "user_message" || item.type === "assistant_message")
      .slice(-6)
      .map((item) => `${item.role === "assistant" ? "Agent" : "用户"}：${item.content.slice(0, 320)}`);
    return [
      `压缩原因：${reason}`,
      "以下内容代表已覆盖的旧会话；原始记录仍保存在本地。",
      ...exchanges
    ].join("\n").slice(0, 2400);
  }
}
```

Implement boundary selection so it returns the last assistant message before `preserveTurnId`, or the last assistant message overall when no turn is preserved.

Create the summary record with `coveredThroughItemId` and a token estimate.

- [ ] **Step 4: Run tests and type checking**

Run: `npm test && npm run check`

Expected: PASS for context-manager and summary-generator behavior.

### Task 3: Make Runtime compaction transactional and rebuild automatic context

**Files:**
- Create: `test/agent-runtime-compaction.test.ts`
- Modify: `src/runtime/agent-runtime.ts`
- Modify: `src/protocol.ts`

**Interfaces:**
- Consumes: `SummaryGenerator`, `ContextManager.findCompactionBoundary`, coverage-aware `buildContext`
- Produces: optional injected `StorageProvider` for deterministic tests
- Produces: rich `compact.completed` RuntimeEvent

- [ ] **Step 1: Write an integration-style failing test with in-memory storage**

Build a fake `StorageProvider` that stores arrays in memory and a fake `ModelAdapter` that records the messages passed to `streamResponse`.

Add the Runtime test to `test/run-tests.ts` only after its file exists:

```ts
import "./context-manager.test.js";
import "./agent-runtime-compaction.test.js";
```

The test must seed an old oversized user/assistant pair, submit a new current user message, and assert:

```ts
assert.equal(modelMessages.some((message) => message.content.includes("old oversized user")), false);
assert.equal(modelMessages.some((message) => message.content === "current exact question"), true);
assert.equal(storage.summaries.length, 1);
assert.equal(storage.summaries[0]?.coveredThroughItemId, "old_assistant");
```

- [ ] **Step 2: Run the test and verify the current bug**

Run: `npm test`

Expected: FAIL because Runtime reuses the pre-compaction `builtContext.messages` and cannot inject in-memory storage.

- [ ] **Step 3: Add test-only dependency injection without changing production defaults**

Extend the constructor tail:

```ts
private readonly summaryGenerator: SummaryGenerator = new LocalSummaryGenerator(),
private readonly storageOverride?: StorageProvider
```

During `init()`:

```ts
this.storage =
  this.storageOverride ??
  (this.config.storage.driver === "sqlite"
    ? new SqliteStorageProvider()
    : new JsonlStorageProvider(this.stateRoot));
```

- [ ] **Step 4: Implement transactional compact behavior**

`compact()` must:

1. Read items and summaries.
2. Build the before-context.
3. Find the eligible boundary.
4. Return a no-op completed event when no boundary exists.
5. Generate summary content from only covered items.
6. Append the summary before treating it as active.
7. Build the after-context with the newly persisted summary.
8. Emit `compact.completed` with `compacted`, boundary, before/after estimates, and summary.

Update the event contract:

```ts
| {
    type: "compact.completed";
    summary: string;
    compacted: boolean;
    coveredThroughItemId?: string;
    beforeTokens: number;
    afterTokens: number;
  }
```

- [ ] **Step 5: Rebuild context after automatic compaction**

Change `builtContext` from `const` to `let`. After successful auto-compaction, re-read summaries and call `buildContext` again using the same exact current `input` before invoking `modelAdapter.streamResponse`.

- [ ] **Step 6: Run all tests**

Run: `npm test`

Expected: PASS; fake model sees the summary plus current question, not the covered oversized history.

- [ ] **Step 7: Run static verification**

Run: `npm run check && npm run build`

Expected: both commands exit 0.

### Task 4: Present compaction results without moving policy into UI

**Files:**
- Modify: `src/ui/App.tsx`
- Modify: `README.md`

**Interfaces:**
- Consumes: rich `compact.completed` RuntimeEvent
- Produces: user-visible before/after token status

- [ ] **Step 1: Update UI event presentation**

Replace the generic completed status with:

```ts
if (event.type === "compact.completed") {
  setStatus(
    event.compacted
      ? `上下文已压缩：${event.beforeTokens} → ${event.afterTokens} token`
      : "当前没有可压缩的已完成历史"
  );
  return;
}
```

- [ ] **Step 2: Document real compact semantics**

Update README `/compact` text to state that original JSONL history is retained while covered messages are replaced in model-visible context by the latest summary.

- [ ] **Step 3: Run the full verification set**

Run: `npm test && npm run check && npm run build`

Expected: all commands exit 0.

- [ ] **Step 4: Run a non-secret local behavior probe**

Run a `tsx -e` probe that constructs a large old exchange, a valid covered summary, and a new question. Print only message lengths and token estimates.

Expected: covered old message lengths are absent; new question remains; after-token estimate is lower than before-token estimate.

- [ ] **Step 5: Record the teaching checkpoint**

Append the changed files, exact commands, results, risks, and the reusable lesson “durable history and model-visible context are different state projections” to `agent-logs/temporary/2026-07-10.md`.

## Plan Self-Review

- Spec coverage: coverage boundary, current-turn preservation, context rebuild, input budget, compatibility, event/UI reporting, error behavior, and offline tests are mapped to Tasks 1–4.
- Type consistency: `coveredThroughItemId`, `SummaryGenerator.generate`, `BuiltContext.inputLimit`, and rich `compact.completed` names are identical across tasks.
- Dependency scope: no new npm package is required.
- Git handling: commit steps are intentionally omitted because `minimax-codex` is not an independent repository and is untracked inside an unrelated root repository.
