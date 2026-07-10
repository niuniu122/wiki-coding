import type {RuntimeEvent} from "../protocol.js";
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
