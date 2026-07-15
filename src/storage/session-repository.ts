import type {ContextSummary, ThreadItem, ThreadRecord, TraceEvent, TurnRecord} from "../types.js";

export interface RepositoryInitResult {
  schemaVersion: 1;
  migrated: boolean;
}

export interface TurnDeltaBatch {
  threadId: string;
  turnId: string;
  delta: string;
  createdAt: string;
}

export interface ThreadSnapshot {
  thread: ThreadRecord | null;
  turns: TurnRecord[];
  items: ThreadItem[];
  traces: TraceEvent[];
  summaries: ContextSummary[];
}

export interface SessionRepository {
  init(): Promise<RepositoryInitResult>;
  createThread(thread: ThreadRecord): Promise<void>;
  updateThread(thread: ThreadRecord): Promise<void>;
  activateThread(threadId: string, activatedAt: string): Promise<ThreadRecord | null>;
  appendTurnSnapshot(turn: TurnRecord): Promise<void>;
  appendTurnDelta(batch: TurnDeltaBatch): Promise<void>;
  checkpointTurns(threadId: string): Promise<void>;
  appendItem(item: ThreadItem): Promise<void>;
  appendTrace(event: TraceEvent): Promise<void>;
  appendSummary(summary: ContextSummary): Promise<void>;
  readThread(threadId: string): Promise<ThreadSnapshot>;
  listThreads(): Promise<ThreadRecord[]>;
}
