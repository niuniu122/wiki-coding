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

function chatConfig(): AppConfig {
  return {
    ...DEFAULT_CONFIG,
    modelProvider: "hashsight",
    modelProviders: {...DEFAULT_CONFIG.modelProviders},
    api: {...DEFAULT_CONFIG.api},
    storage: {...DEFAULT_CONFIG.storage},
    context: {...DEFAULT_CONFIG.context},
    model: "MiniMax-M3"
  };
}

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
