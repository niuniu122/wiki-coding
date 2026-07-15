import type {CapabilityCard} from "../capabilities/search/capability-card.js";
import type {ModelContextMessage} from "../types.js";
import type {ModelToolDefinition} from "./model-action.js";

export const LOCAL_CAPABILITY_TOOL_NAME = "invoke_local_capability";

export interface AgentContext {
  readonly messages: readonly ModelContextMessage[];
  readonly tools: readonly ModelToolDefinition[];
  readonly estimatedTokens: number;
}

export class AgentContextBuilder {
  private readonly messages: ModelContextMessage[];
  private readonly tools: readonly ModelToolDefinition[];

  constructor(input: string, cards: readonly CapabilityCard[], private readonly maxContextTokens: number) {
    if (!input.trim() || cards.length === 0 || cards.length > 5) throw new Error("Agent context requires input and one to five capability cards.");
    const ids = cards.map((card) => card.id);
    this.messages = [
      {
        role: "system",
        content: [
          "You are operating a bounded local Agent loop.",
          "Use only the listed local capabilities. Never invent a capability ID or claim an execution result.",
          "Call invoke_local_capability when local evidence is needed; otherwise return the final answer as text.",
          `Capability cards: ${JSON.stringify(cards)}`
        ].join("\n")
      },
      {role: "user", content: input}
    ];
    this.tools = Object.freeze([Object.freeze({
      name: LOCAL_CAPABILITY_TOOL_NAME,
      description: "Invoke one capability from the locally retrieved, policy-checked catalog.",
      inputSchema: Object.freeze({
        type: "object",
        additionalProperties: false,
        required: ["capabilityId", "arguments"],
        properties: Object.freeze({
          capabilityId: Object.freeze({type: "string", enum: Object.freeze(ids)}),
          arguments: Object.freeze({type: "object"})
        })
      })
    })]);
    this.assertWithinBudget();
  }

  appendAssistant(text: string): void {
    if (text) this.messages.push({role: "assistant", content: text.slice(0, 16_000)});
    this.assertWithinBudget();
  }

  appendToolResult(capabilityId: string, status: string, output: string): void {
    this.messages.push({
      role: "system",
      content: `Local capability result (${capabilityId}, ${status}):\n${output.slice(0, 12_000)}`
    });
    this.assertWithinBudget();
  }

  build(): AgentContext {
    const messages = Object.freeze(this.messages.map((message) => Object.freeze({...message})));
    return Object.freeze({messages, tools: this.tools, estimatedTokens: estimateTokens(messages)});
  }

  private assertWithinBudget(): void {
    if (estimateTokens(this.messages) > this.maxContextTokens) throw new Error("Agent context exceeds its local token budget.");
  }
}

function estimateTokens(messages: readonly ModelContextMessage[]): number {
  return Math.ceil(messages.reduce((sum, message) => sum + message.content.length, 0) / 4);
}
