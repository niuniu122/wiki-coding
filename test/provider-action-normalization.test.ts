import assert from "node:assert/strict";
import test from "node:test";
import {StrictProviderGateway} from "../src/providers/provider-gateway.js";
import type {HttpStreamRequest, HttpStreamTransport} from "../src/providers/http-transport.js";
import {createModelProfileFixture, createProviderProfileFixture} from "./support/provider-conformance-suite.js";

const TOOL = Object.freeze({
  name: "read_file",
  description: "Read a workspace file",
  inputSchema: {type: "object", properties: {path: {type: "string"}}, required: ["path"], additionalProperties: false}
});

for (const protocol of ["responses", "chat_completions"] as const) {
  test(`${protocol} normalizes fragmented tool calls without exposing raw frames`, async () => {
    const frames = protocol === "responses" ? [
      {type: "response.output_item.added", item: {type: "function_call", id: "item-1", call_id: "call-1", name: "read_file"}},
      {type: "response.function_call_arguments.delta", item_id: "call-1", delta: "{\"path\":\"README"},
      {type: "response.function_call_arguments.delta", item_id: "call-1", delta: ".md\"}"},
      {type: "response.completed", response: {usage: {input_tokens: 1, output_tokens: 2}}}
    ] : [
      {choices: [{delta: {tool_calls: [{index: 0, id: "call-1", function: {name: "read_file", arguments: "{\"path\":\"README"}}]}}]},
      {choices: [{delta: {tool_calls: [{index: 0, function: {arguments: ".md\"}"}}]}}]},
      "[DONE]"
    ];
    const transport = new StaticTransport(sseResponse(frames));
    const providerProfile = createProviderProfileFixture(protocol);
    const modelProfile = createModelProfileFixture(providerProfile, {
      features: {
        streaming: true, native_tool_calls: true, parallel_tool_calls: true,
        structured_output: false, reasoning_metadata: true, usage: true,
        prompt_caching: false, image_input: false, audio_input: false,
        provider_hosted_tools: false
      }
    });
    const events = [];
    for await (const event of new StrictProviderGateway(transport).streamProfile({
      providerProfile, modelProfile, apiKey: "fixture-secret", messages: [{role: "user", content: "read"}], maxOutputTokens: 100, tools: [TOOL]
    })) events.push(event);

    assert.deepEqual(events.find((event) => event.type === "tool.call"), {
      type: "tool.call",
      call: {callId: "call-1", name: "read_file", argumentsJson: "{\"path\":\"README.md\"}"}
    });
    assert.doesNotMatch(JSON.stringify(events), /fixture-secret|response\.function_call_arguments/);
    assert.match(JSON.stringify(transport.request?.body), /read_file/);
  });
}

class StaticTransport implements HttpStreamTransport {
  request: HttpStreamRequest | undefined;
  constructor(private readonly response: Response) {}
  async postStream(request: HttpStreamRequest): Promise<Response> { this.request = request; return this.response; }
}

function sseResponse(frames: readonly (Record<string, unknown> | string)[]): Response {
  const text = frames.map((frame) => `data: ${typeof frame === "string" ? frame : JSON.stringify(frame)}\n\n`).join("");
  return new Response(text, {status: 200, headers: {"Content-Type": "text/event-stream"}});
}
