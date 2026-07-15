import assert from "node:assert/strict";
import test from "node:test";
import {DEFAULT_CONFIG} from "../src/config/config-manager.js";
import {ProviderError} from "../src/providers/provider-error.js";
import {
  ProviderModelAdapter
} from "../src/providers/provider-model-adapter.js";
import type {
  HttpStreamRequest,
  HttpStreamTransport
} from "../src/providers/http-transport.js";
import type {AppConfig} from "../src/types.js";
import type {ModelAdapterEvent} from "../src/runtime/model-adapter.js";

class StubTransport implements HttpStreamTransport {
  requests: HttpStreamRequest[] = [];

  constructor(private readonly response: Response) {}

  async postStream(request: HttpStreamRequest): Promise<Response> {
    this.requests.push(request);
    return this.response;
  }
}

function sseResponse(events: unknown[]): Response {
  return new Response(
    events.map((event) => `data: ${JSON.stringify(event)}\n\n`).join("") + "data: [DONE]\n\n",
    {status: 200, headers: {"Content-Type": "text/event-stream"}}
  );
}

function adapterForSse(events: unknown[]): ProviderModelAdapter {
  return adapterForRawSse(
    events.map((event) => `data: ${JSON.stringify(event)}\n\n`).join("")
  );
}

function adapterForRawSse(raw: string): ProviderModelAdapter {
  return new ProviderModelAdapter(
    new StubTransport(
      new Response(raw, {status: 200, headers: {"Content-Type": "text/event-stream"}})
    )
  );
}

function adapterForSseChunks(chunks: string[]): ProviderModelAdapter {
  const encoder = new TextEncoder();
  const body = new ReadableStream<Uint8Array>({
    start(controller) {
      for (const chunk of chunks) {
        controller.enqueue(encoder.encode(chunk));
      }
      controller.close();
    }
  });
  return new ProviderModelAdapter(
    new StubTransport(
      new Response(body, {status: 200, headers: {"Content-Type": "text/event-stream"}})
    )
  );
}

async function collect(
  adapter: ProviderModelAdapter,
  config: AppConfig = DEFAULT_CONFIG
): Promise<ModelAdapterEvent[]> {
  const events: ModelAdapterEvent[] = [];
  for await (const event of adapter.streamResponse({
    config,
    apiKey: "fake-test-key",
    messages: [{role: "user", content: "hello"}]
  })) {
    events.push(event);
  }
  return events;
}

function chatConfig(): AppConfig {
  return {
    ...DEFAULT_CONFIG,
    modelProvider: "hashsight",
    modelProviders: {...DEFAULT_CONFIG.modelProviders},
    context: {...DEFAULT_CONFIG.context},
    model: "MiniMax-M3"
  };
}

test("responses requires response.completed", async () => {
  const adapter = adapterForSse([
    {type: "response.output_text.delta", delta: "partial"}
  ]);

  await assert.rejects(
    () => collect(adapter),
    (error: unknown) =>
      error instanceof ProviderError &&
      error.kind === "protocol" &&
      error.retryable
  );
});

test("malformed SSE JSON is a redacted protocol failure", async () => {
  const rawFrame = "{not-json SECRET_FRAME_CONTENT}";
  const adapter = adapterForRawSse(`data: ${rawFrame}\n\n`);

  await assert.rejects(
    () => collect(adapter),
    (error: unknown) =>
      error instanceof ProviderError &&
      error.kind === "protocol" &&
      /malformed provider event/i.test(error.message) &&
      !error.message.includes("SECRET_FRAME_CONTENT")
  );
});

test("chat completions succeeds after DONE", async () => {
  const events = await collect(
    adapterForRawSse(
      'data: {"choices":[{"delta":{"content":"ok"}}]}\n\ndata: [DONE]\n\n'
    ),
    chatConfig()
  );

  assert.equal(events.at(-1)?.type, "completed");
});

test("chat completions rejects EOF without DONE", async () => {
  const adapter = adapterForRawSse(
    'data: {"choices":[{"delta":{"content":"partial"}}]}\n\n'
  );

  await assert.rejects(
    () => collect(adapter, chatConfig()),
    (error: unknown) => error instanceof ProviderError && error.kind === "protocol"
  );
});

test("provider rejects visible data after completion", async () => {
  const adapter = adapterForRawSse(
    [
      'data: {"type":"response.completed"}',
      'data: {"type":"response.output_text.delta","delta":"late"}'
    ].join("\n\n") + "\n\n"
  );

  await assert.rejects(
    () => collect(adapter),
    (error: unknown) => error instanceof ProviderError && error.kind === "protocol"
  );
});

test("provider rejects duplicate completion", async () => {
  const adapter = adapterForRawSse(
    [
      'data: {"type":"response.completed"}',
      'data: {"type":"response.completed"}'
    ].join("\n\n") + "\n\n"
  );

  await assert.rejects(
    () => collect(adapter),
    (error: unknown) => error instanceof ProviderError && error.kind === "protocol"
  );
});

test("provider rejects an unknown data frame after completion", async () => {
  const adapter = adapterForRawSse(
    [
      'data: {"type":"response.completed"}',
      'data: {"type":"response.unknown"}'
    ].join("\n\n") + "\n\n"
  );

  await assert.rejects(
    () => collect(adapter),
    (error: unknown) => error instanceof ProviderError && error.kind === "protocol"
  );
});

test("responses failure cannot be overwritten by a later completion", async () => {
  const adapter = adapterForRawSse(
    [
      'data: {"type":"response.failed","response":{"error":{"code":"rate_limit_exceeded","message":"SECRET_PROMPT fake-test-key"}}}',
      'data: {"type":"response.completed"}'
    ].join("\n\n") + "\n\n"
  );

  await assert.rejects(
    () => collect(adapter),
    (error: unknown) =>
      error instanceof ProviderError &&
      error.kind === "rate_limit" &&
      error.code === "rate_limit" &&
      !error.message.includes("SECRET_PROMPT") &&
      !error.message.includes("fake-test-key")
  );
});

test("responses incomplete fails at EOF with only allowlisted details", async () => {
  const adapter = adapterForRawSse(
    'data: {"type":"response.incomplete","response":{"incomplete_details":{"reason":"SECRET_RAW_REASON"}}}\n\n'
  );

  await assert.rejects(
    () => collect(adapter),
    (error: unknown) =>
      error instanceof ProviderError &&
      error.kind === "protocol" &&
      error.code === "response_incomplete" &&
      !error.message.includes("SECRET_RAW_REASON")
  );
});

test("chat streaming error object fails even when DONE follows", async () => {
  const adapter = adapterForRawSse(
    'data: {"error":{"code":"authentication_error","message":"LEAKED_KEY fake-test-key"}}\n\ndata: [DONE]\n\n'
  );

  await assert.rejects(
    () => collect(adapter, chatConfig()),
    (error: unknown) =>
      error instanceof ProviderError &&
      error.kind === "authentication" &&
      error.code === "authentication" &&
      !error.message.includes("LEAKED_KEY") &&
      !error.message.includes("fake-test-key")
  );
});

test("SSE parsing preserves a CRLF split across network chunks", async () => {
  const adapter = adapterForSseChunks([
    'data: {"type":"response.output_text.delta",\r',
    '\ndata: "delta":"ok"}\r\n\r\ndata: {"type":"response.completed"}\r\n\r\n'
  ]);

  const events = await collect(adapter);

  assert.equal(
    events.filter((event) => event.type === "delta").map((event) => event.delta).join(""),
    "ok"
  );
  assert.equal(events.at(-1)?.type, "completed");
});

test("provider adapter drops raw reasoning and emits only a safe diagnostic", async () => {
  const transport = new StubTransport(
    sseResponse([
      {choices: [{delta: {reasoning_content: "RAW_PRIVATE_REASONING"}}]},
      {choices: [{delta: {content: "<think>HIDDEN_TAG_REASONING</think> visible answer"}}]},
      {usage: {prompt_tokens: 8, completion_tokens: 2, total_tokens: 10}}
    ])
  );
  const adapter = new ProviderModelAdapter(transport);
  const events = [];

  for await (const event of adapter.streamResponse({
    config: chatConfig(),
    apiKey: "fake-test-key",
    messages: [{role: "user", content: "PRIVATE_USER_PROMPT"}]
  })) {
    events.push(event);
  }

  assert.equal(
    events.filter((event) => event.type === "delta").map((event) => event.delta).join(""),
    "visible answer"
  );
  assert.equal(
    events.some(
      (event) => event.type === "diagnostic" && event.code === "provider.reasoning.filtered"
    ),
    true
  );
  const serializedEvents = JSON.stringify(events);
  assert.equal(serializedEvents.includes("RAW_PRIVATE_REASONING"), false);
  assert.equal(serializedEvents.includes("HIDDEN_TAG_REASONING"), false);
  assert.equal(serializedEvents.includes("PRIVATE_USER_PROMPT"), false);
  assert.equal(transport.requests.length, 1);
});

test("provider errors are structured and redact credentials echoed by an upstream response", async () => {
  const apiKey = "secret-provider-key";
  const transport = new StubTransport(
    new Response(
      JSON.stringify({
        error: {message: `invalid ${apiKey} (1004)`, type: "authentication_error"},
        request_id: "req_test_1"
      }),
      {status: 401, headers: {"Content-Type": "application/json"}}
    )
  );
  const adapter = new ProviderModelAdapter(transport);

  await assert.rejects(
    async () => {
      for await (const _event of adapter.streamResponse({
        config: chatConfig(),
        apiKey,
        messages: [{role: "user", content: "hello"}]
      })) {
        // Consume until the provider error is raised.
      }
    },
    (error: unknown) =>
      error instanceof ProviderError &&
      error.kind === "authentication" &&
      error.requestId === "req_test_1" &&
      !error.message.includes(apiKey)
  );
});
