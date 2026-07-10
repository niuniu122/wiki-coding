import {getActiveProvider} from "../config/provider-config.js";
import type {ModelAdapter, ModelAdapterEvent} from "../runtime/model-adapter.js";
import type {AppConfig, ModelContextMessage} from "../types.js";
import {
  FetchHttpStreamTransport,
  type HttpStreamTransport
} from "./http-transport.js";
import {
  createHttpProviderError,
  normalizeProviderError
} from "./provider-error.js";
import {createProviderProtocol} from "./provider-protocol.js";
import {ReasoningFilter} from "./reasoning-filter.js";

export class ProviderModelAdapter implements ModelAdapter {
  constructor(
    private readonly transport: HttpStreamTransport = new FetchHttpStreamTransport()
  ) {}

  async *streamResponse(params: {
    config: AppConfig;
    apiKey: string;
    messages: ModelContextMessage[];
    signal?: AbortSignal;
  }): AsyncGenerator<ModelAdapterEvent> {
    const provider = getActiveProvider(params.config);
    const protocol = createProviderProtocol(provider.protocol);
    const body = protocol.buildRequest({
      model: params.config.model,
      messages: params.messages,
      maxOutputTokens: params.config.context.maxCompletionTokens
    });
    const url = `${provider.baseUrl.replace(/\/$/, "")}${protocol.path}`;

    yield {
      type: "diagnostic",
      code: "provider.request.started",
      facts: {
        providerId: provider.id,
        protocol: provider.protocol,
        model: params.config.model
      }
    };

    try {
      const response = await this.transport.postStream({
        url,
        headers: {
          ...provider.headers,
          Authorization: `Bearer ${params.apiKey}`,
          "Content-Type": "application/json"
        },
        body,
        ...(params.signal ? {signal: params.signal} : {})
      });

      if (!response.ok || !response.body) {
        const responseBody = await response.text().catch(() => "");
        throw createHttpProviderError({
          providerId: provider.id,
          providerName: provider.name,
          status: response.status,
          body: responseBody,
          apiKey: params.apiKey
        });
      }

      yield {
        type: "diagnostic",
        code: "provider.stream.started",
        facts: {providerId: provider.id}
      };

      const reasoningFilter = new ReasoningFilter();
      for await (const raw of readSseData(response.body)) {
        const event = protocol.parseEvent(raw);
        if (!event) {
          continue;
        }
        if (event.type === "reasoning") {
          reasoningFilter.hide(event.content);
          continue;
        }
        if (event.type === "delta") {
          for (const visible of reasoningFilter.process(event.delta)) {
            yield {type: "delta", delta: visible};
          }
          continue;
        }
        yield event;
      }

      for (const visible of reasoningFilter.flush()) {
        yield {type: "delta", delta: visible};
      }
      if (reasoningFilter.hiddenCharacters > 0) {
        yield {
          type: "diagnostic",
          code: "provider.reasoning.filtered",
          facts: {
            providerId: provider.id,
            hiddenCharacters: reasoningFilter.hiddenCharacters
          }
        };
      }
    } catch (error) {
      if (params.signal?.aborted && isAbortError(error)) {
        throw error;
      }
      throw normalizeProviderError({
        providerId: provider.id,
        providerName: provider.name,
        error
      });
    }

    yield {type: "completed"};
  }
}

async function* readSseData(stream: ReadableStream<Uint8Array>): AsyncGenerator<string> {
  const reader = stream.getReader();
  const decoder = new TextDecoder();
  let buffer = "";

  while (true) {
    const {done, value} = await reader.read();
    if (done) {
      break;
    }
    buffer += decoder.decode(value, {stream: true}).replace(/\r\n/g, "\n").replace(/\r/g, "\n");
    const parts = buffer.split("\n\n");
    buffer = parts.pop() ?? "";
    for (const part of parts) {
      const data = readEventData(part);
      if (data && data !== "[DONE]") {
        yield data;
      }
    }
  }

  buffer += decoder.decode().replace(/\r\n/g, "\n").replace(/\r/g, "\n");
  if (buffer.trim()) {
    const data = readEventData(buffer);
    if (data && data !== "[DONE]") {
      yield data;
    }
  }
}

function readEventData(event: string): string {
  return event
    .split("\n")
    .filter((line) => line.startsWith("data:"))
    .map((line) => line.slice(5).trim())
    .join("\n");
}

function isAbortError(error: unknown): boolean {
  return error instanceof DOMException
    ? error.name === "AbortError"
    : error instanceof Error && error.name === "AbortError";
}
