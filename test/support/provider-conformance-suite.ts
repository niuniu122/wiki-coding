import assert from "node:assert/strict";
import test from "node:test";
import type {ApiProtocol} from "../../src/types.js";
import type {HttpStreamRequest, HttpStreamTransport} from "../../src/providers/http-transport.js";
import {ProviderError} from "../../src/providers/provider-error.js";
import {BuiltinProviderAdapter} from "../../src/providers/builtin-provider-adapter.js";
import {parseProviderProfile, type ProviderProfile} from "../../src/providers/provider-profile.js";
import {parseModelProfile, type ModelProfile} from "../../src/providers/model-profile.js";
import type {ModelAdapterEvent} from "../../src/runtime/model-adapter.js";

export const CONFORMANCE_FEATURES = Object.freeze({
  streaming: true,
  native_tool_calls: false,
  parallel_tool_calls: false,
  structured_output: false,
  reasoning_metadata: true,
  usage: true,
  prompt_caching: false,
  image_input: false,
  audio_input: false,
  provider_hosted_tools: false
});

export function createProviderProfileFixture(
  protocol: ApiProtocol,
  providerProfileId = `provider:test/${protocol}`
): ProviderProfile {
  return parseProviderProfile({
    schemaVersion: 1,
    providerProfileId,
    adapterId: "adapter:minimax/builtin",
    displayName: `${protocol} fixture`,
    enabled: true,
    transport: {
      baseUrl: "https://provider.test/v1",
      protocol,
      publicHeaders: {"X-Title": "fixture"},
      allowInsecureLoopback: false
    },
    authentication: {kind: "bearer", envBinding: "TEST_PROVIDER_KEY"}
  });
}

export function createModelProfileFixture(
  providerProfile: ProviderProfile,
  options: {
    readonly modelProfileId?: string;
    readonly features?: Readonly<Record<string, boolean>>;
  } = {}
): ModelProfile {
  return parseModelProfile({
    schemaVersion: 1,
    modelProfileId:
      options.modelProfileId ??
      `model:test/${providerProfile.transport.protocol}/fixture-model`,
    providerProfileId: providerProfile.providerProfileId,
    displayName: "Fixture Model",
    model: "fixture-model",
    enabled: true,
    featureProfile: {
      schemaVersion: 1,
      features: options.features ?? CONFORMANCE_FEATURES,
      contextWindow: 32_000,
      maxOutputTokens: 2_048
    }
  });
}

export function defineBuiltinProviderConformanceSuite(): void {
  for (const protocol of ["responses", "chat_completions"] as const) {
    test(`builtin ${protocol} runtime streams text, usage, and terminal completion`, async () => {
      const raw =
        protocol === "responses"
          ? sse([
              {type: "response.reasoning.delta", delta: "PRIVATE_REASONING"},
              {type: "response.output_text.delta", delta: "visible"},
              {
                type: "response.completed",
                response: {usage: {input_tokens: 7, output_tokens: 2, total_tokens: 9}}
              }
            ])
          : sse(
              [
                {choices: [{delta: {reasoning_content: "PRIVATE_REASONING"}}]},
                {choices: [{delta: {content: "visible"}}]},
                {usage: {prompt_tokens: 7, completion_tokens: 2, total_tokens: 9}}
              ],
              true
            );
      const {runtime, transport} = await runtimeFor(protocol, raw);

      const events = await collect(runtime);

      assert.equal(
        events.filter((event) => event.type === "delta").map((event) => event.delta).join(""),
        "visible"
      );
      assert.equal(events.some((event) => event.type === "usage"), true);
      assert.equal(events.at(-1)?.type, "completed");
      assert.equal(JSON.stringify(events).includes("PRIVATE_REASONING"), false);
      assert.equal(transport.requests.length, 1);
      const request = transport.requests[0]!;
      assert.equal(
        request.url,
        protocol === "responses"
          ? "https://provider.test/v1/responses"
          : "https://provider.test/v1/chat/completions"
      );
      assert.equal(request.headers.Authorization, "Bearer fixture-secret");
      assert.equal(request.headers["X-Title"], "fixture");
      assert.equal(request.body.model, "fixture-model");
      assert.equal(
        protocol === "responses"
          ? Array.isArray(request.body.input)
          : Array.isArray(request.body.messages),
        true
      );
    });

    test(`builtin ${protocol} runtime rejects malformed and premature streams`, async () => {
      const malformed = await runtimeFor(protocol, "data: {broken SECRET_FRAME}\n\n");
      await assert.rejects(
        () => collect(malformed.runtime),
        (error: unknown) =>
          error instanceof ProviderError &&
          error.kind === "protocol" &&
          !error.message.includes("SECRET_FRAME")
      );

      const partialRaw =
        protocol === "responses"
          ? sse([{type: "response.output_text.delta", delta: "partial"}])
          : sse([{choices: [{delta: {content: "partial"}}]}]);
      const partial = await runtimeFor(protocol, partialRaw);
      await assert.rejects(
        () => collect(partial.runtime),
        (error: unknown) => error instanceof ProviderError && error.kind === "protocol"
      );
    });

    test(`builtin ${protocol} runtime normalizes failures without retaining secrets`, async () => {
      const raw =
        protocol === "responses"
          ? sse([
              {
                type: "response.failed",
                response: {
                  error: {code: "rate_limit_exceeded", message: "SECRET_PROVIDER_DETAIL"}
                }
              }
            ])
          : sse(
              [
                {
                  error: {
                    code: "rate_limit_error",
                    message: "SECRET_PROVIDER_DETAIL"
                  }
                }
              ],
              true
            );
      const {runtime} = await runtimeFor(protocol, raw);

      await assert.rejects(
        () => collect(runtime),
        (error: unknown) =>
          error instanceof ProviderError &&
          error.kind === "rate_limit" &&
          !error.message.includes("SECRET_PROVIDER_DETAIL") &&
          !error.message.includes("fixture-secret")
      );
    });

    test(`builtin ${protocol} runtime preserves caller cancellation`, async () => {
      const controller = new AbortController();
      controller.abort();
      const transport = new FixtureTransport(async (request) => {
        if (request.signal?.aborted) {
          throw new DOMException("Aborted", "AbortError");
        }
        return new Response();
      });
      const runtime = await createRuntime(protocol, transport);

      await assert.rejects(
        () => collect(runtime, controller.signal),
        (error: unknown) => error instanceof DOMException && error.name === "AbortError"
      );
    });

    test(`builtin ${protocol} runtime fails closed on unsupported declared features`, async () => {
      const providerProfile = createProviderProfileFixture(protocol);
      const modelProfile = createModelProfileFixture(providerProfile, {
        features: {...CONFORMANCE_FEATURES, structured_output: true}
      });
      const adapter = new BuiltinProviderAdapter(new FixtureTransport(async () => new Response()));

      await assert.rejects(
        () =>
          adapter.createRuntime({
            providerProfile,
            modelProfile,
            credential: {
              targetId: "test-target",
              readSecret: async () => "fixture-secret"
            }
          }),
        /unsupported feature/i
      );
    });
  }
}

class FixtureTransport implements HttpStreamTransport {
  readonly requests: HttpStreamRequest[] = [];

  constructor(
    private readonly respond: (request: HttpStreamRequest) => Promise<Response>
  ) {}

  async postStream(request: HttpStreamRequest): Promise<Response> {
    this.requests.push(request);
    return this.respond(request);
  }
}

async function runtimeFor(
  protocol: ApiProtocol,
  raw: string
): Promise<{runtime: Awaited<ReturnType<typeof createRuntime>>; transport: FixtureTransport}> {
  const transport = new FixtureTransport(
    async () =>
      new Response(raw, {status: 200, headers: {"Content-Type": "text/event-stream"}})
  );
  return {runtime: await createRuntime(protocol, transport), transport};
}

async function createRuntime(protocol: ApiProtocol, transport: HttpStreamTransport) {
  const providerProfile = createProviderProfileFixture(protocol);
  const modelProfile = createModelProfileFixture(providerProfile);
  return new BuiltinProviderAdapter(transport).createRuntime({
    providerProfile,
    modelProfile,
    credential: {
      targetId: "test-target",
      readSecret: async () => "fixture-secret"
    }
  });
}

async function collect(
  runtime: Awaited<ReturnType<typeof createRuntime>>,
  signal?: AbortSignal
): Promise<ModelAdapterEvent[]> {
  const events: ModelAdapterEvent[] = [];
  for await (const event of runtime.stream({
    messages: [{role: "user", content: "hello"}],
    maxOutputTokens: 512,
    ...(signal ? {signal} : {})
  })) {
    events.push(event);
  }
  return events;
}

function sse(events: readonly unknown[], done = false): string {
  return `${events.map((event) => `data: ${JSON.stringify(event)}\n\n`).join("")}${
    done ? "data: [DONE]\n\n" : ""
  }`;
}
