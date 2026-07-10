import type {AppConfig, ContextSummary, ModelContextMessage, ThreadItem} from "../types.js";
import {createId} from "../utils/id.js";
import {getActiveProvider} from "../config/provider-config.js";

const STABLE_PROMPT = [
  "You are MiniMax Codex, a local coding-agent style CLI shell.",
  "Respond clearly and keep work trace separate from user-facing answers.",
  "Do not include secrets, API keys, or private local credentials in responses.",
  "When unsure, ask concise clarification questions before taking risky actions."
].join("\n");

export interface BuiltContext {
  messages: ModelContextMessage[];
  tokenEstimate: number;
  inputLimit: number;
  autoCompactAt: number;
  shouldCompact: boolean;
  summaryBoundaryValid: boolean;
}

export class ContextManager {
  buildContext(params: {
    config: AppConfig;
    items: ThreadItem[];
    summaries: ContextSummary[];
    userInput?: string;
  }): BuiltContext {
    const provider = getActiveProvider(params.config);
    const inputLimit = Math.max(
      1,
      params.config.context.workingContextLimit - params.config.context.maxCompletionTokens
    );
    const autoCompactAt = Math.max(
      1,
      Math.floor(inputLimit * params.config.context.autoCompactRatio)
    );
    const latestSummary = params.summaries.at(-1);
    const boundaryIndex = latestSummary?.coveredThroughItemId
      ? params.items.findIndex((item) => item.id === latestSummary.coveredThroughItemId)
      : -1;
    const summaryBoundaryValid = Boolean(latestSummary?.coveredThroughItemId) && boundaryIndex >= 0;
    const visibleItems = summaryBoundaryValid ? params.items.slice(boundaryIndex + 1) : params.items;
    const recentMessages = visibleItems
      .filter(isModelVisibleMessage)
      .map((item) => ({
        role: item.role ?? "user",
        content: item.content
      }));

    const messages: ModelContextMessage[] = [
      {role: "system", content: STABLE_PROMPT},
      {
        role: "system",
        content: [
          `Project: minimax-codex`,
          `Model: ${params.config.model}`,
          `Model provider: ${provider.id}`,
          `Protocol: ${provider.protocol}`,
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

    const tokenEstimate = estimateTokens(messages.map((message) => message.content).join("\n"));

    return {
      messages,
      tokenEstimate,
      inputLimit,
      autoCompactAt,
      shouldCompact: tokenEstimate >= autoCompactAt,
      summaryBoundaryValid
    };
  }

  findCompactionBoundary(items: ThreadItem[], preserveTurnId?: string): number {
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

  createSummaryRecord(
    threadId: string,
    content: string,
    _reason: "manual" | "auto" | "resume",
    coveredThroughItemId: string
  ): ContextSummary {
    return {
      id: createId("summary"),
      threadId,
      createdAt: new Date().toISOString(),
      content,
      tokenEstimate: estimateTokens(content),
      coveredThroughItemId
    };
  }
}

function isModelVisibleMessage(item: ThreadItem): boolean {
  if (item.type === "user_message") {
    return true;
  }
  return item.type === "assistant_message" && item.metadata?.partial !== true;
}

export function estimateTokens(text: string): number {
  return Math.ceil(text.length / 4);
}
