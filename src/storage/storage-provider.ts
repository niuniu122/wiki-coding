import type {ContextSummary, ThreadItem, ThreadRecord, TraceEvent, TurnRecord} from "../types.js";

export interface StorageProvider {
  init(): Promise<void>;
  createThread(thread: ThreadRecord): Promise<void>;
  updateThread(thread: ThreadRecord): Promise<void>;
  activateThread(threadId: string, activatedAt: string): Promise<ThreadRecord | null>;
  appendTurn(turn: TurnRecord): Promise<void>;
  appendTurnDelta(
    threadId: string,
    turnId: string,
    delta: string,
    createdAt: string
  ): Promise<void>;
  readTurns(threadId: string): Promise<TurnRecord[]>;
  appendItem(item: ThreadItem): Promise<void>;
  appendTrace(event: TraceEvent): Promise<void>;
  appendSummary(summary: ContextSummary): Promise<void>;
  readThreadItems(threadId: string): Promise<ThreadItem[]>;
  readSummaries(threadId: string): Promise<ContextSummary[]>;
  listThreads(): Promise<ThreadRecord[]>;
}
