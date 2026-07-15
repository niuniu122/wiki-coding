import {AgentBudget, AgentBudgetError, DEFAULT_AGENT_BUDGET_LIMITS, type AgentBudgetLimits} from "../agent/agent-budget.js";
import {auditAgentItems, createAgentCheckpoint} from "../agent/agent-checkpoint.js";
import {AgentContextBuilder, LOCAL_CAPABILITY_TOOL_NAME} from "../agent/agent-context-builder.js";
import type {AgentItemPayload} from "../agent/agent-item.js";
import {parseToolArguments} from "../agent/model-action.js";
import type {CapabilityDispatchResult, CapabilityInvocationRecorder} from "../capabilities/capability-dispatcher.js";
import {createCapabilityInvocation} from "../capabilities/capability-invocation.js";
import type {HybridRetrievalResult} from "../capabilities/search/hybrid-retriever.js";
import type {RuntimeEvent} from "../protocol.js";
import type {SessionRepository} from "../storage/session-repository.js";
import type {ThreadItem, TurnModelProvenance, TurnRecord} from "../types.js";
import {SessionService} from "./session-service.js";
import type {ActiveModelSelection, ModelRuntimeSnapshot, ModelRuntimeSnapshotPort} from "./model-selection-service.js";

type AgentRetrieval = HybridRetrievalResult & {readonly snapshotVersion: string};

export interface AgentCapabilityRetriever {
  retrieve(query: string, inputBudgetTokens?: number): Promise<AgentRetrieval>;
}

export interface AgentCapabilityDispatcherPort {
  dispatch(invocation: ReturnType<typeof createCapabilityInvocation>, signal?: AbortSignal): Promise<CapabilityDispatchResult>;
}

export type AgentCapabilityDispatcherFactory = (recorder: CapabilityInvocationRecorder) => AgentCapabilityDispatcherPort;

export interface AgentRunEngineDependencies {
  readonly sessionService: SessionService;
  readonly repository: SessionRepository;
  readonly modelRuntime: ModelRuntimeSnapshotPort;
  readonly retriever: AgentCapabilityRetriever;
  readonly createDispatcher: AgentCapabilityDispatcherFactory;
  readonly budgetLimits?: AgentBudgetLimits;
  readonly now?: () => number;
}

interface ActiveAgentRun {
  readonly turn: TurnRecord;
  readonly controller: AbortController;
  readonly budget: AgentBudget;
  finalization: Promise<void> | null;
}

interface RunInput {
  readonly input: string;
  readonly turn: TurnRecord;
  readonly runtimeSnapshot: ModelRuntimeSnapshot;
  readonly budget: AgentBudget;
  readonly journal: AgentItemJournal;
  readonly priorItems?: readonly ThreadItem[];
  readonly starting: boolean;
  readonly continuationGeneration: number;
}

export class AgentRunEngine {
  private active: ActiveAgentRun | null = null;

  constructor(private readonly dependencies: AgentRunEngineDependencies) {}

  get hasActiveRun(): boolean { return this.active !== null; }

  async *submit(input: string): AsyncGenerator<RuntimeEvent> {
    if (this.active) throw new Error("Another Agent run is already active.");
    const cleanInput = input.trim();
    if (!cleanInput) throw new Error("Agent input is required.");
    if (!this.agentCompatible()) {
      yield {type: "agent.stopped", turnId: "unavailable", reason: "agent_feature_unsupported"};
      return;
    }
    const runtimeSnapshot = this.dependencies.modelRuntime.getRuntimeSnapshot();
    const turn = await this.dependencies.sessionService.createTurn(cleanInput, modelProvenance(runtimeSnapshot.selection));
    const budget = this.createBudget();
    const journal = new AgentItemJournal(this.dependencies.sessionService, this.dependencies.repository, turn.id, 0);
    yield* this.run({input: cleanInput, turn, runtimeSnapshot, budget, journal, starting: true, continuationGeneration: 0});
  }

  async *continue(): AsyncGenerator<RuntimeEvent> {
    if (this.active) throw new Error("Another Agent run is already active.");
    if (!this.agentCompatible()) {
      yield {type: "agent.stopped", turnId: "unavailable", reason: "agent_feature_unsupported"};
      return;
    }
    const snapshot = await this.dependencies.repository.readThread(this.dependencies.sessionService.activeThread.id);
    const candidate = [...snapshot.turns]
      .filter((turn) => turn.status === "interrupted" || turn.status === "failed")
      .sort((left, right) => right.startedAt.localeCompare(left.startedAt))
      .map((turn) => ({turn, audit: auditAgentItems(turn, snapshot.items)}))
      .find((entry) => entry.audit.status === "recoverable" && entry.audit.checkpoint);
    if (!candidate?.audit.checkpoint) {
      yield {type: "agent.stopped", turnId: "unavailable", reason: "no_recoverable_checkpoint"};
      return;
    }
    const runtimeSnapshot = this.dependencies.modelRuntime.getRuntimeSnapshot();
    if (runtimeSnapshot.selection.modelProfileId !== candidate.audit.checkpoint.modelProfileId) {
      yield {type: "agent.stopped", turnId: candidate.turn.id, reason: "checkpoint_model_changed"};
      return;
    }
    const budget = this.createBudget();
    const priorItems = snapshot.items.filter((item) => item.turnId === candidate.turn.id && item.type === "agent_item");
    const journal = new AgentItemJournal(this.dependencies.sessionService, this.dependencies.repository, candidate.turn.id, candidate.audit.nextSequence);
    yield {type: "agent.continued", turnId: candidate.turn.id, checkpointId: candidate.audit.checkpoint.checkpointId};
    yield* this.run({input: candidate.turn.userInput, turn: candidate.turn, runtimeSnapshot, budget, journal, priorItems, starting: false, continuationGeneration: candidate.audit.checkpoint.continuationGeneration + 1});
  }

  interrupt(): RuntimeEvent {
    if (!this.active) return {type: "turn.interrupt.ignored", reason: "no_active_request"};
    this.active.controller.abort();
    return {type: "turn.interrupt.requested", turnId: this.active.turn.id};
  }

  async shutdown(): Promise<void> {
    const active = this.active;
    if (!active) return;
    active.controller.abort();
    await this.finish(active, "interrupted");
  }

  private async *run(input: RunInput): AsyncGenerator<RuntimeEvent> {
    const active: ActiveAgentRun = {turn: input.turn, controller: new AbortController(), budget: input.budget, finalization: null};
    this.active = active;
    const dispatcher = this.dependencies.createDispatcher(input.journal);
    const timeout = setTimeout(() => active.controller.abort(), input.budget.limits.timeoutMs);
    timeout.unref();
    try {
      if (input.starting) {
        await input.journal.append({kind: "user", text: input.input});
        yield {type: "agent.started", turnId: input.turn.id, input: input.input};
      }
      yield {type: "agent.retrieval.started", turnId: input.turn.id, query: input.input};
      const retrieval = await this.dependencies.retriever.retrieve(input.input, input.runtimeSnapshot.selection.contextWindow);
      if (retrieval.cards.length === 0 || !retrieval.confident) {
        const reason = retrieval.cards.length === 0 ? "no_candidates" : "low_confidence";
        await input.journal.append({kind: "error", code: reason, message: "No confident local capability matched this request."});
        await this.finish(active, "failed");
        yield {type: "agent.stopped", turnId: input.turn.id, reason};
        return;
      }
      if (!input.starting) {
        const priorCheckpoint = latestCheckpoint(input.priorItems ?? []);
        if (!priorCheckpoint || priorCheckpoint.snapshotVersion !== retrieval.snapshotVersion) {
          await input.journal.append({kind: "error", code: "checkpoint_snapshot_changed", message: "The local capability snapshot changed; start a new Agent run."});
          await this.finish(active, "failed");
          yield {type: "agent.stopped", turnId: input.turn.id, reason: "checkpoint_snapshot_changed"};
          return;
        }
      }
      const allowedIds = new Set(retrieval.cards.map((card) => card.id));
      yield {type: "agent.retrieval.completed", turnId: input.turn.id, snapshotVersion: retrieval.snapshotVersion, candidates: retrieval.cards.map((card) => card.id), path: retrieval.path};
      const context = new AgentContextBuilder(input.input, retrieval.cards, Math.min(input.runtimeSnapshot.selection.contextWindow, input.budget.limits.maxTotalTokens));
      if (input.priorItems) replayDurableContext(context, input.priorItems);
      await input.journal.checkpoint(retrieval.snapshotVersion, input.runtimeSnapshot.selection.modelProfileId, input.budget, input.continuationGeneration);

      while (true) {
        const step = input.budget.beginStep();
        const built = context.build();
        input.budget.recordUsage(built.estimatedTokens);
        yield {type: "agent.model.started", turnId: input.turn.id, step};
        let text = "";
        const toolCalls: {callId: string; name: string; argumentsJson: string}[] = [];
        let completed = false;
        for await (const event of input.runtimeSnapshot.runtime.stream({messages: built.messages, tools: built.tools, maxOutputTokens: input.runtimeSnapshot.selection.maxOutputTokens, signal: active.controller.signal})) {
          if (event.type === "delta") {
            text += event.delta;
            if (text.length > 32_000) throw new Error("Agent model output exceeded the bounded item limit.");
            yield {type: "agent.assistant.delta", turnId: input.turn.id, delta: event.delta};
          } else if (event.type === "tool_call") {
            toolCalls.push(event.call);
          } else if (event.type === "usage") {
            input.budget.recordUsage(event.totalTokens ?? (event.inputTokens ?? 0) + (event.outputTokens ?? 0));
          } else if (event.type === "completed") {
            completed = true;
          }
        }
        if (active.controller.signal.aborted) throw abortError();
        if (!completed) throw new Error("Agent model stream ended before completion.");
        if (text) {
          await input.journal.append({kind: "assistant", text});
          context.appendAssistant(text);
        }
        if (toolCalls.length === 0) {
          const finalText = text || "The model completed without a final answer.";
          const item = await input.journal.append({kind: "final", text: finalText});
          await this.finish(active, "completed");
          yield {type: "agent.completed", turnId: input.turn.id, item};
          return;
        }

        context.appendToolCalls(toolCalls);
        for (const call of toolCalls) {
          input.budget.consumeToolCall();
          if (call.name !== LOCAL_CAPABILITY_TOOL_NAME) throw new Error("Model requested an unknown Agent tool.");
          const parsed = parseToolArguments(call);
          const capabilityId = typeof parsed.capabilityId === "string" ? parsed.capabilityId : "";
          const args = parsed.arguments;
          if (!allowedIds.has(capabilityId) || !args || typeof args !== "object" || Array.isArray(args)) throw new Error("Model requested a capability outside the retrieved local set.");
          const invocation = createCapabilityInvocation({invocationId: call.callId, capabilityId, snapshotVersion: retrieval.snapshotVersion, arguments: args as Record<string, unknown>, approved: false});
          yield {type: "agent.tool.requested", turnId: input.turn.id, invocationId: invocation.invocationId, capabilityId};
          const result = await dispatcher.dispatch(invocation, active.controller.signal);
          yield {type: "agent.tool.completed", turnId: input.turn.id, invocationId: invocation.invocationId, status: result.status};
          const output = dispatchOutput(result);
          context.appendToolResult(call.callId, capabilityId, result.status, output);
          await input.journal.checkpoint(retrieval.snapshotVersion, input.runtimeSnapshot.selection.modelProfileId, input.budget, input.continuationGeneration);
          if (result.status === "confirmation_required") {
            await this.finish(active, "interrupted");
            yield {type: "agent.permission.required", turnId: input.turn.id, invocationId: invocation.invocationId, capabilityId};
            return;
          }
        }
      }
    } catch (error) {
      const reason = error instanceof AgentBudgetError ? error.code : active.controller.signal.aborted ? input.budget.expired ? "time_limit" : "interrupted" : "agent_failed";
      if (!active.finalization) {
        await input.journal.append({kind: "error", code: reason, message: safeErrorMessage(error)});
        await this.finish(active, reason === "interrupted" ? "interrupted" : "failed");
      }
      yield {type: "agent.stopped", turnId: input.turn.id, reason};
    } finally {
      clearTimeout(timeout);
      if (!active.finalization) await this.finish(active, "failed");
      if (this.active === active) this.active = null;
    }
  }

  private agentCompatible(): boolean {
    try { this.dependencies.modelRuntime.assertAgentCompatible(); return true; } catch { return false; }
  }

  private createBudget(): AgentBudget {
    return new AgentBudget(this.dependencies.budgetLimits ?? DEFAULT_AGENT_BUDGET_LIMITS, this.dependencies.now);
  }

  private async finish(active: ActiveAgentRun, status: "completed" | "failed" | "interrupted"): Promise<void> {
    if (!active.finalization) {
      active.finalization = (async () => {
        await this.dependencies.sessionService.completeTurn(active.turn, status);
        await this.dependencies.repository.checkpointTurns(active.turn.threadId);
      })();
    }
    await active.finalization;
  }
}

class AgentItemJournal implements CapabilityInvocationRecorder {
  private sequence: number;
  constructor(private readonly session: SessionService, private readonly repository: SessionRepository, private readonly turnId: string, sequence: number) { this.sequence = sequence; }
  get lastSequence(): number { return this.sequence - 1; }

  async append(payload: AgentItemPayload) {
    const item = this.session.createAgentItem({turnId: this.turnId, sequence: this.sequence++, payload});
    await this.repository.appendItem(item);
    return item;
  }

  async checkpoint(snapshotVersion: string, modelProfileId: string, budget: AgentBudget, continuationGeneration: number): Promise<void> {
    if (this.lastSequence < 0) throw new Error("Cannot checkpoint an empty Agent journal.");
    await this.append(createAgentCheckpoint({turnId: this.turnId, lastSequence: this.lastSequence, continuationGeneration, snapshotVersion, modelProfileId, budget}));
  }

  async recordRequest(invocation: ReturnType<typeof createCapabilityInvocation>): Promise<void> {
    await this.append({kind: "tool_request", invocationId: invocation.invocationId, capabilityId: invocation.capabilityId, arguments: invocation.arguments});
  }

  async recordResult(invocation: ReturnType<typeof createCapabilityInvocation>, result: CapabilityDispatchResult): Promise<void> {
    await this.append({kind: "tool_result", invocationId: invocation.invocationId, status: result.status === "succeeded" ? "completed" : "failed", output: dispatchOutput(result).slice(0, 32_000)});
  }
}

function replayDurableContext(context: AgentContextBuilder, items: readonly ThreadItem[]): void {
  const capabilityByInvocation = new Map<string, string>();
  for (const item of [...items].sort((left, right) => (left.agent?.sequence ?? 0) - (right.agent?.sequence ?? 0))) {
    const payload = item.agent?.payload;
    if (payload?.kind === "tool_request") {
      capabilityByInvocation.set(payload.invocationId, payload.capabilityId);
      context.appendToolCalls([{
        callId: payload.invocationId,
        name: LOCAL_CAPABILITY_TOOL_NAME,
        argumentsJson: JSON.stringify({
          capabilityId: payload.capabilityId,
          arguments: payload.arguments
        })
      }]);
    }
    else if (payload?.kind === "assistant") context.appendAssistant(payload.text);
    else if (payload?.kind === "tool_result") context.appendToolResult(
      payload.invocationId,
      capabilityByInvocation.get(payload.invocationId) ?? "unknown",
      payload.status,
      payload.output
    );
  }
}

function latestCheckpoint(items: readonly ThreadItem[]) {
  return items.flatMap((item) => item.agent?.payload.kind === "checkpoint" ? [item.agent.payload] : []).at(-1);
}

function dispatchOutput(result: CapabilityDispatchResult): string {
  if (result.status === "succeeded") return result.output;
  if (result.status === "failed") return `failed:${result.code}`;
  if (result.status === "denied") return `denied:${result.reason}`;
  return result.status;
}

function safeErrorMessage(error: unknown): string {
  if (error instanceof AgentBudgetError) return error.message;
  if (error instanceof Error && error.name === "AbortError") return "The Agent run was interrupted.";
  return "The Agent run stopped at a validated boundary.";
}

function abortError(): Error { const error = new Error("Agent interrupted."); error.name = "AbortError"; return error; }

function modelProvenance(selection: ActiveModelSelection): TurnModelProvenance {
  return {schemaVersion: 1, adapterId: selection.adapterId, providerProfileId: selection.providerProfileId, modelProfileId: selection.modelProfileId, model: selection.model, protocol: selection.protocol};
}
