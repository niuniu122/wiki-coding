import assert from "node:assert/strict";
import test from "node:test";
import {
  formatCompactionStatus,
  formatHistoryMessages,
  formatThreadList
} from "../src/ui/format-runtime-event.js";
import type {ThreadItem, ThreadRecord} from "../src/types.js";

test("compaction status shows the measured token reduction", () => {
  const status = formatCompactionStatus({
    type: "compact.completed",
    summary: "summary",
    compacted: true,
    coveredThroughItemId: "item_1",
    beforeTokens: 12_500,
    afterTokens: 840
  });

  assert.equal(status, "上下文已压缩：12500 → 840 token");
});

test("compaction status explains a no-op", () => {
  const status = formatCompactionStatus({
    type: "compact.completed",
    summary: "",
    compacted: false,
    beforeTokens: 300,
    afterTokens: 300
  });

  assert.equal(status, "当前没有可压缩的已完成历史");
});

test("hydrated history marks an interrupted partial assistant reply", () => {
  const items: ThreadItem[] = [
    {
      id: "user_1",
      threadId: "thread_1",
      turnId: "turn_1",
      type: "user_message",
      role: "user",
      content: "saved question",
      createdAt: "2026-07-10T00:00:00.000Z"
    },
    {
      id: "assistant_1",
      threadId: "thread_1",
      turnId: "turn_1",
      type: "assistant_message",
      role: "assistant",
      content: "saved partial reply",
      createdAt: "2026-07-10T00:00:01.000Z",
      metadata: {partial: true, interrupted: true}
    }
  ];

  assert.deepEqual(formatHistoryMessages(items), [
    {id: "user_1", role: "user", content: "saved question"},
    {
      id: "assistant_1",
      role: "assistant",
      content: "saved partial reply\n[上次运行在回复完成前中断]"
    }
  ]);
});

test("hydrated history marks a failed partial assistant reply", () => {
  const items: ThreadItem[] = [
    {
      id: "assistant_failed",
      threadId: "thread_1",
      turnId: "turn_1",
      type: "assistant_message",
      role: "assistant",
      content: "recoverable partial",
      createdAt: "2026-07-10T00:00:01.000Z",
      metadata: {partial: true, failed: true}
    }
  ];

  assert.deepEqual(formatHistoryMessages(items), [
    {
      id: "assistant_failed",
      role: "assistant",
      content: "recoverable partial\n[回复在发生错误前停止]"
    }
  ]);
});

test("thread list formatting makes the active conversation obvious", () => {
  const threads: ThreadRecord[] = [
    {
      id: "thread_target",
      title: "Target conversation",
      createdAt: "2026-07-10T00:00:00.000Z",
      updatedAt: "2026-07-10T01:00:00.000Z",
      model: "MiniMax-M2.1",
      cwd: "C:/workspace",
      status: "active"
    },
    {
      id: "thread_old",
      title: "Old conversation",
      createdAt: "2026-07-09T00:00:00.000Z",
      updatedAt: "2026-07-09T01:00:00.000Z",
      model: "MiniMax-M2.1",
      cwd: "C:/workspace",
      status: "archived"
    }
  ];

  assert.equal(
    formatThreadList(threads),
    [
      "历史会话：",
      "* thread_target | active | Target conversation | 2026-07-10T01:00:00.000Z",
      "  thread_old | archived | Old conversation | 2026-07-09T01:00:00.000Z",
      "使用 /resume <threadId> 切换会话。"
    ].join("\n")
  );
});

test("Agent history formatting omits raw tool output", () => {
  const item: ThreadItem = {
    id: "agent-tool-result",
    threadId: "thread-1",
    turnId: "turn-1",
    type: "agent_item",
    content: "Tool result: completed",
    createdAt: "2026-07-14T00:00:00.000Z",
    agent: {schemaVersion: 1, sequence: 2, payload: {kind: "tool_result", invocationId: "inv-1", status: "completed", output: "private raw output"}}
  };
  assert.deepEqual(formatHistoryMessages([item]), [{id: "agent-tool-result", role: "system", content: "Agent 本机能力结果：completed"}]);
});
