import {mkdir} from "node:fs/promises";
import {join} from "node:path";
import type {ContextSummary, ThreadItem, ThreadRecord, TraceEvent, TurnRecord} from "../types.js";
import {appendJsonl, readJsonFile, readJsonl, writeJsonFile} from "../utils/jsonl.js";
import type {StorageProvider} from "./storage-provider.js";

interface ThreadsIndex {
  threads: ThreadRecord[];
}

type StoredTurnEvent =
  | {kind: "turn.snapshot"; turn: TurnRecord}
  | {
      kind: "assistant.delta";
      threadId: string;
      turnId: string;
      delta: string;
      createdAt: string;
    };

export class JsonlStorageProvider implements StorageProvider {
  constructor(private readonly rootDir: string) {}

  async init(): Promise<void> {
    await mkdir(this.rootDir, {recursive: true});
    await mkdir(join(this.rootDir, "sessions"), {recursive: true});
    await mkdir(join(this.rootDir, "history"), {recursive: true});
    await mkdir(join(this.rootDir, "traces"), {recursive: true});
    await mkdir(join(this.rootDir, "summaries"), {recursive: true});
    await mkdir(join(this.rootDir, "turns"), {recursive: true});
    await mkdir(join(this.rootDir, "indexes"), {recursive: true});
    await mkdir(join(this.rootDir, "db"), {recursive: true});
    await this.ensureThreadIndex();
  }

  async createThread(thread: ThreadRecord): Promise<void> {
    const index = await this.readThreadIndex();
    const existing = index.threads.find((item) => item.id === thread.id);
    if (!existing) {
      const remaining =
        thread.status === "active"
          ? index.threads.map((item): ThreadRecord =>
              item.status === "active" ? {...item, status: "archived"} : item
            )
          : index.threads;
      await this.writeThreadIndex({threads: [thread, ...remaining]});
    }
  }

  async updateThread(thread: ThreadRecord): Promise<void> {
    const index = await this.readThreadIndex();
    const nextThreads = index.threads.filter((item) => item.id !== thread.id);
    nextThreads.unshift(thread);
    await this.writeThreadIndex({threads: nextThreads});
  }

  async activateThread(threadId: string, activatedAt: string): Promise<ThreadRecord | null> {
    const index = await this.readThreadIndex();
    const target = index.threads.find((thread) => thread.id === threadId);
    if (!target) {
      return null;
    }

    const activated: ThreadRecord = {
      ...target,
      status: "active",
      updatedAt: activatedAt
    };
    const remaining = index.threads
      .filter((thread) => thread.id !== threadId)
      .map((thread): ThreadRecord =>
        thread.status === "active" ? {...thread, status: "archived"} : thread
      );
    await this.writeThreadIndex({threads: [activated, ...remaining]});
    return activated;
  }

  async appendTurn(turn: TurnRecord): Promise<void> {
    await appendJsonl(this.turnPath(turn.threadId), {
      kind: "turn.snapshot",
      turn
    } satisfies StoredTurnEvent);
  }

  async appendTurnDelta(
    threadId: string,
    turnId: string,
    delta: string,
    createdAt: string
  ): Promise<void> {
    await appendJsonl(this.turnPath(threadId), {
      kind: "assistant.delta",
      threadId,
      turnId,
      delta,
      createdAt
    } satisfies StoredTurnEvent);
  }

  async readTurns(threadId: string): Promise<TurnRecord[]> {
    const events = await readJsonl<StoredTurnEvent>(this.turnPath(threadId));
    const turns = new Map<string, TurnRecord>();
    const drafts = new Map<string, string>();

    for (const event of events) {
      if (event.kind === "turn.snapshot") {
        turns.set(event.turn.id, event.turn);
        continue;
      }
      drafts.set(event.turnId, `${drafts.get(event.turnId) ?? ""}${event.delta}`);
    }

    return [...turns.values()]
      .map((turn) => {
        const assistantDraft = drafts.get(turn.id) ?? turn.assistantDraft;
        return assistantDraft === undefined ? turn : {...turn, assistantDraft};
      })
      .sort((left, right) => left.startedAt.localeCompare(right.startedAt));
  }

  async appendItem(item: ThreadItem): Promise<void> {
    await appendJsonl(await this.sessionPath(item.threadId), item);
    if (item.type === "user_message") {
      await appendJsonl(join(this.rootDir, "history", "input-history.jsonl"), {
        threadId: item.threadId,
        turnId: item.turnId,
        text: item.content,
        ts: item.createdAt
      });
    }
  }

  async appendTrace(event: TraceEvent): Promise<void> {
    await appendJsonl(await this.tracePath(event.threadId), event);
  }

  async appendSummary(summary: ContextSummary): Promise<void> {
    await appendJsonl(this.summaryPath(summary.threadId), summary);
  }

  async readThreadItems(threadId: string): Promise<ThreadItem[]> {
    return readJsonl<ThreadItem>(await this.sessionPath(threadId));
  }

  async readSummaries(threadId: string): Promise<ContextSummary[]> {
    return readJsonl<ContextSummary>(this.summaryPath(threadId));
  }

  async listThreads(): Promise<ThreadRecord[]> {
    return (await this.readThreadIndex()).threads;
  }

  private async sessionPath(threadId: string): Promise<string> {
    const date = await this.threadDate(threadId);
    return join(
      this.rootDir,
      "sessions",
      String(date.getFullYear()),
      String(date.getMonth() + 1).padStart(2, "0"),
      String(date.getDate()).padStart(2, "0"),
      `${threadId}.jsonl`
    );
  }

  private async tracePath(threadId: string): Promise<string> {
    const date = await this.threadDate(threadId);
    return join(
      this.rootDir,
      "traces",
      String(date.getFullYear()),
      String(date.getMonth() + 1).padStart(2, "0"),
      String(date.getDate()).padStart(2, "0"),
      `${threadId}.trace.jsonl`
    );
  }

  private async threadDate(threadId: string): Promise<Date> {
    const index = await this.readThreadIndex();
    const thread = index.threads.find((item) => item.id === threadId);
    return thread ? new Date(thread.createdAt) : new Date();
  }

  private summaryPath(threadId: string): string {
    return join(this.rootDir, "summaries", `${threadId}.summary.jsonl`);
  }

  private turnPath(threadId: string): string {
    return join(this.rootDir, "turns", `${threadId}.turns.jsonl`);
  }

  private indexPath(): string {
    return join(this.rootDir, "indexes", "threads.json");
  }

  private async ensureThreadIndex(): Promise<void> {
    await readJsonFile<ThreadsIndex>(this.indexPath(), {threads: []}, {
      validate: isThreadsIndex
    });
    const index = await this.readThreadIndex();
    await this.writeThreadIndex(index);
  }

  private async readThreadIndex(): Promise<ThreadsIndex> {
    return readJsonFile<ThreadsIndex>(this.indexPath(), {threads: []}, {
      validate: isThreadsIndex
    });
  }

  private async writeThreadIndex(index: ThreadsIndex): Promise<void> {
    await writeJsonFile(this.indexPath(), index);
  }
}

function isThreadsIndex(value: unknown): value is ThreadsIndex {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    return false;
  }
  const threads = (value as {threads?: unknown}).threads;
  if (!Array.isArray(threads) || !threads.every(isThreadRecord)) {
    return false;
  }
  return threads.filter((thread) => thread.status === "active").length <= 1;
}

function isThreadRecord(value: unknown): value is ThreadRecord {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    return false;
  }
  const thread = value as Record<string, unknown>;
  return (
    typeof thread.id === "string" &&
    typeof thread.title === "string" &&
    typeof thread.createdAt === "string" &&
    typeof thread.updatedAt === "string" &&
    typeof thread.model === "string" &&
    typeof thread.cwd === "string" &&
    (thread.status === "active" || thread.status === "archived")
  );
}
