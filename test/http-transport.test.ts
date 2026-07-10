import assert from "node:assert/strict";
import test from "node:test";
import {
  FetchHttpStreamTransport,
  TransportError
} from "../src/providers/http-transport.js";

test("HTTP transport preserves an external abort instead of calling it a timeout", async () => {
  const originalFetch = globalThis.fetch;
  const controller = new AbortController();
  globalThis.fetch = ((_input: URL | RequestInfo, init?: RequestInit) =>
    new Promise<Response>((_resolve, reject) => {
      const abort = (): void => {
        const error = new Error("request aborted");
        error.name = "AbortError";
        reject(error);
      };
      if (init?.signal?.aborted) {
        abort();
      } else {
        init?.signal?.addEventListener("abort", abort, {once: true});
      }
    })) as typeof fetch;

  try {
    const transport = new FetchHttpStreamTransport(1_000);
    const pending = transport.postStream({
      url: "https://provider.test/v1/responses",
      headers: {Authorization: "Bearer fake"},
      body: {},
      signal: controller.signal
    });
    controller.abort();
    await assert.rejects(
      pending,
      (error: unknown) => error instanceof Error && error.name === "AbortError"
    );
  } finally {
    globalThis.fetch = originalFetch;
  }
});

test("HTTP transport classifies its own deadline as a timeout", async () => {
  const originalFetch = globalThis.fetch;
  globalThis.fetch = ((_input: URL | RequestInfo, init?: RequestInit) =>
    new Promise<Response>((_resolve, reject) => {
      init?.signal?.addEventListener(
        "abort",
        () => {
          const error = new Error("deadline abort");
          error.name = "AbortError";
          reject(error);
        },
        {once: true}
      );
    })) as typeof fetch;

  try {
    const transport = new FetchHttpStreamTransport(5);
    await assert.rejects(
      transport.postStream({
        url: "https://provider.test/v1/responses",
        headers: {},
        body: {}
      }),
      (error: unknown) => error instanceof TransportError && error.kind === "timeout"
    );
  } finally {
    globalThis.fetch = originalFetch;
  }
});

test("HTTP transport deadline remains active until the response stream finishes", async () => {
  const originalFetch = globalThis.fetch;
  globalThis.fetch = (async (_input: URL | RequestInfo, init?: RequestInit) => {
    const body = new ReadableStream<Uint8Array>({
      start(controller) {
        const fallback = setTimeout(
          () => controller.error(new Error("response stream was not aborted")),
          100
        );
        init?.signal?.addEventListener(
          "abort",
          () => {
            clearTimeout(fallback);
            const error = new Error("stream deadline abort");
            error.name = "AbortError";
            controller.error(error);
          },
          {once: true}
        );
      }
    });
    return new Response(body, {status: 200});
  }) as typeof fetch;

  try {
    const transport = new FetchHttpStreamTransport(5);
    const response = await transport.postStream({
      url: "https://provider.test/v1/responses",
      headers: {},
      body: {}
    });
    assert.ok(response.body);
    await assert.rejects(
      response.body.getReader().read(),
      (error: unknown) => error instanceof TransportError && error.kind === "timeout"
    );
  } finally {
    globalThis.fetch = originalFetch;
  }
});
