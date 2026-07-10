import assert from "node:assert/strict";
import test from "node:test";
import {createProviderProtocol} from "../src/providers/provider-protocol.js";

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
});
