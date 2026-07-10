import type {ContextSummary, ThreadItem, ThreadRecord, TraceEvent, TurnRecord} from "../types.js";
import type {StorageProvider} from "./storage-provider.js";

export class SqliteStorageProvider implements StorageProvider {
  async init(): Promise<void> {
    throw new Error("SQLite storage is reserved for a later version. Use storage.driver=jsonl for now.");
  }

  async createThread(_thread: ThreadRecord): Promise<void> {
    throw this.notImplemented();
  }

  async updateThread(_thread: ThreadRecord): Promise<void> {
    throw this.notImplemented();
  }

  async activateThread(_threadId: string, _activatedAt: string): Promise<ThreadRecord | null> {
    throw this.notImplemented();
  }

  async appendTurn(_turn: TurnRecord): Promise<void> {
    throw this.notImplemented();
  }

  async appendTurnDelta(
    _threadId: string,
    _turnId: string,
    _delta: string,
    _createdAt: string
  ): Promise<void> {
    throw this.notImplemented();
  }

  async readTurns(_threadId: string): Promise<TurnRecord[]> {
    throw this.notImplemented();
  }

  async appendItem(_item: ThreadItem): Promise<void> {
    throw this.notImplemented();
  }

  async appendTrace(_event: TraceEvent): Promise<void> {
    throw this.notImplemented();
  }

  async appendSummary(_summary: ContextSummary): Promise<void> {
    throw this.notImplemented();
  }

  async readThreadItems(_threadId: string): Promise<ThreadItem[]> {
    throw this.notImplemented();
  }

  async readSummaries(_threadId: string): Promise<ContextSummary[]> {
    throw this.notImplemented();
  }

  async listThreads(): Promise<ThreadRecord[]> {
    throw this.notImplemented();
  }

  private notImplemented(): Error {
    return new Error("SQLite storage interface is present, but the first version only enables JSONL storage.");
  }
}
