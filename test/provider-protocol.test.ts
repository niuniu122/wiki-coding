import assert from "node:assert/strict";
import test from "node:test";
import {createProviderProtocol} from "../src/providers/provider-protocol.js";
import type {ModelContextMessage} from "../src/types.js";

test("responses protocol owns its request shape and separates reasoning from visible text", () => {
  const protocol = createProviderProtocol("responses");
  const request = protocol.buildRequest({
    model: "MiniMax-M3",
    messages: [{role: "user", content: "hello"}],
    maxOutputTokens: 512
  });

  assert.equal(protocol.path, "/responses");
  assert.deepEqual(request, {
    model: "MiniMax-M3",
    input: [{role: "user", content: "hello"}],
    stream: true,
    max_output_tokens: 512,
    metadata: {prompt_cache_key: "minimax-codex-v1"}
  });
  assert.deepEqual(
    protocol.parseEvent(
      JSON.stringify({type: "response.output_text.delta", delta: "visible"})
    ),
    {type: "delta", delta: "visible"}
  );
  assert.deepEqual(
    protocol.parseEvent(
      JSON.stringify({type: "response.reasoning.delta", delta: "private reasoning"})
    ),
    {type: "reasoning", content: "private reasoning"}
  );
  assert.deepEqual(
    protocol.parseEvent(JSON.stringify({type: "response.completed"})),
    {type: "completed"}
  );
  assert.deepEqual(
    protocol.parseEvent(JSON.stringify({type: "response.unknown"})),
    {type: "ignored"}
  );
});

test("chat completions protocol owns chat request and usage parsing", () => {
  const protocol = createProviderProtocol("chat_completions");
  const request = protocol.buildRequest({
    model: "MiniMax-M3",
    messages: [{role: "user", content: "hello"}],
    maxOutputTokens: 256
  });

  assert.equal(protocol.path, "/chat/completions");
  assert.deepEqual(request, {
    model: "MiniMax-M3",
    messages: [{role: "user", content: "hello"}],
    stream: true,
    stream_options: {include_usage: true},
    max_tokens: 256
  });
  assert.deepEqual(
    protocol.parseEvent(
      JSON.stringify({usage: {prompt_tokens: 10, completion_tokens: 3, total_tokens: 13}})
    ),
    {type: "usage", inputTokens: 10, outputTokens: 3, totalTokens: 13}
  );
  assert.deepEqual(protocol.parseEvent("[DONE]"), {type: "completed"});
  assert.deepEqual(protocol.parseEvent(JSON.stringify({choices: []})), {type: "ignored"});
});

test("tool exchanges preserve call identity in both Provider protocols", () => {
  const messages: ModelContextMessage[] = [
    {role: "user", content: "read package.json"},
    {
      role: "assistant",
      content: "",
      toolCalls: [{
        callId: "call-1",
        name: "invoke_local_capability",
        argumentsJson: "{\"path\":\"package.json\"}"
      }]
    },
    {role: "tool", toolCallId: "call-1", content: "file contents"}
  ];

  const responses = createProviderProtocol("responses").buildRequest({
    model: "MiniMax-M3",
    messages,
    maxOutputTokens: 128
  });
  assert.deepEqual(responses.input, [
    {role: "user", content: "read package.json"},
    {
      type: "function_call",
      call_id: "call-1",
      name: "invoke_local_capability",
      arguments: "{\"path\":\"package.json\"}"
    },
    {type: "function_call_output", call_id: "call-1", output: "file contents"}
  ]);

  const chat = createProviderProtocol("chat_completions").buildRequest({
    model: "deepseek-v4-flash",
    messages,
    maxOutputTokens: 128
  });
  assert.deepEqual(chat.messages, [
    {role: "user", content: "read package.json"},
    {
      role: "assistant",
      content: null,
      tool_calls: [{
        id: "call-1",
        type: "function",
        function: {
          name: "invoke_local_capability",
          arguments: "{\"path\":\"package.json\"}"
        }
      }]
    },
    {role: "tool", tool_call_id: "call-1", content: "file contents"}
  ]);
});

test("malformed provider JSON is rejected without exposing the frame", () => {
  const protocol = createProviderProtocol("responses");
  const secretFrame = "{not-json SECRET_FRAME_CONTENT}";

  assert.throws(
    () => protocol.parseEvent(secretFrame),
    (error: unknown) =>
      error instanceof Error &&
      /malformed provider event/i.test(error.message) &&
      !error.message.includes("SECRET_FRAME_CONTENT")
  );
});

test("provider-declared failures are normalized without retaining raw details", () => {
  const responses = createProviderProtocol("responses");
  const chat = createProviderProtocol("chat_completions");

  assert.deepEqual(
    responses.parseEvent(
      JSON.stringify({
        type: "response.failed",
        response: {error: {code: "rate_limit_exceeded", message: "SECRET_PROMPT"}}
      })
    ),
    {type: "failed", code: "rate_limit", category: "rate_limit"}
  );
  assert.deepEqual(
    responses.parseEvent(
      JSON.stringify({
        type: "response.incomplete",
        response: {incomplete_details: {reason: "max_output_tokens"}}
      })
    ),
    {type: "failed", code: "response_incomplete", category: "protocol"}
  );
  assert.deepEqual(
    chat.parseEvent(
      JSON.stringify({error: {code: "invalid_request_error", message: "SECRET_KEY"}})
    ),
    {type: "failed", code: "invalid_request", category: "request"}
  );
});

test("protocol factory fails closed for an unknown runtime protocol", () => {
  assert.throws(
    () => createProviderProtocol("future_protocol" as never),
    /unsupported provider protocol/i
  );
});
