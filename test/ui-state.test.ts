import assert from "node:assert/strict";
import test from "node:test";
import type {RuntimeEvent} from "../src/protocol.js";
import {
  initialUiState,
  reduceRuntimeEvent
} from "../src/ui/ui-state.js";
import type {
  ThreadItem,
  ThreadRecord,
  TraceEvent,
  TurnRecord
} from "../src/types.js";

const READY_EVENT: RuntimeEvent = {
  type: "runtime.ready",
  hasApiKey: true,
  providerSummary: "minimax-official | responses | model=MiniMax-M3",
  recoveredTurns: 0
};

const THREAD: ThreadRecord = {
  id: "thread_1",
  title: "UI state",
  createdAt: "2026-07-10T00:00:00.000Z",
  updatedAt: "2026-07-10T00:00:00.000Z",
  model: "MiniMax-M3",
  cwd: "C:/workspace",
  status: "active"
};

const TURN: TurnRecord = {
  id: "turn_1",
  threadId: THREAD.id,
  userInput: "hello",
  status: "running",
  startedAt: "2026-07-10T00:00:00.000Z"
};

const ITEM: ThreadItem = {
  id: "assistant_1",
  threadId: THREAD.id,
  turnId: TURN.id,
  type: "assistant_message",
  role: "assistant",
  content: "complete",
  createdAt: "2026-07-10T00:00:01.000Z"
};

const TRACE: TraceEvent = {
  id: "trace_1",
  threadId: THREAD.id,
  turnId: TURN.id,
  category: "lifecycle",
  code: "turn.start",
  message: "Turn started.",
  createdAt: "2026-07-10T00:00:00.000Z"
};

test("runtime.ready is the only event that leaves booting", () => {
  const initial = initialUiState();
  const failed = reduceRuntimeEvent(initial, {
    type: "error",
    message: "init failed"
  });

  assert.equal(failed.phase, "booting");
  assert.match(failed.status, /init failed/i);
  assert.equal(reduceRuntimeEvent(initial, READY_EVENT).phase, "idle");
});

test("runtime.ready with a missing key waits for the API request flow", () => {
  const state = reduceRuntimeEvent(initialUiState(), {
    ...READY_EVENT,
    hasApiKey: false
  });

  assert.equal(state.phase, "idle");
  assert.equal(state.inputMode, "api_setup_required");
  assert.match(state.status, /\/api/);
});

test("ordinary RuntimeEvents cannot leave booting", () => {
  const events: RuntimeEvent[] = [
    {type: "thread.loaded", thread: THREAD},
    {type: "thread.listed", threads: [THREAD]},
    {type: "history.loaded", items: [ITEM]},
    {type: "turn.started", turnId: TURN.id, input: TURN.userInput},
    {type: "turn.recovered", turn: TURN},
    {type: "turn.interrupt.requested", turnId: TURN.id},
    {type: "turn.interrupt.ignored", reason: "no_active_request"},
    {type: "turn.interrupted", turnId: TURN.id},
    {type: "assistant.delta", turnId: TURN.id, delta: "partial"},
    {type: "assistant.completed", item: ITEM},
    {type: "trace.event", event: TRACE},
    {type: "token.usage", used: 10, limit: 100, autoCompactAt: 80},
    {type: "compact.started", reason: "manual"},
    {
      type: "compact.completed",
      summary: "summary",
      compacted: true,
      beforeTokens: 100,
      afterTokens: 20
    },
    {type: "api.status", status: "requesting"},
    {type: "config.api_key.requested", providerSummary: "minimax-official"},
    {
      type: "config.legacy_credential.reentry_required",
      path: "C:/workspace/.mini-codex/secrets.local.json",
      hasUsableCredential: false
    },
    {
      type: "config.api_key.plaintext_confirmation_required",
      path: "C:/Users/test/credentials.json"
    },
    {
      type: "config.api_key.plaintext_confirmed",
      providerSummary: "minimax-official"
    },
    {
      type: "config.api_key.saved",
      location: "user-file",
      providerSummary: "minimax-official"
    },
    {
      type: "provider.listed",
      current: "minimax-official",
      providers: ["minimax-official"]
    },
    {
      type: "provider.changed",
      summary: "hashsight",
      hasApiKey: false
    },
    {type: "trace.toggle.requested"},
    {
      type: "command.rejected",
      commandType: "turn.submit",
      phase: "booting",
      message: "Runtime is booting."
    },
    {type: "error", message: "init failed"}
  ];

  for (const event of events) {
    assert.equal(
      reduceRuntimeEvent(initialUiState(), event).phase,
      "booting",
      event.type
    );
  }
});

test("legacy credential re-entry warning is visible without exposing a secret", () => {
  const path = "C:/workspace/.mini-codex/secrets.local.json";
  const warning = reduceRuntimeEvent(initialUiState(), {
    type: "config.legacy_credential.reentry_required",
    path,
    hasUsableCredential: false
  });

  assert.equal(warning.phase, "booting");
  assert.equal(warning.messages.at(-1)?.content.includes(path), true);
  assert.match(warning.messages.at(-1)?.content ?? "", /\/api/);
  assert.equal(JSON.stringify(warning).includes("workspace-secret"), false);
});

test("initialization failure enters recovery and replaces the stable startup error", () => {
  const first = reduceRuntimeEvent(initialUiState(), {
    type: "runtime.init_failed",
    message: "workspace is busy"
  });
  const second = reduceRuntimeEvent(first, {
    type: "runtime.init_failed",
    message: "configuration is invalid"
  });

  assert.equal(second.phase, "init_failed");
  assert.equal(second.inputMode, "init_recovery");
  assert.equal(
    second.messages.filter((message) => message.id === "runtime-init-failed").length,
    1
  );
  assert.equal(
    second.messages.find((message) => message.id === "runtime-init-failed")?.content,
    "configuration is invalid"
  );
});

test("retry returns recovery state to disabled booting", () => {
  const failed = reduceRuntimeEvent(initialUiState(), {
    type: "runtime.init_failed",
    message: "failed"
  });
  const retrying = reduceRuntimeEvent(failed, {type: "ui.init.retrying"});

  assert.equal(retrying.phase, "booting");
  assert.equal(retrying.inputMode, "disabled");
});

test("runtime ready removes the stable initialization failure message", () => {
  const failed = reduceRuntimeEvent(initialUiState(), {
    type: "runtime.init_failed",
    message: "workspace is busy"
  });
  const retrying = reduceRuntimeEvent(failed, {type: "ui.init.retrying"});
  const ready = reduceRuntimeEvent(retrying, READY_EVENT);

  assert.equal(ready.phase, "idle");
  assert.equal(ready.inputMode, "chat");
  assert.equal(
    ready.messages.some((message) => message.id === "runtime-init-failed"),
    false
  );
});

test("stopped UI ignores late initialization results", () => {
  const stopped = reduceRuntimeEvent(initialUiState(), {type: "app.exit.requested"});
  const lateReady = reduceRuntimeEvent(stopped, {
    type: "runtime.ready",
    hasApiKey: true,
    providerSummary: "late",
    recoveredTurns: 0
  });
  const lateFailure = reduceRuntimeEvent(stopped, {
    type: "runtime.init_failed",
    message: "late failure"
  });

  assert.deepEqual(lateReady, stopped);
  assert.deepEqual(lateFailure, stopped);
});

test("plaintext warning shows its absolute path before API input", () => {
  const path = "C:/Users/test/credentials.json";
  const warning = reduceRuntimeEvent(initialUiState(), {
    type: "config.api_key.plaintext_confirmation_required",
    path
  });

  assert.equal(warning.phase, "booting");
  assert.equal(warning.inputMode, "confirming_plaintext");
  assert.match(warning.status, /plaintext|明文/i);
  assert.equal(warning.status.includes(path), true);

  const confirmed = reduceRuntimeEvent(warning, {
    type: "config.api_key.plaintext_confirmed",
    providerSummary: "minimax-official"
  });
  assert.equal(confirmed.phase, "booting");
  assert.equal(confirmed.inputMode, "entering_api_key");
});

test("a local plaintext cancellation preserves booting and requires API setup", () => {
  const warning = reduceRuntimeEvent(initialUiState(), {
    type: "config.api_key.plaintext_confirmation_required",
    path: "C:/Users/test/credentials.json"
  });

  const cancelled = reduceRuntimeEvent(warning, {
    type: "ui.plaintext.cancelled"
  });

  assert.equal(cancelled.phase, "booting");
  assert.equal(cancelled.inputMode, "api_setup_required");
  assert.match(cancelled.status, /cancel/i);
});

test("turn events preserve stable user and assistant identities", () => {
  const ready = reduceRuntimeEvent(initialUiState(), READY_EVENT);
  const started = reduceRuntimeEvent(ready, {
    type: "turn.started",
    turnId: "turn_123",
    input: "hello"
  });
  const streamed = reduceRuntimeEvent(started, {
    type: "assistant.delta",
    turnId: "turn_123",
    delta: "partial"
  });
  const completed = reduceRuntimeEvent(streamed, {
    type: "assistant.completed",
    item: {
      id: "persisted_assistant",
      threadId: "thread_1",
      turnId: "turn_123",
      type: "assistant_message",
      role: "assistant",
      content: "complete",
      createdAt: "2026-07-10T00:00:00.000Z"
    }
  });

  assert.equal(started.phase, "running");
  assert.deepEqual(
    completed.messages.slice(-2).map(({id, content}) => ({id, content})),
    [
      {id: "user-turn_123", content: "hello"},
      {id: "assistant-turn_123", content: "complete"}
    ]
  );
  assert.equal(completed.phase, "idle");
});

test("command rejection owns busy feedback without changing the display phase", () => {
  const ready = reduceRuntimeEvent(initialUiState(), READY_EVENT);
  const running = reduceRuntimeEvent(ready, {
    type: "turn.started",
    turnId: "turn_busy",
    input: "first"
  });
  const rejected = reduceRuntimeEvent(running, {
    type: "command.rejected",
    commandType: "turn.submit",
    phase: "running_turn",
    message: "Another command is already running."
  });

  assert.equal(rejected.phase, "running");
  assert.equal(rejected.status, "Another command is already running.");
});

test("the reducer is immutable and app exit stops the UI", () => {
  const initial = initialUiState();
  const next = reduceRuntimeEvent(initial, {type: "trace.toggle.requested"});

  assert.equal(initial.traceOpen, false);
  assert.equal(next.traceOpen, true);
  assert.equal(
    reduceRuntimeEvent(
      reduceRuntimeEvent(next, READY_EVENT),
      {type: "app.exit.requested"}
    ).phase,
    "stopped"
  );
});
