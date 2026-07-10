import assert from "node:assert/strict";
import test from "node:test";
import {DEFAULT_CONFIG} from "../src/config/config-manager.js";
import {MiniMaxModelAdapter} from "../src/runtime/model-adapter.js";

test("the MiniMax adapter forwards an external abort signal without translating it to a timeout", async () => {
  const originalFetch = globalThis.fetch;
  const controller = new AbortController();

  globalThis.fetch = ((_input: URL | RequestInfo, init?: RequestInit) =>
    new Promise<Response>((_resolve, reject) => {
      const signal = init?.signal;
      const fallback = setTimeout(
        () => reject(new Error("fetch was not aborted by the external signal")),
        100
      );
      const rejectAsAborted = (): void => {
        clearTimeout(fallback);
        const error = new Error("request aborted");
        error.name = "AbortError";
        reject(error);
      };
      if (signal?.aborted) {
        rejectAsAborted();
      } else {
        signal?.addEventListener("abort", rejectAsAborted, {once: true});
      }
    })) as typeof fetch;

  try {
    const adapter = new MiniMaxModelAdapter();
    const stream = adapter.streamResponse({
      config: DEFAULT_CONFIG,
      apiKey: "fake-test-key",
      messages: [{role: "user", content: "cancel me"}],
      signal: controller.signal
    });

    const requestDiagnostic = await stream.next();
    assert.equal(requestDiagnostic.value?.type, "diagnostic");
    if (requestDiagnostic.value?.type === "diagnostic") {
      assert.equal(requestDiagnostic.value.code, "provider.request.started");
    }

    const pendingRequest = stream.next();
    controller.abort();

    await assert.rejects(
      pendingRequest,
      (error: unknown) => error instanceof Error && error.name === "AbortError"
    );
  } finally {
    globalThis.fetch = originalFetch;
  }
});
