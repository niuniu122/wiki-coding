import type {ModelCatalogView, RuntimeEvent} from "../protocol.js";
import type {ThreadItem, ThreadRecord} from "../types.js";

type CompactCompletedEvent = Extract<RuntimeEvent, {type: "compact.completed"}>;

export function formatCompactionStatus(event: CompactCompletedEvent): string {
  return event.compacted
    ? `上下文已压缩：${event.beforeTokens} → ${event.afterTokens} token`
    : "当前没有可压缩的已完成历史";
}

export interface DisplayMessage {
  id: string;
  role: "user" | "assistant" | "system";
  content: string;
}

export function formatHistoryMessages(items: ThreadItem[]): DisplayMessage[] {
  return items.flatMap((item): DisplayMessage[] => {
    if (item.type === "user_message") {
      return [{id: item.id, role: "user", content: item.content}];
    }
    if (item.type === "assistant_message") {
      const interrupted = item.metadata?.partial === true && item.metadata?.interrupted === true;
      const failed = item.metadata?.partial === true && item.metadata?.failed === true;
      const suffix = interrupted
        ? "\n[上次运行在回复完成前中断]"
        : failed
          ? "\n[回复在发生错误前停止]"
          : "";
      return [
        {
          id: item.id,
          role: "assistant",
          content: `${item.content}${suffix}`
        }
      ];
    }
    if (item.type === "error") {
      return [{id: item.id, role: "system", content: `错误：${item.content}`}];
    }
    if (item.type === "agent_item" && item.agent) {
      const payload = item.agent.payload;
      switch (payload.kind) {
        case "user": return [{id: item.id, role: "user", content: payload.text}];
        case "assistant": return [];
        case "final": return [{id: item.id, role: "assistant", content: payload.text.slice(0, 16_000)}];
        case "tool_request": return [{id: item.id, role: "system", content: `Agent 请求本机能力：${payload.capabilityId}`}];
        case "tool_result": return [{id: item.id, role: "system", content: `Agent 本机能力结果：${payload.status}`}];
        case "checkpoint": return [{id: item.id, role: "system", content: `Agent 检查点：${payload.checkpointId}`}];
        case "error": return [{id: item.id, role: "system", content: `Agent 停止：${payload.code}`}];
      }
    }
    return [];
  });
}

export function formatThreadList(threads: ThreadRecord[]): string {
  if (threads.length === 0) {
    return "暂无历史会话。";
  }
  return [
    "历史会话：",
    ...threads.map((thread) => {
      const marker = thread.status === "active" ? "*" : " ";
      return `${marker} ${thread.id} | ${thread.status} | ${thread.title} | ${thread.updatedAt}`;
    }),
    "使用 /resume <threadId> 切换会话。"
  ].join("\n");
}

export function formatModelCatalog(models: readonly ModelCatalogView[]): string {
  if (models.length === 0) {
    return "没有已注册的模型。";
  }
  return models
    .map((model) => {
      const reason = model.reason ? ` | ${model.reason}` : "";
      return `${model.modelProfileId} | ${model.providerDisplayName} | ${model.availability}${reason}`;
    })
    .join("\n");
}
