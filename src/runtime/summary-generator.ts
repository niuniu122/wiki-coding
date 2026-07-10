import type {ThreadItem} from "../types.js";

export type CompactReason = "manual" | "auto" | "resume";

export interface SummaryGenerator {
  generate(items: ThreadItem[], reason: CompactReason): Promise<string>;
}

export class LocalSummaryGenerator implements SummaryGenerator {
  async generate(items: ThreadItem[], reason: CompactReason): Promise<string> {
    const exchanges = items
      .filter(
        (item) =>
          item.type === "user_message" ||
          (item.type === "assistant_message" && item.metadata?.partial !== true)
      )
      .slice(-6)
      .map((item) => {
        const label = item.role === "assistant" ? "Agent" : "用户";
        return `${label}：${item.content.slice(0, 320)}`;
      });

    return [
      `压缩原因：${reason}`,
      "以下内容代表已覆盖的旧会话；原始记录仍保存在本地。",
      ...exchanges
    ]
      .join("\n")
      .slice(0, 2400);
  }
}
