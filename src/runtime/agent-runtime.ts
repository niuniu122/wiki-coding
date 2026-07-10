import {join} from "node:path";
import {ConfigManager} from "../config/config-manager.js";
import {getActiveProvider, listProviders} from "../config/provider-config.js";
import {SecretStore, type SecretLocation} from "../config/secret-store.js";
import {ProviderModelAdapter} from "../providers/provider-model-adapter.js";
import {ProviderError} from "../providers/provider-error.js";
import type {RuntimeEvent} from "../protocol.js";
import {JsonlStorageProvider} from "../storage/jsonl-storage.js";
import {SqliteStorageProvider} from "../storage/sqlite-storage.js";
import type {StorageProvider} from "../storage/storage-provider.js";
import type {AppConfig, ThreadItem, ThreadRecord, TraceCode, TurnRecord} from "../types.js";
import {createId} from "../utils/id.js";
import {ContextManager, estimateTokens} from "./context-manager.js";
import type {ModelAdapter} from "./model-adapter.js";
import {LocalSummaryGenerator, type CompactReason, type SummaryGenerator} from "./summary-generator.js";
import {SafeTraceRecorder} from "./trace-recorder.js";

interface CompactOptions {
  preserveTurnId?: string;
  userInput?: string;
}

interface ActiveRequest {
  turnId: string;
  controller: AbortController;
}

export class AgentRuntime {
  private config!: AppConfig;
  private storage!: StorageProvider;
  private currentThread!: ThreadRecord;
  private readonly contextManager = new ContextManager();
  private readonly traceRecorder = new SafeTraceRecorder();
  private activeRequest: ActiveRequest | null = null;

  constructor(
    private readonly cwd = process.cwd(),
    private readonly stateRoot = join(cwd, ".mini-codex"),
    private readonly configManager = new ConfigManager(stateRoot),
    private readonly secretStore = new SecretStore(stateRoot),
    private readonly modelAdapter: ModelAdapter = new ProviderModelAdapter(),
    private readonly summaryGenerator: SummaryGenerator = new LocalSummaryGenerator()
  ) {}

  async init(): Promise<RuntimeEvent[]> {
    this.config = await this.configManager.load();
    this.storage =
      this.config.storage.driver === "sqlite"
        ? new SqliteStorageProvider()
        : new JsonlStorageProvider(this.stateRoot);
    await this.storage.init();
    await this.ensureThread();
    const recoveryEvents = await this.recoverInterruptedTurns();
    const items = await this.storage.readThreadItems(this.currentThread.id);
    return [
      {type: "thread.loaded", thread: this.currentThread},
      {type: "history.loaded", items},
      ...recoveryEvents
    ];
  }

  async hasApiKey(): Promise<boolean> {
    const provider = getActiveProvider(this.config);
    return Boolean(await this.secretStore.getApiKey(provider.id, provider.envKey));
  }

  async setApiKey(apiKey: string): Promise<SecretLocation> {
    const provider = getActiveProvider(this.config);
    return this.secretStore.setApiKey(apiKey, provider.id);
  }

  getProviderSummary(): string {
    const provider = getActiveProvider(this.config);
    return `${provider.id} | ${provider.protocol} | ${provider.baseUrl} | model=${this.config.model}`;
  }

  listProviderSummaries(): string[] {
    return listProviders(this.config).map((provider) => {
      const active = provider.id === this.config.modelProvider ? "active" : "available";
      return `${provider.id} (${active}) - ${provider.name} - ${provider.protocol} - ${provider.baseUrl}`;
    });
  }

  async listThreads(): Promise<ThreadRecord[]> {
    return this.storage.listThreads();
  }

  async newThread(): Promise<RuntimeEvent[]> {
    if (this.activeRequest) {
      throw new Error("当前模型请求仍在进行，请先使用 /interrupt 取消后再新建会话。");
    }
    const now = new Date().toISOString();
    const thread: ThreadRecord = {
      id: createId("thread"),
      title: "MiniMax Codex Session",
      createdAt: now,
      updatedAt: now,
      model: this.config.model,
      cwd: this.cwd,
      status: "active"
    };
    await this.storage.createThread(thread);
    this.currentThread = thread;
    return [
      {type: "thread.loaded", thread},
      {type: "history.loaded", items: []}
    ];
  }

  async resumeThread(threadId: string): Promise<RuntimeEvent[]> {
    if (this.activeRequest) {
      throw new Error("当前模型请求仍在进行，请先使用 /interrupt 取消后再切换会话。");
    }
    const activated = await this.storage.activateThread(threadId, new Date().toISOString());
    if (!activated) {
      throw new Error(`历史会话不存在：${threadId}`);
    }

    this.currentThread = activated;
    const recoveryEvents = await this.recoverInterruptedTurns();
    const items = await this.storage.readThreadItems(this.currentThread.id);
    return [
      {type: "thread.loaded", thread: this.currentThread},
      {type: "history.loaded", items},
      ...recoveryEvents
    ];
  }

  async switchProvider(providerId: string): Promise<string> {
    const provider = this.config.modelProviders[providerId];
    if (!provider) {
      throw new Error(`未知 provider：${providerId}`);
    }

    this.config = {
      ...this.config,
      modelProvider: providerId,
      model: provider.defaultModel ?? this.config.model,
      api: {
        provider:
          providerId === "hashsight"
            ? "hashsight"
            : providerId === "minimax-official"
              ? "minimax"
              : "openai-compatible",
        protocol: provider.protocol,
        baseUrl: provider.baseUrl
      }
    };
    await this.configManager.save(this.config);
    return this.getProviderSummary();
  }

  interruptCurrentTurn(): RuntimeEvent {
    if (!this.activeRequest) {
      return {type: "turn.interrupt.ignored", reason: "no_active_request"};
    }
    this.activeRequest.controller.abort();
    return {type: "turn.interrupt.requested", turnId: this.activeRequest.turnId};
  }

  async compact(reason: CompactReason, options: CompactOptions = {}): Promise<RuntimeEvent[]> {
    const items = await this.storage.readThreadItems(this.currentThread.id);
    const summaries = await this.storage.readSummaries(this.currentThread.id);
    const beforeContext = this.contextManager.buildContext({
      config: this.config,
      items,
      summaries,
      ...(options.userInput !== undefined ? {userInput: options.userInput} : {})
    });
    const boundaryIndex = this.contextManager.findCompactionBoundary(
      items,
      options.preserveTurnId
    );

    if (boundaryIndex < 0) {
      return [
        {type: "compact.started", reason},
        {
          type: "compact.completed",
          summary: "",
          compacted: false,
          beforeTokens: beforeContext.tokenEstimate,
          afterTokens: beforeContext.tokenEstimate
        }
      ];
    }

    const boundary = items[boundaryIndex];
    if (!boundary) {
      throw new Error("Compaction boundary disappeared while preparing the summary.");
    }

    const coveredItems = items.slice(0, boundaryIndex + 1);
    const content = await this.summaryGenerator.generate(coveredItems, reason);
    const summary = this.contextManager.createSummaryRecord(
      this.currentThread.id,
      content,
      reason,
      boundary.id
    );
    await this.storage.appendSummary(summary);
    const afterContext = this.contextManager.buildContext({
      config: this.config,
      items,
      summaries: [...summaries, summary],
      ...(options.userInput !== undefined ? {userInput: options.userInput} : {})
    });
    await this.appendTrace("compact.completed", {
      reason,
      summaryId: summary.id,
      coveredThroughItemId: boundary.id,
      beforeTokens: beforeContext.tokenEstimate,
      afterTokens: afterContext.tokenEstimate
    });
    return [
      {type: "compact.started", reason},
      {
        type: "compact.completed",
        summary: summary.content,
        compacted: true,
        coveredThroughItemId: boundary.id,
        beforeTokens: beforeContext.tokenEstimate,
        afterTokens: afterContext.tokenEstimate
      }
    ];
  }

  async *submitUserInput(input: string): AsyncGenerator<RuntimeEvent> {
    const provider = getActiveProvider(this.config);
    const apiKey = await this.secretStore.getApiKey(provider.id, provider.envKey);
    if (!apiKey) {
      yield {type: "error", message: `还没有 ${provider.name} API key，请先输入 /api 设置。`};
      return;
    }

    const turn = await this.createTurn(input);
    yield {type: "turn.started", turnId: turn.id, input};
    yield await this.appendTrace("turn.start", {turnId: turn.id});

    const userItem = this.createItem({
      turnId: turn.id,
      type: "user_message",
      role: "user",
      content: input
    });
    await this.storage.appendItem(userItem);

    const items = await this.storage.readThreadItems(this.currentThread.id);
    const summaries = await this.storage.readSummaries(this.currentThread.id);
    let builtContext = this.contextManager.buildContext({
      config: this.config,
      items,
      summaries,
      userInput: input
    });
    yield {
      type: "token.usage",
      used: builtContext.tokenEstimate,
      limit: builtContext.inputLimit,
      autoCompactAt: builtContext.autoCompactAt
    };

    if (builtContext.shouldCompact) {
      const compactEvents = await this.compact("auto", {
        preserveTurnId: turn.id,
        userInput: input
      });
      let didCompact = false;
      for (const event of compactEvents) {
        if (event.type === "compact.completed" && event.compacted) {
          didCompact = true;
        }
        yield event;
      }
      if (didCompact) {
        const refreshedSummaries = await this.storage.readSummaries(this.currentThread.id);
        builtContext = this.contextManager.buildContext({
          config: this.config,
          items,
          summaries: refreshedSummaries,
          userInput: input
        });
        yield {
          type: "token.usage",
          used: builtContext.tokenEstimate,
          limit: builtContext.inputLimit,
          autoCompactAt: builtContext.autoCompactAt
        };
      }
    }

    if (builtContext.shouldCompact) {
      const message =
        `当前输入在压缩后仍超过上下文安全上限` +
        `（估算 ${builtContext.tokenEstimate} token，安全线 ${builtContext.autoCompactAt} token）。` +
        `请缩短本次输入，或提高 workingContextLimit。`;
      const errorItem = this.createItem({
        turnId: turn.id,
        type: "error",
        content: message
      });
      await this.storage.appendItem(errorItem);
      await this.completeTurn(turn, "failed");
      yield await this.appendTrace("compact.limit", {
        turnId: turn.id,
        tokenEstimate: builtContext.tokenEstimate,
        autoCompactAt: builtContext.autoCompactAt,
        inputLimit: builtContext.inputLimit
      });
      yield {type: "error", message, turnId: turn.id};
      return;
    }

    let assistantContent = "";
    const activeRequest: ActiveRequest = {
      turnId: turn.id,
      controller: new AbortController()
    };
    this.activeRequest = activeRequest;
    yield {type: "api.status", status: "requesting"};

    try {
      for await (const event of this.modelAdapter.streamResponse({
        config: this.config,
        apiKey,
        messages: builtContext.messages,
        signal: activeRequest.controller.signal
      })) {
        if (event.type === "delta") {
          assistantContent += event.delta;
          await this.storage.appendTurnDelta(
            this.currentThread.id,
            turn.id,
            event.delta,
            new Date().toISOString()
          );
          yield {type: "assistant.delta", turnId: turn.id, delta: event.delta};
        } else if (event.type === "usage") {
          yield {
            type: "token.usage",
            used: event.totalTokens ?? estimateTokens(assistantContent),
            limit: builtContext.inputLimit,
            autoCompactAt: builtContext.autoCompactAt
          };
        } else if (event.type === "diagnostic") {
          yield await this.appendTrace(event.code, {...event.facts, turnId: turn.id});
        }
      }

      if (activeRequest.controller.signal.aborted) {
        throw new Error("The active model request was interrupted.");
      }

      const assistantItem = this.createItem({
        turnId: turn.id,
        type: "assistant_message",
        role: "assistant",
        content: assistantContent || "MiniMax 没有返回文本内容。"
      });
      await this.storage.appendItem(assistantItem);
      await this.completeTurn(turn, "completed");
      yield {type: "assistant.completed", item: assistantItem};
      yield {type: "api.status", status: "completed"};
    } catch (error) {
      if (activeRequest.controller.signal.aborted) {
        if (assistantContent) {
          const partialItem = this.createItem({
            turnId: turn.id,
            type: "assistant_message",
            role: "assistant",
            content: assistantContent,
            metadata: {partial: true, interrupted: true}
          });
          await this.storage.appendItem(partialItem);
        }
        await this.completeTurn(turn, "interrupted");
        yield await this.appendTrace("turn.interrupted", {
          turnId: turn.id,
          hadAssistantDraft: Boolean(assistantContent)
        });
        yield {type: "api.status", status: "idle"};
        yield {type: "turn.interrupted", turnId: turn.id};
      } else {
        const message = error instanceof Error ? error.message : String(error);
        if (assistantContent) {
          const partialItem = this.createItem({
            turnId: turn.id,
            type: "assistant_message",
            role: "assistant",
            content: assistantContent,
            metadata: {partial: true, failed: true}
          });
          await this.storage.appendItem(partialItem);
        }
        const errorItem = this.createItem({
          turnId: turn.id,
          type: "error",
          content: message
        });
        await this.storage.appendItem(errorItem);
        await this.completeTurn(turn, "failed");
        const providerError = error instanceof ProviderError ? error : null;
        yield await this.appendTrace("provider.request.failed", {
          turnId: turn.id,
          providerId: providerError?.providerId ?? this.config.modelProvider,
          kind: providerError?.kind ?? "unknown",
          status: providerError?.status,
          retryable: providerError?.retryable ?? false,
          requestId: providerError?.requestId
        });
        yield {type: "error", message, turnId: turn.id};
      }
    } finally {
      if (this.activeRequest === activeRequest) {
        this.activeRequest = null;
      }
    }
  }

  private async ensureThread(): Promise<void> {
    const threads = await this.storage.listThreads();
    const existing = threads.find((thread) => thread.status === "active");
    if (existing) {
      this.currentThread = existing;
      return;
    }

    const now = new Date().toISOString();
    this.currentThread = {
      id: createId("thread"),
      title: "MiniMax Codex Session",
      createdAt: now,
      updatedAt: now,
      model: this.config.model,
      cwd: this.cwd,
      status: "active"
    };
    await this.storage.createThread(this.currentThread);
  }

  private async recoverInterruptedTurns(): Promise<RuntimeEvent[]> {
    const turns = await this.storage.readTurns(this.currentThread.id);
    const runningTurns = turns.filter((turn) => turn.status === "running");
    if (runningTurns.length === 0) {
      return [];
    }

    const existingItems = await this.storage.readThreadItems(this.currentThread.id);
    const events: RuntimeEvent[] = [];

    for (const turn of runningTurns) {
      const hasRecoveredDraft = existingItems.some(
        (item) => item.metadata?.recoveredTurnId === turn.id
      );
      if (turn.assistantDraft && !hasRecoveredDraft) {
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
        await this.storage.appendItem(partialItem);
        existingItems.push(partialItem);
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

    return events;
  }

  private async createTurn(input: string): Promise<TurnRecord> {
    const now = new Date().toISOString();
    this.currentThread = {
      ...this.currentThread,
      title: this.currentThread.title === "MiniMax Codex Session" ? input.slice(0, 40) : this.currentThread.title,
      updatedAt: now
    };
    await this.storage.updateThread(this.currentThread);
    const turn: TurnRecord = {
      id: createId("turn"),
      threadId: this.currentThread.id,
      userInput: input,
      status: "running",
      startedAt: now
    };
    await this.storage.appendTurn(turn);
    return turn;
  }

  private async completeTurn(turn: TurnRecord, status: TurnRecord["status"]): Promise<void> {
    turn.status = status;
    turn.completedAt = new Date().toISOString();
    await this.storage.appendTurn(turn);
    this.currentThread = {...this.currentThread, updatedAt: turn.completedAt};
    await this.storage.updateThread(this.currentThread);
  }

  private createItem(params: {
    turnId: string;
    type: ThreadItem["type"];
    content: string;
    role?: ThreadItem["role"];
    metadata?: Record<string, unknown>;
  }): ThreadItem {
    const item: ThreadItem = {
      id: createId("item"),
      threadId: this.currentThread.id,
      turnId: params.turnId,
      type: params.type,
      content: params.content,
      createdAt: new Date().toISOString()
    };
    if (params.role) {
      item.role = params.role;
    }
    if (params.metadata) {
      item.metadata = params.metadata;
    }
    return item;
  }

  private async appendTrace(
    code: TraceCode,
    facts: Record<string, unknown> = {}
  ): Promise<RuntimeEvent> {
    const event = this.traceRecorder.create(this.currentThread.id, code, facts);
    await this.storage.appendTrace(event);
    return {type: "trace.event", event};
  }
}
