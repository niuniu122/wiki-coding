import {ProviderError} from "../providers/provider-error.js";
import type {RuntimeEvent} from "../protocol.js";
import type {SessionRepository} from "../storage/session-repository.js";
import {
  TurnDeltaBuffer,
  type TurnDeltaBufferOptions
} from "../storage/turn-delta-buffer.js";
import type {
  ThreadItem,
  TraceCode,
  TurnModelProvenance,
  TurnRecord,
  TurnStatus
} from "../types.js";
import {ContextEngine, type BuiltContext} from "./context-engine.js";
import type {ModelAdapterEvent} from "./model-adapter.js";
import type {
  ActiveModelSelection,
  ModelRuntimeSnapshotPort
} from "./model-selection-service.js";
import {SessionService} from "./session-service.js";
import {
  LocalSummaryGenerator,
  type CompactReason,
  type SummaryGenerator
} from "./summary-generator.js";
import {SafeTraceRecorder} from "./trace-recorder.js";

export interface CompactOptions {
  preserveTurnId?: string;
  userInput?: string;
}

export interface TurnEngineDependencies {
  sessionService: SessionService;
  modelRuntime: ModelRuntimeSnapshotPort;
  repository: SessionRepository;
  contextEngine?: ContextEngine;
  summaryGenerator?: SummaryGenerator;
  traceRecorder?: SafeTraceRecorder;
  deltaBufferOptions?: TurnDeltaBufferOptions;
}

interface ActiveTurn {
  turn: TurnRecord;
  controller: AbortController;
  buffer: TurnDeltaBuffer;
  assistantContent: string;
  terminalPersisted: boolean;
  finalization: Promise<void> | null;
  consumerPulling: boolean;
  emitInterruptionEvents: boolean;
}

class PrematureProviderCompletionError extends Error {
  constructor(message = "Provider stream ended before a terminal completion event.") {
    super(message);
    this.name = "PrematureProviderCompletionError";
  }
}

export class TurnEngine {
  private readonly sessionService: SessionService;
  private readonly modelRuntime: ModelRuntimeSnapshotPort;
  private readonly repository: SessionRepository;
  private readonly contextEngine: ContextEngine;
  private readonly summaryGenerator: SummaryGenerator;
  private readonly traceRecorder: SafeTraceRecorder;
  private readonly deltaBufferOptions: TurnDeltaBufferOptions;
  private activeTurn: ActiveTurn | null = null;
  private turnStarting = false;

  constructor(dependencies: TurnEngineDependencies) {
    this.sessionService = dependencies.sessionService;
    this.modelRuntime = dependencies.modelRuntime;
    this.repository = dependencies.repository;
    this.contextEngine = dependencies.contextEngine ?? new ContextEngine();
    this.summaryGenerator =
      dependencies.summaryGenerator ?? new LocalSummaryGenerator();
    this.traceRecorder = dependencies.traceRecorder ?? new SafeTraceRecorder();
    this.deltaBufferOptions = dependencies.deltaBufferOptions ?? {};
  }

  get hasActiveTurn(): boolean {
    return this.turnStarting || this.activeTurn !== null;
  }

  async *submit(input: string): AsyncGenerator<RuntimeEvent> {
    if (this.hasActiveTurn) {
      throw new Error("Another Turn is already active.");
    }

    this.turnStarting = true;
    let runtimeSnapshot;
    let selection: ActiveModelSelection;
    let turn: TurnRecord;
    try {
      runtimeSnapshot = this.modelRuntime.getRuntimeSnapshot();
      selection = runtimeSnapshot.selection;
      turn = await this.sessionService.createTurn(
        input,
        modelProvenance(selection)
      );
    } finally {
      this.turnStarting = false;
    }
    const active = this.createActiveTurn(turn);
    this.activeTurn = active;

    try {
      active.consumerPulling = false;
      yield {type: "turn.started", turnId: turn.id, input};
      active.consumerPulling = true;
      if (await this.stopAfterExternalFinalization(active)) {
        return;
      }
      yield await this.appendTrace("turn.start", {turnId: turn.id});
      if (await this.stopAfterExternalFinalization(active)) {
        return;
      }

      const userItem = this.sessionService.createItem({
        turnId: turn.id,
        type: "user_message",
        role: "user",
        content: input
      });
      await this.repository.appendItem(userItem);

      let builtContext = await this.buildContext(turn.threadId, input, selection);
      yield tokenUsageEvent(builtContext);
      if (await this.stopAfterExternalFinalization(active)) {
        return;
      }

      if (builtContext.shouldCompact) {
        const compactEvents = await this.compact("auto", {
          preserveTurnId: turn.id,
          userInput: input
        });
        let compacted = false;
        for (const event of compactEvents) {
          compacted ||= event.type === "compact.completed" && event.compacted;
          yield event;
          if (await this.stopAfterExternalFinalization(active)) {
            return;
          }
        }
        if (compacted) {
          builtContext = await this.buildContext(turn.threadId, input, selection);
          yield tokenUsageEvent(builtContext);
          if (await this.stopAfterExternalFinalization(active)) {
            return;
          }
        }
      }

      if (builtContext.shouldCompact) {
        const message =
          `当前输入在压缩后仍超过上下文安全上限` +
          `（估算 ${builtContext.tokenEstimate} token，安全线 ${builtContext.autoCompactAt} token）。` +
          `请缩短本次输入，或提高 workingContextLimit。`;
        const errorItem = this.sessionService.createItem({
          turnId: turn.id,
          type: "error",
          content: message
        });
        await this.persistTerminal(active, "failed", active.assistantContent, [errorItem]);
        yield await this.appendTrace("compact.limit", {
          turnId: turn.id,
          tokenEstimate: builtContext.tokenEstimate,
          autoCompactAt: builtContext.autoCompactAt,
          inputLimit: builtContext.inputLimit
        });
        yield {type: "error", message, turnId: turn.id};
        return;
      }

      yield {type: "api.status", status: "requesting"};
      if (await this.stopAfterExternalFinalization(active)) {
        return;
      }
      let providerCompleted = false;
      for await (const event of runtimeSnapshot.runtime.stream({
        messages: builtContext.messages,
        maxOutputTokens: selection.maxOutputTokens,
        signal: active.controller.signal
      })) {
        if (providerCompleted) {
          throw new PrematureProviderCompletionError(
            "Provider emitted an event after terminal completion."
          );
        }
        if (event.type === "completed") {
          providerCompleted = true;
          continue;
        }
        yield* this.consumeProviderEvent(event, turn, active, builtContext, (delta) => {
          active.assistantContent += delta;
        });
        if (await this.stopAfterExternalFinalization(active)) {
          return;
        }
      }

      if (!providerCompleted) {
        throw new PrematureProviderCompletionError();
      }
      if (active.controller.signal.aborted) {
        throw abortError();
      }

      const assistantItem = this.sessionService.createItem({
        turnId: turn.id,
        type: "assistant_message",
        role: "assistant",
        content: active.assistantContent || "MiniMax 没有返回文本内容。"
      });
      await this.persistTerminal(
        active,
        "completed",
        active.assistantContent,
        [assistantItem]
      );
      yield {type: "assistant.completed", item: assistantItem};
      yield {type: "api.status", status: "completed"};
    } catch (error) {
      if (active.finalization) {
        await active.finalization;
        if (active.emitInterruptionEvents) {
          active.emitInterruptionEvents = false;
          yield await this.appendTrace("turn.interrupted", {
            turnId: turn.id,
            hadAssistantDraft: Boolean(active.assistantContent)
          });
          yield {type: "api.status", status: "idle"};
          yield {type: "turn.interrupted", turnId: turn.id};
        }
        return;
      }
      if (!active.terminalPersisted) {
        if (active.controller.signal.aborted) {
          const items = active.assistantContent
            ? [
                this.partialAssistant(turn.id, active.assistantContent, {
                  interrupted: true
                })
              ]
            : [];
          await this.persistTerminal(
            active,
            "interrupted",
            active.assistantContent,
            items
          );
          yield await this.appendTrace("turn.interrupted", {
            turnId: turn.id,
            hadAssistantDraft: Boolean(active.assistantContent)
          });
          yield {type: "api.status", status: "idle"};
          yield {type: "turn.interrupted", turnId: turn.id};
        } else {
          const message = errorMessage(error);
          const items: ThreadItem[] = [];
          if (active.assistantContent) {
            items.push(
              this.partialAssistant(turn.id, active.assistantContent, {failed: true})
            );
          }
          items.push(
            this.sessionService.createItem({
              turnId: turn.id,
              type: "error",
              content: message
            })
          );
          await this.persistTerminal(
            active,
            "failed",
            active.assistantContent,
            items
          );
          yield await this.appendTrace("provider.request.failed", {
            turnId: turn.id,
            providerId:
              error instanceof ProviderError
                ? error.providerId
                : selection.providerProfileId,
            kind:
              error instanceof ProviderError
                ? error.kind
                : error instanceof PrematureProviderCompletionError
                  ? "protocol"
                  : "unknown",
            status: error instanceof ProviderError ? error.status : undefined,
            retryable:
              error instanceof ProviderError ||
              error instanceof PrematureProviderCompletionError
                ? error instanceof ProviderError
                  ? error.retryable
                  : true
                : false,
            requestId: error instanceof ProviderError ? error.requestId : undefined
          });
          yield {type: "error", message, turnId: turn.id};
        }
      }
    } finally {
      try {
        if (active.finalization) {
          await active.finalization;
        } else if (!active.terminalPersisted) {
          active.controller.abort();
          const items = active.assistantContent
            ? [
                this.partialAssistant(turn.id, active.assistantContent, {
                  failed: true
                })
              ]
            : [];
          await this.persistTerminal(
            active,
            "failed",
            active.assistantContent,
            items
          );
        }
      } finally {
        if (this.activeTurn === active) {
          this.activeTurn = null;
        }
      }
    }
  }

  interrupt(): RuntimeEvent {
    if (!this.activeTurn) {
      return {type: "turn.interrupt.ignored", reason: "no_active_request"};
    }
    this.activeTurn.controller.abort();
    return {type: "turn.interrupt.requested", turnId: this.activeTurn.turn.id};
  }

  async compact(
    reason: CompactReason,
    options: CompactOptions = {}
  ): Promise<RuntimeEvent[]> {
    const threadId = this.sessionService.activeThread.id;
    const snapshot = await this.repository.readThread(threadId);
    const selection = this.modelRuntime.getRuntimeSnapshot().selection;
    const beforeContext = this.contextEngine.build({
      modelProjection: contextProjection(selection),
      items: snapshot.items,
      summaries: snapshot.summaries,
      ...(options.userInput !== undefined ? {userInput: options.userInput} : {})
    });
    const boundaryIndex = this.contextEngine.compactionBoundary(
      snapshot.items,
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

    const boundary = snapshot.items[boundaryIndex];
    if (!boundary) {
      throw new Error("Compaction boundary disappeared while preparing the summary.");
    }
    const content = await this.summaryGenerator.generate(
      snapshot.items.slice(0, boundaryIndex + 1),
      reason
    );
    const summary = this.contextEngine.createSummary(threadId, content, boundary.id);
    await this.repository.appendSummary(summary);
    const afterContext = this.contextEngine.build({
      modelProjection: contextProjection(selection),
      items: snapshot.items,
      summaries: [...snapshot.summaries, summary],
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

  async shutdown(): Promise<void> {
    const active = this.activeTurn;
    if (!active) {
      return;
    }
    active.emitInterruptionEvents ||= active.consumerPulling;
    active.controller.abort();
    const items = active.assistantContent
      ? [
          this.partialAssistant(active.turn.id, active.assistantContent, {
            interrupted: true
          })
        ]
      : [];
    try {
      await this.persistTerminal(
        active,
        "interrupted",
        active.assistantContent,
        items
      );
    } finally {
      if (this.activeTurn === active) {
        this.activeTurn = null;
      }
    }
  }

  private createActiveTurn(turn: TurnRecord): ActiveTurn {
    const buffer = new TurnDeltaBuffer(
      (delta, createdAt) =>
        this.repository.appendTurnDelta({
          threadId: turn.threadId,
          turnId: turn.id,
          delta,
          createdAt
        }),
      this.deltaBufferOptions
    );
    return {
      turn,
      controller: new AbortController(),
      buffer,
      assistantContent: "",
      terminalPersisted: false,
      finalization: null,
      consumerPulling: true,
      emitInterruptionEvents: false
    };
  }

  private async stopAfterExternalFinalization(active: ActiveTurn): Promise<boolean> {
    if (!active.finalization) {
      return false;
    }
    await active.finalization;
    return true;
  }

  private async *consumeProviderEvent(
    event: Exclude<ModelAdapterEvent, {type: "completed"}>,
    turn: TurnRecord,
    active: ActiveTurn,
    context: BuiltContext,
    appendContent: (delta: string) => void
  ): AsyncGenerator<RuntimeEvent> {
    if (event.type === "delta") {
      appendContent(event.delta);
      await active.buffer.push(event.delta);
      active.consumerPulling = false;
      yield {type: "assistant.delta", turnId: turn.id, delta: event.delta};
      active.consumerPulling = true;
      return;
    }
    if (event.type === "usage") {
      active.consumerPulling = false;
      yield {
        type: "token.usage",
        used: event.totalTokens ?? context.tokenEstimate,
        limit: context.inputLimit,
        autoCompactAt: context.autoCompactAt
      };
      active.consumerPulling = true;
      return;
    }
    if (event.type === "tool_call") {
      throw new Error("The chat route received an unexpected tool call.");
    }
    active.consumerPulling = false;
    yield await this.appendTrace(event.code, {...event.facts, turnId: turn.id});
    active.consumerPulling = true;
  }

  private async buildContext(
    threadId: string,
    userInput: string,
    selection: ActiveModelSelection
  ): Promise<BuiltContext> {
    const snapshot = await this.repository.readThread(threadId);
    return this.contextEngine.build({
      modelProjection: contextProjection(selection),
      items: snapshot.items,
      summaries: snapshot.summaries,
      userInput
    });
  }

  private partialAssistant(
    turnId: string,
    content: string,
    metadata: {failed?: true; interrupted?: true}
  ): ThreadItem {
    return this.sessionService.createItem({
      turnId,
      type: "assistant_message",
      role: "assistant",
      content,
      metadata: {partial: true, ...metadata}
    });
  }

  private persistTerminal(
    active: ActiveTurn,
    status: TurnStatus,
    assistantContent: string,
    items: ThreadItem[]
  ): Promise<void> {
    if (!active.finalization) {
      active.finalization = this.writeTerminal(
        active,
        status,
        assistantContent,
        items
      );
    }
    return active.finalization;
  }

  private async writeTerminal(
    active: ActiveTurn,
    status: TurnStatus,
    assistantContent: string,
    items: ThreadItem[]
  ): Promise<void> {
    await active.buffer.close();
    if (assistantContent) {
      active.turn.assistantDraft = assistantContent;
    }
    for (const item of items) {
      await this.repository.appendItem(item);
    }
    await this.sessionService.completeTurn(active.turn, status);
    await this.repository.checkpointTurns(active.turn.threadId);
    active.terminalPersisted = true;
  }

  private async appendTrace(
    code: TraceCode,
    facts: Record<string, unknown>
  ): Promise<RuntimeEvent> {
    const event = this.traceRecorder.create(
      this.sessionService.activeThread.id,
      code,
      facts
    );
    await this.repository.appendTrace(event);
    return {type: "trace.event", event};
  }
}

function tokenUsageEvent(context: BuiltContext): RuntimeEvent {
  return {
    type: "token.usage",
    used: context.tokenEstimate,
    limit: context.inputLimit,
    autoCompactAt: context.autoCompactAt
  };
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function abortError(): Error {
  const error = new Error("The active model request was interrupted.");
  error.name = "AbortError";
  return error;
}

function contextProjection(selection: ActiveModelSelection) {
  return {
    model: selection.model,
    providerId: selection.providerProfileId,
    protocol: selection.protocol,
    workingContextLimit: selection.contextWindow,
    maxCompletionTokens: selection.maxOutputTokens,
    autoCompactRatio: selection.autoCompactRatio
  };
}

function modelProvenance(
  selection: ActiveModelSelection
): TurnModelProvenance {
  return Object.freeze({
    schemaVersion: 1,
    adapterId: selection.adapterId,
    providerProfileId: selection.providerProfileId,
    modelProfileId: selection.modelProfileId,
    model: selection.model,
    protocol: selection.protocol
  });
}
