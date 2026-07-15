import type {ApiProtocol, ModelContextMessage} from "../types.js";
import type {ModelToolDefinition} from "../agent/model-action.js";

export interface ProviderUsage {
  inputTokens?: number;
  outputTokens?: number;
  totalTokens?: number;
}

export type ProviderStreamEvent =
  | {type: "ignored"}
  | {type: "delta"; delta: string}
  | {type: "reasoning"; content: string}
  | {
      type: "tool_call.fragments";
      fragments: readonly {
        callId: string;
        name?: string;
        argumentsDelta?: string;
        index?: number;
      }[];
    }
  | ({type: "usage"} & ProviderUsage)
  | {type: "completed"; usage?: ProviderUsage}
  | {
      type: "failed";
      code: ProviderFailureCode;
      category: ProviderFailureCategory;
    };

export type ProviderFailureCategory =
  | "authentication"
  | "rate_limit"
  | "server"
  | "request"
  | "protocol";

export type ProviderFailureCode =
  | "authentication"
  | "rate_limit"
  | "server_error"
  | "invalid_request"
  | "content_filter"
  | "response_failed"
  | "response_incomplete";

export class ProviderProtocolError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "ProviderProtocolError";
  }
}

export interface ProviderRequestInput {
  model: string;
  messages: ModelContextMessage[];
  maxOutputTokens: number;
  tools?: readonly ModelToolDefinition[];
}

export interface ProviderProtocol {
  readonly path: string;
  buildRequest(input: ProviderRequestInput): Record<string, unknown>;
  parseEvent(raw: string): ProviderStreamEvent;
}

class ResponsesProtocol implements ProviderProtocol {
  readonly path = "/responses";

  buildRequest(input: ProviderRequestInput): Record<string, unknown> {
    return {
      model: input.model,
      input: input.messages.map((message) => ({
        role: message.role,
        content: message.content
      })),
      stream: true,
      max_output_tokens: input.maxOutputTokens,
      metadata: {prompt_cache_key: "minimax-codex-v1"},
      ...(input.tools?.length ? {tools: input.tools.map((tool) => ({type: "function", name: tool.name, description: tool.description, parameters: tool.inputSchema}))} : {})
    };
  }

  parseEvent(raw: string): ProviderStreamEvent {
    if (raw === "[DONE]") {
      return {type: "completed"};
    }

    const json = parseJson(raw);
    if (json.type === "response.failed") {
      return classifyFailure(getString(json, ["response", "error", "code"]));
    }
    if (json.type === "response.incomplete") {
      return {
        type: "failed",
        code: "response_incomplete",
        category: "protocol"
      };
    }
    if (json.type === "response.completed") {
      const usage = readUsage(json);
      return usage
        ? {type: "completed", usage: withoutUsageType(usage)}
        : {type: "completed"};
    }
    if (json.type === "response.function_call_arguments.delta") {
      const callId = getString(json, ["item_id"]) ?? getString(json, ["call_id"]);
      if (!callId) throw new ProviderProtocolError("Tool-call fragment is missing an ID.");
      return {type: "tool_call.fragments", fragments: [{callId, argumentsDelta: getString(json, ["delta"]) ?? ""}]};
    }
    if (json.type === "response.output_item.added" || json.type === "response.output_item.done") {
      const item = getValue(json, ["item"]);
      if (isRecord(item) && item.type === "function_call") {
        const callId = getString(item, ["call_id"]) ?? getString(item, ["id"]);
        const name = getString(item, ["name"]);
        if (!callId || !name) throw new ProviderProtocolError("Tool call is missing identity.");
        return {type: "tool_call.fragments", fragments: [{callId, name, ...(typeof item.arguments === "string" ? {argumentsDelta: item.arguments} : {})}]};
      }
    }

    const responseDelta = getString(json, ["delta"]);
    if (responseDelta && json.type === "response.output_text.delta") {
      return {type: "delta", delta: responseDelta};
    }
    if (responseDelta && json.type === "response.reasoning.delta") {
      return {type: "reasoning", content: responseDelta};
    }

    const outputDelta = getString(json, ["output_text", "delta"]);
    if (outputDelta) {
      return {type: "delta", delta: outputDelta};
    }

    return readUsage(json) ?? {type: "ignored"};
  }
}

class ChatCompletionsProtocol implements ProviderProtocol {
  readonly path = "/chat/completions";

  buildRequest(input: ProviderRequestInput): Record<string, unknown> {
    return {
      model: input.model,
      messages: input.messages,
      stream: true,
      stream_options: {include_usage: true},
      max_tokens: input.maxOutputTokens,
      ...(input.tools?.length ? {tools: input.tools.map((tool) => ({type: "function", function: {name: tool.name, description: tool.description, parameters: tool.inputSchema}}))} : {})
    };
  }

  parseEvent(raw: string): ProviderStreamEvent {
    if (raw === "[DONE]") {
      return {type: "completed"};
    }

    const json = parseJson(raw);
    if (isRecord(json.error)) {
      return classifyFailure(getString(json, ["error", "code"]));
    }

    const content = getString(json, ["choices", 0, "delta", "content"]);
    if (content) {
      return {type: "delta", delta: content};
    }

    const reasoning = getString(json, ["choices", 0, "delta", "reasoning_content"]);
    if (reasoning) {
      return {type: "reasoning", content: reasoning};
    }
    const toolCalls = getValue(json, ["choices", 0, "delta", "tool_calls"]);
    if (Array.isArray(toolCalls) && toolCalls.length) {
      const fragments = toolCalls.map((value, arrayIndex) => {
        if (!isRecord(value)) throw new ProviderProtocolError("Malformed tool-call fragment.");
        const index = typeof value.index === "number" ? value.index : arrayIndex;
        const callId = getString(value, ["id"]) ?? `index:${index}`;
        return {
          callId,
          index,
          ...(getString(value, ["function", "name"]) ? {name: getString(value, ["function", "name"])!} : {}),
          ...(getString(value, ["function", "arguments"]) !== null ? {argumentsDelta: getString(value, ["function", "arguments"])!} : {})
        };
      });
      return {type: "tool_call.fragments", fragments};
    }

    return readUsage(json) ?? {type: "ignored"};
  }
}

export function createProviderProtocol(protocol: ApiProtocol): ProviderProtocol {
  if (protocol === "responses") {
    return new ResponsesProtocol();
  }
  if (protocol === "chat_completions") {
    return new ChatCompletionsProtocol();
  }
  throw new ProviderProtocolError("Unsupported provider protocol.");
}

function parseJson(raw: string): Record<string, unknown> {
  try {
    const parsed = JSON.parse(raw) as unknown;
    return typeof parsed === "object" && parsed !== null
      ? (parsed as Record<string, unknown>)
      : {};
  } catch {
    throw new ProviderProtocolError("Malformed provider event.");
  }
}

function classifyFailure(code: string | null): Extract<ProviderStreamEvent, {type: "failed"}> {
  switch (code) {
    case "authentication_error":
    case "invalid_api_key":
      return {type: "failed", code: "authentication", category: "authentication"};
    case "rate_limit_exceeded":
    case "rate_limit_error":
      return {type: "failed", code: "rate_limit", category: "rate_limit"};
    case "server_error":
      return {type: "failed", code: "server_error", category: "server"};
    case "invalid_request_error":
      return {type: "failed", code: "invalid_request", category: "request"};
    case "content_filter":
      return {type: "failed", code: "content_filter", category: "request"};
    default:
      return {type: "failed", code: "response_failed", category: "request"};
  }
}

function readUsage(
  json: Record<string, unknown>
): Extract<ProviderStreamEvent, {type: "usage"}> | null {
  const usage = (json.usage ?? getValue(json, ["response", "usage"])) as
    | Record<string, unknown>
    | undefined;
  if (!usage || typeof usage !== "object") {
    return null;
  }

  const event: Extract<ProviderStreamEvent, {type: "usage"}> = {type: "usage"};
  const inputTokens = getNumber(usage, ["input_tokens"]) ?? getNumber(usage, ["prompt_tokens"]);
  const outputTokens =
    getNumber(usage, ["output_tokens"]) ?? getNumber(usage, ["completion_tokens"]);
  const totalTokens = getNumber(usage, ["total_tokens"]);

  if (inputTokens !== undefined) {
    event.inputTokens = inputTokens;
  }
  if (outputTokens !== undefined) {
    event.outputTokens = outputTokens;
  }
  if (totalTokens !== undefined) {
    event.totalTokens = totalTokens;
  }
  return event;
}

function withoutUsageType(
  usage: Extract<ProviderStreamEvent, {type: "usage"}>
): ProviderUsage {
  const result: ProviderUsage = {};
  if (usage.inputTokens !== undefined) {
    result.inputTokens = usage.inputTokens;
  }
  if (usage.outputTokens !== undefined) {
    result.outputTokens = usage.outputTokens;
  }
  if (usage.totalTokens !== undefined) {
    result.totalTokens = usage.totalTokens;
  }
  return result;
}

function getString(source: unknown, path: Array<string | number>): string | null {
  const value = getValue(source, path);
  return typeof value === "string" ? value : null;
}

function getNumber(source: unknown, path: Array<string | number>): number | undefined {
  const value = getValue(source, path);
  return typeof value === "number" ? value : undefined;
}

function getValue(source: unknown, path: Array<string | number>): unknown {
  let current = source;
  for (const key of path) {
    if (typeof key === "number") {
      if (!Array.isArray(current)) {
        return undefined;
      }
      current = current[key];
      continue;
    }
    if (typeof current !== "object" || current === null) {
      return undefined;
    }
    current = (current as Record<string, unknown>)[key];
  }
  return current;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
