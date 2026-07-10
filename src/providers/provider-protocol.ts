import type {ApiProtocol, ModelContextMessage} from "../types.js";

export type ProviderStreamEvent =
  | {type: "delta"; delta: string}
  | {type: "reasoning"; content: string}
  | {type: "usage"; inputTokens?: number; outputTokens?: number; totalTokens?: number};

export interface ProviderRequestInput {
  model: string;
  messages: ModelContextMessage[];
  maxOutputTokens: number;
}

export interface ProviderProtocol {
  readonly path: string;
  buildRequest(input: ProviderRequestInput): Record<string, unknown>;
  parseEvent(raw: string): ProviderStreamEvent | null;
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
      metadata: {prompt_cache_key: "minimax-codex-v1"}
    };
  }

  parseEvent(raw: string): ProviderStreamEvent | null {
    const json = parseJson(raw);
    if (!json) {
      return null;
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

    return readUsage(json);
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
      max_tokens: input.maxOutputTokens
    };
  }

  parseEvent(raw: string): ProviderStreamEvent | null {
    const json = parseJson(raw);
    if (!json) {
      return null;
    }

    const content = getString(json, ["choices", 0, "delta", "content"]);
    if (content) {
      return {type: "delta", delta: content};
    }

    const reasoning = getString(json, ["choices", 0, "delta", "reasoning_content"]);
    if (reasoning) {
      return {type: "reasoning", content: reasoning};
    }

    return readUsage(json);
  }
}

export function createProviderProtocol(protocol: ApiProtocol): ProviderProtocol {
  return protocol === "responses" ? new ResponsesProtocol() : new ChatCompletionsProtocol();
}

function parseJson(raw: string): Record<string, unknown> | null {
  try {
    const parsed = JSON.parse(raw) as unknown;
    return typeof parsed === "object" && parsed !== null
      ? (parsed as Record<string, unknown>)
      : null;
  } catch {
    return null;
  }
}

function readUsage(json: Record<string, unknown>): ProviderStreamEvent | null {
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
