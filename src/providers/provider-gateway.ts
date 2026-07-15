import {getActiveProvider} from "../config/provider-config.js";
import {
  assertProviderSecurity,
  normalizeProviderEndpoint,
  normalizePublicProviderHeaders
} from "../config/provider-security.js";
import type {
  ModelDiagnosticCode,
  ModelDiagnosticFacts
} from "../runtime/model-adapter.js";
import type {ModelToolCall, ModelToolDefinition} from "../agent/model-action.js";
import type {AppConfig, ModelContextMessage} from "../types.js";
import type {ModelProfile} from "./model-profile.js";
import type {ProviderProfile} from "./provider-profile.js";
import {
  FetchHttpStreamTransport,
  type HttpStreamTransport
} from "./http-transport.js";
import {
  createHttpProviderError,
  createStreamProviderError,
  normalizeProviderError
} from "./provider-error.js";
import {
  createProviderProtocol,
  ProviderProtocolError,
  type ProviderUsage
} from "./provider-protocol.js";
import {ReasoningFilter} from "./reasoning-filter.js";

export type ProviderGatewayEvent =
  | {type: "text.delta"; delta: string}
  | {type: "tool.call"; call: ModelToolCall}
  | ({type: "usage"} & ProviderUsage)
  | {type: "diagnostic"; code: ModelDiagnosticCode; facts: ModelDiagnosticFacts}
  | {type: "completed"};

export interface ProviderRequest {
  config: AppConfig;
  apiKey: string;
  messages: ModelContextMessage[];
  signal?: AbortSignal;
  tools?: readonly ModelToolDefinition[];
}

export interface ProviderGateway {
  stream(request: ProviderRequest): AsyncGenerator<ProviderGatewayEvent>;
}

export interface ProfileProviderRequest {
  providerProfile: ProviderProfile;
  modelProfile: ModelProfile;
  apiKey: string;
  messages: readonly ModelContextMessage[];
  maxOutputTokens: number;
  signal?: AbortSignal;
  tools?: readonly ModelToolDefinition[];
}

export interface ProfileProviderGateway {
  streamProfile(request: ProfileProviderRequest): AsyncGenerator<ProviderGatewayEvent>;
}

interface ResolvedProviderRequest {
  providerId: string;
  providerName: string;
  baseUrl: string;
  protocol: ProviderProfile["transport"]["protocol"];
  publicHeaders: Readonly<Record<string, string>>;
  allowInsecureLoopback: boolean;
  model: string;
  apiKey: string;
  messages: readonly ModelContextMessage[];
  maxOutputTokens: number;
  signal?: AbortSignal;
  tools?: readonly ModelToolDefinition[];
}

export class StrictProviderGateway implements ProviderGateway, ProfileProviderGateway {
  constructor(
    private readonly transport: HttpStreamTransport = new FetchHttpStreamTransport()
  ) {}

  async *stream(request: ProviderRequest): AsyncGenerator<ProviderGatewayEvent> {
    const provider = getActiveProvider(request.config);
    yield* this.streamResolved({
      providerId: provider.id,
      providerName: provider.name,
      baseUrl: provider.baseUrl,
      protocol: provider.protocol,
      publicHeaders: normalizePublicProviderHeaders(provider.headers) ?? {},
      allowInsecureLoopback: provider.allowInsecureLoopback ?? false,
      model: request.config.model,
      apiKey: request.apiKey,
      messages: request.messages,
      maxOutputTokens: request.config.context.maxCompletionTokens,
      ...(request.tools ? {tools: request.tools} : {}),
      ...(request.signal ? {signal: request.signal} : {})
    });
  }

  async *streamProfile(
    request: ProfileProviderRequest
  ): AsyncGenerator<ProviderGatewayEvent> {
    if (
      request.modelProfile.providerProfileId !==
      request.providerProfile.providerProfileId
    ) {
      throw new ProviderProtocolError(
        "Model profile does not reference the supplied provider profile."
      );
    }
    yield* this.streamResolved({
      providerId: request.providerProfile.providerProfileId,
      providerName: request.providerProfile.displayName,
      baseUrl: request.providerProfile.transport.baseUrl,
      protocol: request.providerProfile.transport.protocol,
      publicHeaders: request.providerProfile.transport.publicHeaders,
      allowInsecureLoopback:
        request.providerProfile.transport.allowInsecureLoopback,
      model: request.modelProfile.model,
      apiKey: request.apiKey,
      messages: request.messages,
      maxOutputTokens: request.maxOutputTokens,
      ...(request.tools ? {tools: request.tools} : {}),
      ...(request.signal ? {signal: request.signal} : {})
    });
  }

  private async *streamResolved(
    request: ResolvedProviderRequest
  ): AsyncGenerator<ProviderGatewayEvent> {
    assertProviderSecurity({
      name: request.providerName,
      baseUrl: request.baseUrl,
      protocol: request.protocol,
      headers: {...request.publicHeaders},
      allowInsecureLoopback: request.allowInsecureLoopback
    });
    const publicHeaders = normalizePublicProviderHeaders(request.publicHeaders);
    const protocol = createProviderProtocol(request.protocol);
    const body = protocol.buildRequest({
      model: request.model,
      messages: [...request.messages],
      maxOutputTokens: request.maxOutputTokens,
      ...(request.tools ? {tools: request.tools} : {})
    });
    const url = `${normalizeProviderEndpoint(request.baseUrl)}${protocol.path}`;

    yield {
      type: "diagnostic",
      code: "provider.request.started",
      facts: {
        providerId: request.providerId,
        protocol: request.protocol,
        model: request.model
      }
    };

    try {
      const response = await this.transport.postStream({
        url,
        headers: {
          ...publicHeaders,
          Authorization: `Bearer ${request.apiKey}`,
          "Content-Type": "application/json"
        },
        body,
        ...(request.signal ? {signal: request.signal} : {})
      });

      if (!response.ok || !response.body) {
        const responseBody = await response.text().catch(() => "");
        throw createHttpProviderError({
          providerId: request.providerId,
          providerName: request.providerName,
          status: response.status,
          body: responseBody,
          apiKey: request.apiKey
        });
      }

      yield {
        type: "diagnostic",
        code: "provider.stream.started",
        facts: {providerId: request.providerId}
      };

      const reasoningFilter = new ReasoningFilter();
      const toolCalls = new ToolCallAssembler();
      let completed = false;
      for await (const raw of readSseData(response.body)) {
        if (completed) {
          throw new ProviderProtocolError("Provider emitted data after completion.");
        }
        const event = protocol.parseEvent(raw);
        if (event.type === "ignored") {
          continue;
        }
        if (event.type === "completed") {
          completed = true;
          if (event.usage !== undefined) {
            yield {type: "usage", ...event.usage};
          }
          continue;
        }
        if (event.type === "tool_call.fragments") {
          toolCalls.add(event.fragments);
          continue;
        }
        if (event.type === "failed") {
          throw createStreamProviderError({
            providerId: request.providerId,
            providerName: request.providerName,
            kind: event.category,
            code: event.code
          });
        }
        if (event.type === "reasoning") {
          reasoningFilter.hide(event.content);
          continue;
        }
        if (event.type === "delta") {
          for (const visible of reasoningFilter.process(event.delta)) {
            yield {type: "text.delta", delta: visible};
          }
          continue;
        }
        yield event;
      }

      if (!completed) {
        throw new ProviderProtocolError(
          "Provider stream ended before a terminal event."
        );
      }

      for (const visible of reasoningFilter.flush()) {
        yield {type: "text.delta", delta: visible};
      }
      for (const call of toolCalls.finish()) {
        yield {type: "tool.call", call};
      }
      if (reasoningFilter.hiddenCharacters > 0) {
        yield {
          type: "diagnostic",
          code: "provider.reasoning.filtered",
          facts: {
            providerId: request.providerId,
            hiddenCharacters: reasoningFilter.hiddenCharacters
          }
        };
      }
      yield {type: "completed"};
    } catch (error) {
      if (request.signal?.aborted && isAbortError(error)) {
        throw error;
      }
      throw normalizeProviderError({
        providerId: request.providerId,
        providerName: request.providerName,
        error
      });
    }
  }
}

class ToolCallAssembler {
  private readonly calls = new Map<string, {callId: string; name?: string; argumentsJson: string; order: number}>();
  private nextOrder = 0;
  add(fragments: readonly {callId: string; name?: string; argumentsDelta?: string; index?: number}[]): void {
    for (const fragment of fragments) {
      const alias = fragment.callId.startsWith("index:") && fragment.index !== undefined
        ? [...this.calls.values()].find((call) => call.order === fragment.index)?.callId ?? fragment.callId
        : fragment.callId;
      const current = this.calls.get(alias) ?? {callId: alias, argumentsJson: "", order: fragment.index ?? this.nextOrder++};
      if (fragment.name) current.name = fragment.name;
      if (fragment.argumentsDelta) current.argumentsJson += fragment.argumentsDelta;
      this.calls.set(alias, current);
    }
  }
  finish(): readonly ModelToolCall[] {
    return [...this.calls.values()].sort((a, b) => a.order - b.order).map((call) => {
      if (!call.name || !call.callId || !call.argumentsJson) throw new ProviderProtocolError("Incomplete tool call.");
      try { JSON.parse(call.argumentsJson); } catch { throw new ProviderProtocolError("Malformed tool-call arguments."); }
      return Object.freeze({callId: call.callId, name: call.name, argumentsJson: call.argumentsJson});
    });
  }
}

async function* readSseData(stream: ReadableStream<Uint8Array>): AsyncGenerator<string> {
  const reader = stream.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  let pendingCarriageReturn = false;

  const normalizeLineEndings = (chunk: string, final = false): string => {
    let text = pendingCarriageReturn ? `\r${chunk}` : chunk;
    pendingCarriageReturn = false;
    if (!final && text.endsWith("\r")) {
      text = text.slice(0, -1);
      pendingCarriageReturn = true;
    }
    return text.replace(/\r\n/g, "\n").replace(/\r/g, "\n");
  };

  while (true) {
    const {done, value} = await reader.read();
    if (done) {
      break;
    }
    buffer += normalizeLineEndings(decoder.decode(value, {stream: true}));
    const parts = buffer.split("\n\n");
    buffer = parts.pop() ?? "";
    for (const part of parts) {
      const data = readEventData(part);
      if (data) {
        yield data;
      }
    }
  }

  buffer += normalizeLineEndings(decoder.decode(), true);
  if (buffer.trim()) {
    const data = readEventData(buffer);
    if (data) {
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
