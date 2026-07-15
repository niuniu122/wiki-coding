import type {ContextSummary, ThreadItem, TurnRecord} from "../types.js";
import type {RepositoryInitResult, SessionRepository} from "./session-repository.js";

// Compatibility contract for callers that still use the pre-rewrite storage
// method names. New runtime services depend on SessionRepository directly.

export interface StorageProvider
  extends Pick<
    SessionRepository,
    | "createThread"
    | "updateThread"
    | "activateThread"
    | "appendItem"
    | "appendTrace"
    | "appendSummary"
    | "listThreads"
  > {
  init(): Promise<void | RepositoryInitResult>;
  appendTurn(turn: TurnRecord): Promise<void>;
  appendTurnDelta(
    threadId: string,
    turnId: string,
    delta: string,
    createdAt: string
  ): Promise<void>;
  readTurns(threadId: string): Promise<TurnRecord[]>;
  readThreadItems(threadId: string): Promise<ThreadItem[]>;
  readSummaries(threadId: string): Promise<ContextSummary[]>;
}
