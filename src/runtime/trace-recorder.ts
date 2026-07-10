import type {
  TraceCategory,
  TraceCode,
  TraceEvent,
  TraceFact
} from "../types.js";
import {createId} from "../utils/id.js";

interface TraceDefinition {
  category: TraceCategory;
  message: string;
  allowedFacts: readonly string[];
}

const TRACE_DEFINITIONS: Record<TraceCode, TraceDefinition> = {
  "turn.start": {
    category: "lifecycle",
    message: "收到用户输入，开始整理上下文。",
    allowedFacts: []
  },
  "turn.recovered": {
    category: "lifecycle",
    message: "检测到上次异常结束的 Turn，已标记为 interrupted。",
    allowedFacts: ["hadAssistantDraft"]
  },
  "turn.interrupted": {
    category: "lifecycle",
    message: "用户主动取消了当前模型请求。",
    allowedFacts: ["hadAssistantDraft"]
  },
  "compact.completed": {
    category: "context",
    message: "已生成本地上下文摘要。",
    allowedFacts: [
      "reason",
      "summaryId",
      "coveredThroughItemId",
      "beforeTokens",
      "afterTokens"
    ]
  },
  "compact.limit": {
    category: "context",
    message: "压缩后上下文仍超过安全上限，本轮没有调用模型。",
    allowedFacts: ["tokenEstimate", "autoCompactAt", "inputLimit"]
  },
  "provider.request.started": {
    category: "provider",
    message: "模型请求已发送。",
    allowedFacts: ["providerId", "protocol", "model"]
  },
  "provider.stream.started": {
    category: "provider",
    message: "模型开始返回流式响应。",
    allowedFacts: ["providerId"]
  },
  "provider.reasoning.filtered": {
    category: "provider",
    message: "模型返回的隐藏推理已过滤，不会写入聊天或持久化 trace。",
    allowedFacts: ["providerId", "hiddenCharacters"]
  },
  "provider.request.failed": {
    category: "error",
    message: "模型请求失败。",
    allowedFacts: ["providerId", "kind", "status", "retryable", "requestId"]
  }
};

export class SafeTraceRecorder {
  create(threadId: string, code: TraceCode, input: Record<string, unknown> = {}): TraceEvent {
    const definition = TRACE_DEFINITIONS[code];
    const facts: Record<string, TraceFact> = {};
    for (const key of definition.allowedFacts) {
      const value = input[key];
      if (isTraceFact(value)) {
        facts[key] = value;
      }
    }

    const event: TraceEvent = {
      id: createId("trace"),
      threadId,
      category: definition.category,
      code,
      message: definition.message,
      createdAt: new Date().toISOString()
    };
    if (typeof input.turnId === "string") {
      event.turnId = input.turnId;
    }
    if (Object.keys(facts).length > 0) {
      event.facts = facts;
    }
    return event;
  }
}

function isTraceFact(value: unknown): value is TraceFact {
  return (
    value === null ||
    typeof value === "string" ||
    typeof value === "number" ||
    typeof value === "boolean"
  );
}
