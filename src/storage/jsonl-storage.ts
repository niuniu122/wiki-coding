import {randomUUID} from "node:crypto";
import {mkdir, readFile, readdir, rename, rm, stat} from "node:fs/promises";
import {join, relative, resolve} from "node:path";
import type {ContextSummary, ThreadItem, ThreadRecord, TraceEvent, TurnRecord} from "../types.js";
import {
  appendJsonl,
  readJsonFile,
  readJsonl,
  writeJsonFile,
  writeJsonlFile,
  writeTextFileAtomic,
  type WriteJsonFileOptions
} from "../utils/jsonl.js";
import type {
  RepositoryInitResult,
  SessionRepository,
  ThreadSnapshot,
  TurnDeltaBatch
} from "./session-repository.js";
import {
  createStoredEnvelope,
  inspectStoredEnvelope,
  unwrapStoredRecord,
  type StoredEnvelope,
  type Validator
} from "./storage-envelope.js";
import type {StorageProvider} from "./storage-provider.js";
import {isAgentItemEnvelope} from "../agent/agent-item.js";

interface ThreadsIndex {
  threads: ThreadRecord[];
}

interface StorageManifest {
  schemaVersion: 1;
  storage: "jsonl";
  createdAt: string;
}

interface InputHistoryRecord {
  threadId: string;
  turnId?: string;
  text: string;
  ts: string;
}

interface DecodedRecord {
  kind: string;
  payload: unknown;
  createdAt: string;
  sequence: number | null;
}

interface PreparedMigration {
  filePath: string;
  tempPath: string;
  backupPath: string;
  legacyRaw: string;
  originalMoved: boolean;
}

export interface JsonlStorageOperations {
  appendJsonl(filePath: string, value: unknown): Promise<void>;
  writeJsonlFile(filePath: string, values: readonly unknown[]): Promise<void>;
  writeTextFileAtomic(filePath: string, content: string, mode?: number): Promise<void>;
  writeJsonFile<T>(
    filePath: string,
    value: T,
    options?: WriteJsonFileOptions
  ): Promise<void>;
  rename: typeof rename;
  rm: typeof rm;
}

const JSONL_DIRECTORIES = ["sessions", "history", "traces", "summaries", "turns"] as const;
const DEFAULT_OPERATIONS: JsonlStorageOperations = {
  appendJsonl,
  writeJsonlFile,
  writeTextFileAtomic,
  writeJsonFile,
  rename,
  rm
};

export class JsonlStorageProvider implements StorageProvider, SessionRepository {
  private readonly operations: JsonlStorageOperations;
  private readonly fileMutationTails = new Map<string, Promise<void>>();

  constructor(
    private readonly rootDir: string,
    operations: Partial<JsonlStorageOperations> = {}
  ) {
    this.operations = {...DEFAULT_OPERATIONS, ...operations};
  }

  async init(): Promise<RepositoryInitResult> {
    const manifest = await this.readManifest();
    if (manifest) {
      await this.validateCurrentJsonlFiles();
      await this.ensureLayout();
      await this.ensureThreadIndex();
      return {schemaVersion: 1, migrated: false};
    }

    await this.recoverInterruptedMigration();
    const legacyFiles = await this.findLegacyJsonlFiles();
    await this.validateLegacyThreadIndex();
    if (legacyFiles.length > 0) {
      await this.migrateLegacyFiles(legacyFiles);
      return {schemaVersion: 1, migrated: true};
    }

    await this.ensureLayout();
    await this.ensureThreadIndex();
    await this.writeManifest();
    return {schemaVersion: 1, migrated: false};
  }

  async createThread(thread: ThreadRecord): Promise<void> {
    assertValid(thread, isThreadRecord, "thread");
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
    assertValid(thread, isThreadRecord, "thread");
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

  async appendTurnSnapshot(turn: TurnRecord): Promise<void> {
    assertValid(turn, isTurnRecord, "turn snapshot");
    await this.appendVersionedRecord(
      this.turnPath(turn.threadId),
      "turn.snapshot",
      turn,
      turn.completedAt ?? turn.startedAt
    );
  }

  async appendTurn(turn: TurnRecord): Promise<void> {
    await this.appendTurnSnapshot(turn);
  }

  async appendTurnDelta(batch: TurnDeltaBatch): Promise<void>;
  async appendTurnDelta(
    threadId: string,
    turnId: string,
    delta: string,
    createdAt: string
  ): Promise<void>;
  async appendTurnDelta(
    batchOrThreadId: TurnDeltaBatch | string,
    turnId?: string,
    delta?: string,
    createdAt?: string
  ): Promise<void> {
    const batch: TurnDeltaBatch =
      typeof batchOrThreadId === "string"
        ? {
            threadId: batchOrThreadId,
            turnId: turnId ?? "",
            delta: delta ?? "",
            createdAt: createdAt ?? ""
          }
        : batchOrThreadId;
    assertValid(batch, isTurnDeltaBatch, "assistant delta");
    await this.appendVersionedRecord(
      this.turnPath(batch.threadId),
      "assistant.delta",
      batch,
      batch.createdAt
    );
  }

  async checkpointTurns(threadId: string): Promise<void> {
    const filePath = this.turnPath(threadId);
    await this.serializeFileMutation(filePath, async () => {
      const turns = await this.readTurns(threadId);
      const envelopes = turns.map((turn, index) =>
        createStoredEnvelope(
          "turn.snapshot",
          turn,
          index + 1,
          turn.completedAt ?? turn.startedAt
        )
      );
      await this.operations.writeJsonlFile(filePath, envelopes);
    });
  }

  async readTurns(threadId: string): Promise<TurnRecord[]> {
    const decoded = await this.readDecodedFile(this.turnPath(threadId));
    const turns = new Map<string, TurnRecord>();
    const drafts = new Map<string, string>();

    for (const record of decoded) {
      if (record.kind === "turn.snapshot") {
        const turn = record.payload as TurnRecord;
        if (turn.threadId !== threadId) {
          throw new Error(`Turn ${turn.id} does not belong to thread ${threadId}.`);
        }
        const priorDraft = drafts.get(turn.id);
        turns.set(
          turn.id,
          priorDraft === undefined || turn.assistantDraft !== undefined
            ? turn
            : {...turn, assistantDraft: priorDraft}
        );
        if (turn.assistantDraft !== undefined) {
          drafts.set(turn.id, turn.assistantDraft);
        }
        continue;
      }
      const batch = record.payload as TurnDeltaBatch;
      if (batch.threadId !== threadId) {
        throw new Error(`Turn delta ${batch.turnId} does not belong to thread ${threadId}.`);
      }
      drafts.set(batch.turnId, `${drafts.get(batch.turnId) ?? ""}${batch.delta}`);
    }

    return [...turns.values()]
      .map((turn) => {
        const assistantDraft = drafts.get(turn.id) ?? turn.assistantDraft;
        return assistantDraft === undefined ? turn : {...turn, assistantDraft};
      })
      .sort((left, right) => left.startedAt.localeCompare(right.startedAt));
  }

  async appendItem(item: ThreadItem): Promise<void> {
    assertValid(item, isThreadItem, "thread item");
    await this.appendVersionedRecord(
      await this.sessionPath(item.threadId),
      "thread.item",
      item,
      item.createdAt
    );
    if (item.type === "user_message") {
      const history: InputHistoryRecord = {
        threadId: item.threadId,
        ...(item.turnId === undefined ? {} : {turnId: item.turnId}),
        text: item.content,
        ts: item.createdAt
      };
      await this.appendVersionedRecord(
        join(this.rootDir, "history", "input-history.jsonl"),
        "history.input",
        history,
        item.createdAt
      );
    }
  }

  async appendTrace(event: TraceEvent): Promise<void> {
    assertValid(event, isTraceEvent, "trace event");
    await this.appendVersionedRecord(
      await this.tracePath(event.threadId),
      "trace.event",
      event,
      event.createdAt
    );
  }

  async appendSummary(summary: ContextSummary): Promise<void> {
    assertValid(summary, isContextSummary, "context summary");
    await this.appendVersionedRecord(
      this.summaryPath(summary.threadId),
      "context.summary",
      summary,
      summary.createdAt
    );
  }

  async readThreadItems(threadId: string): Promise<ThreadItem[]> {
    const records = await this.readDecodedFile(await this.sessionPath(threadId));
    const items = records.map((record) => record.payload as ThreadItem);
    if (items.some((item) => item.threadId !== threadId)) {
      throw new Error(`Session file contains an item for a different thread than ${threadId}.`);
    }
    return items;
  }

  async readSummaries(threadId: string): Promise<ContextSummary[]> {
    const records = await this.readDecodedFile(this.summaryPath(threadId));
    const summaries = records.map((record) => record.payload as ContextSummary);
    if (summaries.some((summary) => summary.threadId !== threadId)) {
      throw new Error(`Summary file contains a record for a different thread than ${threadId}.`);
    }
    return summaries;
  }

  async readThread(threadId: string): Promise<ThreadSnapshot> {
    const [threads, turns, items, traces, summaries] = await Promise.all([
      this.listThreads(),
      this.readTurns(threadId),
      this.readThreadItems(threadId),
      this.readTraces(threadId),
      this.readSummaries(threadId)
    ]);
    return {
      thread: threads.find((thread) => thread.id === threadId) ?? null,
      turns,
      items,
      traces,
      summaries
    };
  }

  async listThreads(): Promise<ThreadRecord[]> {
    return (await this.readThreadIndex()).threads;
  }

  private async readTraces(threadId: string): Promise<TraceEvent[]> {
    const records = await this.readDecodedFile(await this.tracePath(threadId));
    const traces = records.map((record) => record.payload as TraceEvent);
    if (traces.some((trace) => trace.threadId !== threadId)) {
      throw new Error(`Trace file contains a record for a different thread than ${threadId}.`);
    }
    return traces;
  }

  private async appendVersionedRecord(
    filePath: string,
    kind: string,
    payload: unknown,
    createdAt: string
  ): Promise<void> {
    await this.serializeFileMutation(filePath, async () => {
      const current = await this.readDecodedFile(filePath);
      const lastSequence = current.reduce(
        (sequence, record) => Math.max(sequence, record.sequence ?? 0),
        0
      );
      await this.operations.appendJsonl(
        filePath,
        createStoredEnvelope(kind, payload, lastSequence + 1, createdAt)
      );
    });
  }

  private async serializeFileMutation<T>(
    filePath: string,
    mutation: () => Promise<T>
  ): Promise<T> {
    const key = resolve(filePath);
    const previous = this.fileMutationTails.get(key) ?? Promise.resolve();
    const operation = previous.catch(() => undefined).then(mutation);
    const tail = operation.then(
      () => undefined,
      () => undefined
    );
    this.fileMutationTails.set(key, tail);
    return operation.finally(() => {
      if (this.fileMutationTails.get(key) === tail) {
        this.fileMutationTails.delete(key);
      }
    });
  }

  private async readDecodedFile(filePath: string): Promise<DecodedRecord[]> {
    const values = await readJsonl<unknown>(filePath);
    return this.decodeRecordsForFile(filePath, values, false);
  }

  private decodeRecordsForFile(
    filePath: string,
    values: readonly unknown[],
    requireVersioned: boolean
  ): DecodedRecord[] {
    const area = relative(this.rootDir, filePath).split(/[\\/]/u)[0];
    const records: DecodedRecord[] = [];
    let lastSequence = 0;

    for (const value of values) {
      const envelope = inspectStoredEnvelope(value);
      if (requireVersioned && !envelope) {
        throw new Error(`Migrated replacement for ${filePath} contains a legacy record.`);
      }
      if (envelope) {
        if (envelope.sequence <= lastSequence) {
          throw new Error(`Non-monotonic storage sequence in ${filePath}.`);
        }
        lastSequence = envelope.sequence;
      }

      let kind: string;
      let payload: unknown;
      let createdAt: string;
      switch (area) {
        case "sessions": {
          kind = "thread.item";
          payload = unwrapStoredRecord(value, kind, isThreadItem);
          createdAt = envelope?.createdAt ?? (payload as ThreadItem).createdAt;
          break;
        }
        case "history": {
          kind = "history.input";
          payload = unwrapStoredRecord(value, kind, isInputHistoryRecord);
          createdAt = envelope?.createdAt ?? (payload as InputHistoryRecord).ts;
          break;
        }
        case "traces": {
          kind = "trace.event";
          payload = unwrapStoredRecord(value, kind, isTraceEvent);
          createdAt = envelope?.createdAt ?? (payload as TraceEvent).createdAt;
          break;
        }
        case "summaries": {
          kind = "context.summary";
          payload = unwrapStoredRecord(value, kind, isContextSummary);
          createdAt = envelope?.createdAt ?? (payload as ContextSummary).createdAt;
          break;
        }
        case "turns": {
          ({kind, payload, createdAt} = decodeTurnRecord(value, envelope));
          break;
        }
        default:
          throw new Error(`Unrecognized JSONL storage path ${filePath}.`);
      }
      records.push({kind, payload, createdAt, sequence: envelope?.sequence ?? null});
    }
    return records;
  }

  private async sessionPath(threadId: string): Promise<string> {
    const date = await this.threadDate(threadId);
    const currentPath = join(
      this.rootDir,
      "sessions",
      String(date.getFullYear()),
      String(date.getMonth() + 1).padStart(2, "0"),
      String(date.getDate()).padStart(2, "0"),
      `${threadId}.jsonl`
    );
    if (await pathExists(currentPath)) {
      return currentPath;
    }
    const legacyMonthPath = join(
      this.rootDir,
      "sessions",
      String(date.getFullYear()),
      String(date.getMonth() + 1).padStart(2, "0"),
      `${threadId}.jsonl`
    );
    return (await pathExists(legacyMonthPath)) ? legacyMonthPath : currentPath;
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

  private manifestPath(): string {
    return join(this.rootDir, "manifest.json");
  }

  private async ensureLayout(): Promise<void> {
    await mkdir(this.rootDir, {recursive: true});
    for (const directory of [...JSONL_DIRECTORIES, "indexes", "db"]) {
      await mkdir(join(this.rootDir, directory), {recursive: true});
    }
  }

  private async ensureThreadIndex(): Promise<void> {
    if (await pathExists(this.indexPath())) {
      await this.readThreadIndex();
      return;
    }
    await this.writeThreadIndex({threads: []});
  }

  private async readThreadIndex(): Promise<ThreadsIndex> {
    return readJsonFile<ThreadsIndex>(this.indexPath(), {threads: []}, {
      validate: isThreadsIndex
    });
  }

  private async writeThreadIndex(index: ThreadsIndex): Promise<void> {
    assertValid(index, isThreadsIndex, "thread index");
    await this.operations.writeJsonFile(this.indexPath(), index);
  }

  private async readManifest(): Promise<StorageManifest | null> {
    let raw: string;
    try {
      raw = await readFile(this.manifestPath(), "utf8");
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === "ENOENT") {
        return null;
      }
      throw new Error(`Unable to read storage manifest at ${this.manifestPath()}.`, {cause: error});
    }

    let value: unknown;
    try {
      value = JSON.parse(raw) as unknown;
    } catch (error) {
      throw new Error(`Storage manifest at ${this.manifestPath()} is invalid JSON.`, {cause: error});
    }
    if (isRecord(value) && value.schemaVersion !== 1) {
      throw new Error(`Unknown storage manifest schema version ${String(value.schemaVersion)}.`);
    }
    if (!isStorageManifest(value)) {
      throw new Error(`Storage manifest at ${this.manifestPath()} is structurally invalid.`);
    }
    return value;
  }

  private async writeManifest(): Promise<void> {
    await this.operations.writeJsonFile(
      this.manifestPath(),
      {
        schemaVersion: 1,
        storage: "jsonl",
        createdAt: new Date().toISOString()
      } satisfies StorageManifest,
      {backup: false}
    );
  }

  private async validateLegacyThreadIndex(): Promise<void> {
    if (!(await pathExists(this.indexPath()))) {
      return;
    }
    const candidates = [this.indexPath(), `${this.indexPath()}.bak`];
    for (const candidate of candidates) {
      try {
        const parsed = JSON.parse(await readFile(candidate, "utf8")) as unknown;
        if (isThreadsIndex(parsed)) {
          return;
        }
      } catch (error) {
        if ((error as NodeJS.ErrnoException).code === "ENOENT") {
          continue;
        }
      }
    }
    throw new Error(`Legacy thread index at ${this.indexPath()} is structurally invalid.`);
  }

  private async findLegacyJsonlFiles(): Promise<string[]> {
    const files: string[] = [];
    for (const directory of JSONL_DIRECTORIES) {
      files.push(...(await findJsonlFiles(join(this.rootDir, directory))));
    }
    return files.sort((left, right) => left.localeCompare(right));
  }

  private async recoverInterruptedMigration(): Promise<void> {
    const backups: Array<{backupPath: string; filePath: string; raw: string}> = [];
    for (const directory of JSONL_DIRECTORIES) {
      for (const backupPath of await findMigrationBackupFiles(join(this.rootDir, directory))) {
        const filePath = backupPath.slice(0, -".v0.bak".length);
        const raw = await readFile(backupPath, "utf8");
        this.decodeRecordsForFile(filePath, parseJsonlForMigration(raw, backupPath), false);
        backups.push({backupPath, filePath, raw});
      }
    }

    for (const backup of backups) {
      await this.operations.writeTextFileAtomic(backup.filePath, backup.raw);
      const restored = await readFile(backup.filePath, "utf8");
      if (restored !== backup.raw) {
        throw new Error(`Interrupted migration recovery changed ${backup.filePath}.`);
      }
      this.decodeRecordsForFile(
        backup.filePath,
        parseJsonlForMigration(restored, backup.filePath),
        false
      );
    }
    for (const backup of backups) {
      await this.operations.rm(backup.backupPath, {force: true});
    }
  }

  private async validateCurrentJsonlFiles(): Promise<void> {
    for (const filePath of await this.findLegacyJsonlFiles()) {
      const values = await readJsonl<unknown>(filePath);
      this.decodeRecordsForFile(filePath, values, false);
    }
  }

  private async migrateLegacyFiles(filePaths: readonly string[]): Promise<void> {
    const prepared: PreparedMigration[] = [];
    try {
      for (const filePath of filePaths) {
        const original = await readFile(filePath, "utf8");
        const values = parseJsonlForMigration(original, filePath);
        const decoded = this.decodeRecordsForFile(filePath, values, false);
        const envelopes = decoded.map((record, index) =>
          createStoredEnvelope(record.kind, record.payload, index + 1, record.createdAt)
        );
        const tempPath = `${filePath}.v1.${randomUUID()}.tmp`;
        const migration: PreparedMigration = {
          filePath,
          tempPath,
          backupPath: `${filePath}.v0.bak`,
          legacyRaw: original,
          originalMoved: false
        };
        prepared.push(migration);
        await this.operations.writeJsonlFile(tempPath, envelopes);
        const staged = await readJsonl<unknown>(tempPath);
        const checked = this.decodeRecordsForFile(filePath, staged, true);
        if (checked.length !== values.length) {
          throw new Error(`Migrated replacement validation changed record count for ${filePath}.`);
        }
      }

      for (const migration of prepared) {
        await this.operations.rename(migration.filePath, migration.backupPath);
        migration.originalMoved = true;
        await this.operations.rename(migration.tempPath, migration.filePath);
      }
      await this.ensureLayout();
      await this.ensureThreadIndex();
      await this.writeManifest();
    } catch (error) {
      const rollbackErrors: unknown[] = [];
      for (const migration of [...prepared].reverse()) {
        try {
          if (migration.originalMoved) {
            const backupRaw = await readFile(migration.backupPath, "utf8");
            if (backupRaw !== migration.legacyRaw) {
              throw new Error(`Migration backup changed before rollback for ${migration.filePath}.`);
            }
            this.decodeRecordsForFile(
              migration.filePath,
              parseJsonlForMigration(backupRaw, migration.backupPath),
              false
            );
            await this.operations.writeTextFileAtomic(migration.filePath, backupRaw);
            const restored = await readFile(migration.filePath, "utf8");
            if (restored !== backupRaw) {
              throw new Error(`Migration rollback changed ${migration.filePath}.`);
            }
            this.decodeRecordsForFile(
              migration.filePath,
              parseJsonlForMigration(restored, migration.filePath),
              false
            );
          }
        } catch (rollbackError) {
          rollbackErrors.push(rollbackError);
        }
      }
      await Promise.all(
        prepared.map((migration) =>
          this.operations.rm(migration.tempPath, {force: true}).catch(() => undefined)
        )
      );
      if (rollbackErrors.length > 0) {
        throw new AggregateError([error, ...rollbackErrors], "Storage migration and rollback failed.");
      }
      throw error;
    }
  }
}

export {JsonlStorageProvider as JsonlSessionRepository};

function decodeTurnRecord(
  value: unknown,
  envelope: StoredEnvelope<unknown> | null
): Pick<DecodedRecord, "kind" | "payload" | "createdAt"> {
  if (envelope?.kind === "turn.snapshot") {
    const turn = unwrapStoredRecord(value, "turn.snapshot", isTurnRecord);
    return {
      kind: "turn.snapshot",
      payload: turn,
      createdAt: envelope.createdAt
    };
  }
  if (envelope?.kind === "assistant.delta") {
    const batch = unwrapStoredRecord(value, "assistant.delta", isTurnDeltaBatch);
    return {
      kind: "assistant.delta",
      payload: batch,
      createdAt: envelope.createdAt
    };
  }
  if (envelope) {
    throw new Error(`Unexpected stored Turn record kind ${envelope.kind}.`);
  }
  if (!isRecord(value)) {
    throw new Error("Structurally invalid legacy Turn record.");
  }
  if (value.kind === "turn.snapshot" && isTurnRecord(value.turn)) {
    return {
      kind: "turn.snapshot",
      payload: value.turn,
      createdAt: value.turn.completedAt ?? value.turn.startedAt
    };
  }
  if (value.kind === "assistant.delta" && isLegacyTurnDelta(value)) {
    const batch: TurnDeltaBatch = {
      threadId: value.threadId,
      turnId: value.turnId,
      delta: value.delta,
      createdAt: value.createdAt
    };
    return {kind: "assistant.delta", payload: batch, createdAt: batch.createdAt};
  }
  throw new Error("Structurally invalid legacy Turn record.");
}

function parseJsonlForMigration(raw: string, filePath: string): unknown[] {
  if (!raw) {
    return [];
  }
  const lines = raw.split("\n");
  const nonEmpty = lines
    .map((line, index) => ({line: line.trim(), index}))
    .filter(({line}) => line.length > 0);
  const lastIndex = nonEmpty.at(-1)?.index ?? -1;
  const values: unknown[] = [];
  for (const {line, index} of nonEmpty) {
    try {
      values.push(JSON.parse(line) as unknown);
    } catch (error) {
      const tail = index === lastIndex && !raw.endsWith("\n") ? " final record" : " record";
      throw new Error(
        `JSONL corruption in ${filePath} at line ${index + 1}:${tail} is incomplete. ` +
          "Repair or restore the legacy file before retrying migration.",
        {cause: error}
      );
    }
  }
  return values;
}

async function findJsonlFiles(directory: string): Promise<string[]> {
  let entries;
  try {
    entries = await readdir(directory, {withFileTypes: true});
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") {
      return [];
    }
    throw error;
  }
  const files: string[] = [];
  for (const entry of entries) {
    const entryPath = join(directory, entry.name);
    if (entry.isDirectory()) {
      files.push(...(await findJsonlFiles(entryPath)));
    } else if (entry.isFile() && entry.name.endsWith(".jsonl")) {
      files.push(entryPath);
    }
  }
  return files;
}

async function findMigrationBackupFiles(directory: string): Promise<string[]> {
  let entries;
  try {
    entries = await readdir(directory, {withFileTypes: true});
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") {
      return [];
    }
    throw error;
  }
  const files: string[] = [];
  for (const entry of entries) {
    const entryPath = join(directory, entry.name);
    if (entry.isDirectory()) {
      files.push(...(await findMigrationBackupFiles(entryPath)));
    } else if (entry.isFile() && entry.name.endsWith(".jsonl.v0.bak")) {
      files.push(entryPath);
    }
  }
  return files;
}

async function pathExists(filePath: string): Promise<boolean> {
  try {
    await stat(filePath);
    return true;
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") {
      return false;
    }
    throw error;
  }
}

function assertValid<T>(value: unknown, validate: Validator<T>, label: string): asserts value is T {
  if (!validate(value)) {
    throw new Error(`Structurally invalid ${label}.`);
  }
}

function isStorageManifest(value: unknown): value is StorageManifest {
  return (
    isRecord(value) &&
    value.schemaVersion === 1 &&
    value.storage === "jsonl" &&
    typeof value.createdAt === "string" &&
    value.createdAt.length > 0
  );
}

function isThreadsIndex(value: unknown): value is ThreadsIndex {
  if (!isRecord(value) || !Array.isArray(value.threads) || !value.threads.every(isThreadRecord)) {
    return false;
  }
  return value.threads.filter((thread) => thread.status === "active").length <= 1;
}

function isThreadRecord(value: unknown): value is ThreadRecord {
  if (!isRecord(value)) {
    return false;
  }
  return (
    isNonEmptyString(value.id) &&
    typeof value.title === "string" &&
    isNonEmptyString(value.createdAt) &&
    isNonEmptyString(value.updatedAt) &&
    isNonEmptyString(value.model) &&
    typeof value.cwd === "string" &&
    (value.status === "active" || value.status === "archived")
  );
}

function isTurnRecord(value: unknown): value is TurnRecord {
  if (!isRecord(value)) {
    return false;
  }
  return (
    isNonEmptyString(value.id) &&
    isNonEmptyString(value.threadId) &&
    typeof value.userInput === "string" &&
    (value.status === "running" ||
      value.status === "completed" ||
      value.status === "failed" ||
      value.status === "interrupted") &&
    isNonEmptyString(value.startedAt) &&
    optionalString(value.completedAt) &&
    optionalString(value.assistantDraft) &&
    (value.modelProvenance === undefined ||
      isTurnModelProvenance(value.modelProvenance))
  );
}

function isTurnModelProvenance(value: unknown): boolean {
  return (
    isRecord(value) &&
    value.schemaVersion === 1 &&
    isNonEmptyString(value.adapterId) &&
    isNonEmptyString(value.providerProfileId) &&
    isNonEmptyString(value.modelProfileId) &&
    isNonEmptyString(value.model) &&
    (value.protocol === "responses" || value.protocol === "chat_completions")
  );
}

function isTurnDeltaBatch(value: unknown): value is TurnDeltaBatch {
  return (
    isRecord(value) &&
    isNonEmptyString(value.threadId) &&
    isNonEmptyString(value.turnId) &&
    typeof value.delta === "string" &&
    isNonEmptyString(value.createdAt)
  );
}

function isLegacyTurnDelta(
  value: Record<string, unknown>
): value is Record<string, unknown> & TurnDeltaBatch {
  return isTurnDeltaBatch(value);
}

function isThreadItem(value: unknown): value is ThreadItem {
  if (!isRecord(value)) {
    return false;
  }
  const itemTypes = new Set([
    "user_message",
    "assistant_message",
    "trace_event",
    "error",
    "agent_item"
  ]);
  return (
    isNonEmptyString(value.id) &&
    isNonEmptyString(value.threadId) &&
    optionalString(value.turnId) &&
    itemTypes.has(String(value.type)) &&
    (value.role === undefined ||
      value.role === "system" ||
      value.role === "user" ||
      value.role === "assistant") &&
    typeof value.content === "string" &&
    isNonEmptyString(value.createdAt) &&
    (value.metadata === undefined || isRecord(value.metadata)) &&
    (value.agent === undefined || isAgentItemEnvelope(value.agent)) &&
    (value.type === "agent_item"
      ? isAgentItemEnvelope(value.agent)
      : value.agent === undefined)
  );
}

function isTraceEvent(value: unknown): value is TraceEvent {
  if (!isRecord(value)) {
    return false;
  }
  const categories = new Set(["lifecycle", "provider", "context", "error"]);
  const codes = new Set([
    "turn.start",
    "turn.recovered",
    "turn.interrupted",
    "compact.completed",
    "compact.limit",
    "provider.request.started",
    "provider.stream.started",
    "provider.reasoning.filtered",
    "provider.request.failed"
  ]);
  return (
    isNonEmptyString(value.id) &&
    isNonEmptyString(value.threadId) &&
    optionalString(value.turnId) &&
    categories.has(String(value.category)) &&
    codes.has(String(value.code)) &&
    typeof value.message === "string" &&
    isNonEmptyString(value.createdAt) &&
    (value.facts === undefined || isTraceFacts(value.facts))
  );
}

function isTraceFacts(value: unknown): boolean {
  return (
    isRecord(value) &&
    Object.values(value).every(
      (fact) =>
        fact === null ||
        typeof fact === "string" ||
        typeof fact === "number" ||
        typeof fact === "boolean"
    )
  );
}

function isContextSummary(value: unknown): value is ContextSummary {
  return (
    isRecord(value) &&
    isNonEmptyString(value.id) &&
    isNonEmptyString(value.threadId) &&
    isNonEmptyString(value.createdAt) &&
    typeof value.content === "string" &&
    typeof value.tokenEstimate === "number" &&
    Number.isFinite(value.tokenEstimate) &&
    value.tokenEstimate >= 0 &&
    optionalString(value.coveredThroughItemId)
  );
}

function isInputHistoryRecord(value: unknown): value is InputHistoryRecord {
  return (
    isRecord(value) &&
    isNonEmptyString(value.threadId) &&
    optionalString(value.turnId) &&
    typeof value.text === "string" &&
    isNonEmptyString(value.ts)
  );
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isNonEmptyString(value: unknown): value is string {
  return typeof value === "string" && value.length > 0;
}

function optionalString(value: unknown): boolean {
  return value === undefined || typeof value === "string";
}
