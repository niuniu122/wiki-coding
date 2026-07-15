import {getActiveProvider} from "../config/provider-config.js";
import type {
  ApiProtocol,
  AppConfig,
  ContextSummary,
  ModelContextMessage,
  ThreadItem
} from "../types.js";
import {createId} from "../utils/id.js";
import {
  ConservativeTokenEstimator,
  type TokenEstimator
} from "./token-estimator.js";

const STABLE_PROMPT = [
  "You are MiniMax Codex, a local coding-agent style CLI shell.",
  "Respond clearly and keep work trace separate from user-facing answers.",
  "Do not include secrets, API keys, or private local credentials in responses.",
  "When unsure, ask concise clarification questions before taking risky actions."
].join("\n");

interface BuildContextBase {
  items: ThreadItem[];
  summaries: ContextSummary[];
  userInput?: string;
}

export interface ModelContextProjection {
  readonly model: string;
  readonly providerId: string;
  readonly protocol: ApiProtocol;
  readonly workingContextLimit: number;
  readonly maxCompletionTokens: number;
  readonly autoCompactRatio: number;
}

export type BuildContextParams = BuildContextBase &
  (
    | {config: AppConfig; modelProjection?: never}
    | {config?: never; modelProjection: ModelContextProjection}
  );

export interface BuiltContext {
  messages: ModelContextMessage[];
  tokenEstimate: number;
  inputLimit: number;
  autoCompactAt: number;
  shouldCompact: boolean;
  summaryBoundaryValid: boolean;
}

export class ContextEngine {
  constructor(
    private readonly estimator: TokenEstimator = new ConservativeTokenEstimator()
  ) {}

  build(params: BuildContextParams): BuiltContext {
    const projection = resolveModelProjection(params);
    const inputLimit = Math.max(
      1,
      projection.workingContextLimit - projection.maxCompletionTokens
    );
    const autoCompactAt = Math.max(
      1,
      Math.floor(inputLimit * projection.autoCompactRatio)
    );
    const latestSummary = params.summaries.at(-1);
    const boundaryIndex = latestSummary?.coveredThroughItemId
      ? params.items.findIndex((item) => item.id === latestSummary.coveredThroughItemId)
      : -1;
    const summaryBoundaryValid = Boolean(latestSummary?.coveredThroughItemId) && boundaryIndex >= 0;
    const visibleItems = summaryBoundaryValid
      ? params.items.slice(boundaryIndex + 1)
      : params.items;
    const recentMessages = visibleItems
      .filter(isModelVisibleMessage)
      .map<ModelContextMessage>((item) => ({
        role: item.role ?? "user",
        content: item.content
      }));

    const messages: ModelContextMessage[] = [
      {role: "system", content: STABLE_PROMPT},
      {
        role: "system",
        content: [
          "Project: minimax-codex",
          `Model: ${projection.model}`,
          `Model provider: ${projection.providerId}`,
          `Protocol: ${projection.protocol}`,
          "Full trace is stored locally and should not be treated as model context."
        ].join("\n")
      }
    ];

    if (latestSummary) {
      messages.push({
        role: "system",
        content: `Conversation summary:\n${latestSummary.content}`
      });
    }

    messages.push(...recentMessages);
    const latestMessage = recentMessages.at(-1);
    if (
      params.userInput !== undefined &&
      (latestMessage?.role !== "user" || latestMessage.content !== params.userInput)
    ) {
      messages.push({role: "user", content: params.userInput});
    }

    const tokenEstimate = this.estimator.estimateMessages(messages);

    return {
      messages,
      tokenEstimate,
      inputLimit,
      autoCompactAt,
      shouldCompact: tokenEstimate >= autoCompactAt,
      summaryBoundaryValid
    };
  }

  compactionBoundary(items: ThreadItem[], preserveTurnId?: string): number {
    const preservedTurnStart = preserveTurnId
      ? items.findIndex((item) => item.turnId === preserveTurnId)
      : -1;
    const searchBefore = preservedTurnStart >= 0 ? preservedTurnStart : items.length;

    for (let index = searchBefore - 1; index >= 0; index--) {
      if (items[index]?.type === "assistant_message" && items[index]?.metadata?.partial !== true) {
        return index;
      }
    }

    return -1;
  }

  createSummary(
    threadId: string,
    content: string,
    coveredThroughItemId: string
  ): ContextSummary {
    return {
      id: createId("summary"),
      threadId,
      createdAt: new Date().toISOString(),
      content,
      tokenEstimate: this.estimator.estimateText(content),
      coveredThroughItemId
    };
  }

  buildContext(params: BuildContextParams): BuiltContext {
    return this.build(params);
  }

  findCompactionBoundary(items: ThreadItem[], preserveTurnId?: string): number {
    return this.compactionBoundary(items, preserveTurnId);
  }

  createSummaryRecord(
    threadId: string,
    content: string,
    _reason: "manual" | "auto" | "resume",
    coveredThroughItemId: string
  ): ContextSummary {
    return this.createSummary(threadId, content, coveredThroughItemId);
  }
}

function resolveModelProjection(params: BuildContextParams): ModelContextProjection {
  if (params.modelProjection) {
    return params.modelProjection;
  }
  const provider = getActiveProvider(params.config);
  return {
    model: params.config.model,
    providerId: provider.id,
    protocol: provider.protocol,
    workingContextLimit: params.config.context.workingContextLimit,
    maxCompletionTokens: params.config.context.maxCompletionTokens,
    autoCompactRatio: params.config.context.autoCompactRatio
  };
}

function isModelVisibleMessage(item: ThreadItem): boolean {
  if (item.type === "user_message") {
    return true;
  }
  return item.type === "assistant_message" && item.metadata?.partial !== true;
}
