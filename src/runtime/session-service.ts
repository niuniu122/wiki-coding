import type {RuntimeEvent} from "../protocol.js";
import type {SessionRepository} from "../storage/session-repository.js";
import type {
  ThreadItem,
  ThreadRecord,
  TraceCode,
  TurnRecord,
  TurnModelProvenance,
  TurnStatus
} from "../types.js";
import {createId} from "../utils/id.js";
import {SafeTraceRecorder} from "./trace-recorder.js";
import {createAgentItemEnvelope, type AgentItemPayload} from "../agent/agent-item.js";
import {auditAgentItems} from "../agent/agent-checkpoint.js";

export interface CreateItemParams {
  turnId: string;
  type: ThreadItem["type"];
  content: string;
  role?: ThreadItem["role"];
  metadata?: Record<string, unknown>;
}

export interface CreateAgentItemParams {
  turnId: string;
  sequence: number;
  payload: AgentItemPayload;
}

export interface SessionTransition {
  thread: ThreadRecord;
  events: RuntimeEvent[];
}

const DEFAULT_THREAD_TITLE = "MiniMax Codex Session";

export class SessionService {
  private currentThread: ThreadRecord | undefined;

  constructor(
    private readonly repository: SessionRepository,
    private readonly traceRecorder: SafeTraceRecorder = new SafeTraceRecorder()
  ) {}

  async init(model: string, cwd: string): Promise<RuntimeEvent[]> {
    await this.repository.init();
    const threads = await this.repository.listThreads();
    const existing = threads.find((thread) => thread.status === "active");
    if (existing) {
      this.currentThread = existing;
    } else {
      this.currentThread = this.buildThread(model, cwd);
      await this.repository.createThread(this.currentThread);
    }

    const recoveryEvents = await this.recoverRunningTurns();
    const snapshot = await this.repository.readThread(this.activeThread.id);
    return [
      {type: "thread.loaded", thread: this.activeThread},
      {type: "history.loaded", items: snapshot.items},
      ...recoveryEvents
    ];
  }

  get activeThread(): ThreadRecord {
    if (!this.currentThread) {
      throw new Error("SessionService has not been initialized.");
    }
    return this.currentThread;
  }

  async newThread(model: string, cwd: string): Promise<SessionTransition> {
    const thread = this.buildThread(model, cwd);
    await this.repository.createThread(thread);
    this.currentThread = thread;
    return {
      thread,
      events: [
        {type: "thread.loaded", thread},
        {type: "history.loaded", items: []}
      ]
    };
  }

  listThreads(): Promise<ThreadRecord[]> {
    return this.repository.listThreads();
  }

  async resumeThread(threadId: string): Promise<SessionTransition> {
    const thread = await this.repository.activateThread(
      threadId,
      new Date().toISOString()
    );
    if (!thread) {
      throw new Error(`历史会话不存在：${threadId}`);
    }

    this.currentThread = thread;
    const recoveryEvents = await this.recoverRunningTurns();
    const snapshot = await this.repository.readThread(thread.id);
    return {
      thread: this.activeThread,
      events: [
        {type: "thread.loaded", thread: this.activeThread},
        {type: "history.loaded", items: snapshot.items},
        ...recoveryEvents
      ]
    };
  }

  async createTurn(
    input: string,
    modelProvenance?: TurnModelProvenance
  ): Promise<TurnRecord> {
    const now = new Date().toISOString();
    const activeThread = this.activeThread;
    this.currentThread = {
      ...activeThread,
      title:
        activeThread.title === DEFAULT_THREAD_TITLE
          ? input.slice(0, 40)
          : activeThread.title,
      updatedAt: now
    };
    await this.repository.updateThread(this.currentThread);

    const turn: TurnRecord = {
      id: createId("turn"),
      threadId: this.currentThread.id,
      userInput: input,
      status: "running",
      startedAt: now,
      ...(modelProvenance ? {modelProvenance} : {})
    };
    await this.repository.appendTurnSnapshot(turn);
    return turn;
  }

  async completeTurn(turn: TurnRecord, status: TurnStatus): Promise<void> {
    const completedAt = new Date().toISOString();
    turn.status = status;
    turn.completedAt = completedAt;
    await this.repository.appendTurnSnapshot(turn);
    this.currentThread = {...this.activeThread, updatedAt: completedAt};
    await this.repository.updateThread(this.currentThread);
  }

  createItem(params: CreateItemParams): ThreadItem {
    const item: ThreadItem = {
      id: createId("item"),
      threadId: this.activeThread.id,
      turnId: params.turnId,
      type: params.type,
      content: params.content,
      createdAt: new Date().toISOString()
    };
    if (params.role !== undefined) {
      item.role = params.role;
    }
    if (params.metadata !== undefined) {
      item.metadata = params.metadata;
    }
    return item;
  }

  createAgentItem(params: CreateAgentItemParams): ThreadItem {
    return {
      id: createId("item"),
      threadId: this.activeThread.id,
      turnId: params.turnId,
      type: "agent_item",
      content: summarizeAgentPayload(params.payload),
      createdAt: new Date().toISOString(),
      agent: createAgentItemEnvelope(params.sequence, params.payload)
    };
  }

  private buildThread(model: string, cwd: string): ThreadRecord {
    const now = new Date().toISOString();
    return {
      id: createId("thread"),
      title: DEFAULT_THREAD_TITLE,
      createdAt: now,
      updatedAt: now,
      model,
      cwd,
      status: "active"
    };
  }

  private async recoverRunningTurns(): Promise<RuntimeEvent[]> {
    const snapshot = await this.repository.readThread(this.activeThread.id);
    const runningTurns = snapshot.turns.filter((turn) => turn.status === "running");
    if (runningTurns.length === 0) {
      return [];
    }

    const items = [...snapshot.items];
    const events: RuntimeEvent[] = [];
    for (const turn of runningTurns) {
      const agentItems = items.filter((item) => item.turnId === turn.id && item.type === "agent_item");
      if (agentItems.length > 0) {
        const audit = auditAgentItems(turn, agentItems);
        let sequence = audit.nextSequence;
        for (const invocationId of audit.unmatchedInvocationIds) {
          const indeterminate = this.createAgentItem({
            turnId: turn.id,
            sequence: sequence++,
            payload: {
              kind: "tool_result",
              invocationId,
              status: "indeterminate",
              output: "Execution outcome is unknown after recovery; this invocation was not replayed."
            }
          });
          await this.repository.appendItem(indeterminate);
          items.push(indeterminate);
        }
        await this.completeTurn(turn, "interrupted");
        if (audit.status === "recoverable" && audit.checkpoint) {
          events.push({type: "agent.recovery.available", turnId: turn.id, checkpointId: audit.checkpoint.checkpointId});
        } else {
          events.push({type: "agent.recovery.blocked", turnId: turn.id, reason: audit.unmatchedInvocationIds.length > 0 ? "indeterminate_invocation" : "invalid_checkpoint"});
        }
        continue;
      }
      const alreadyRecovered = items.some(
        (item) => item.metadata?.recoveredTurnId === turn.id
      );
      if (turn.assistantDraft && !alreadyRecovered) {
        const partialItem = this.createItem({
          turnId: turn.id,
          type: "assistant_message",
          role: "assistant",
          content: turn.assistantDraft,
          metadata: {
            partial: true,
            interrupted: true,
            recoveredTurnId: turn.id
          }
        });
        await this.repository.appendItem(partialItem);
        items.push(partialItem);
      }

      await this.completeTurn(turn, "interrupted");
      events.push(
        await this.appendTrace("turn.recovered", {
          turnId: turn.id,
          hadAssistantDraft: Boolean(turn.assistantDraft)
        })
      );
      events.push({type: "turn.recovered", turn});
    }

    await this.repository.checkpointTurns(this.activeThread.id);
    return events;
  }

  private async appendTrace(
    code: TraceCode,
    facts: Record<string, unknown>
  ): Promise<RuntimeEvent> {
    const event = this.traceRecorder.create(this.activeThread.id, code, facts);
    await this.repository.appendTrace(event);
    return {type: "trace.event", event};
  }
}

function summarizeAgentPayload(payload: AgentItemPayload): string {
  switch (payload.kind) {
    case "user": case "assistant": case "final": return payload.text.slice(0, 4_000);
    case "tool_request": return `Tool request: ${payload.capabilityId}`;
    case "tool_result": return `Tool result: ${payload.status}`;
    case "checkpoint": return `Checkpoint: ${payload.checkpointId}`;
    case "error": return `Agent error: ${payload.code}`;
  }
}
