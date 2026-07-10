export interface HttpStreamRequest {
  url: string;
  headers: Record<string, string>;
  body: Record<string, unknown>;
  signal?: AbortSignal;
}

export interface HttpStreamTransport {
  postStream(request: HttpStreamRequest): Promise<Response>;
}

export type TransportErrorKind = "timeout" | "network";

export class TransportError extends Error {
  constructor(
    readonly kind: TransportErrorKind,
    message: string,
    options?: ErrorOptions
  ) {
    super(message, options);
    this.name = "TransportError";
  }
}

export class FetchHttpStreamTransport implements HttpStreamTransport {
  constructor(private readonly timeoutMs = 300_000) {}

  async postStream(request: HttpStreamRequest): Promise<Response> {
    const controller = new AbortController();
    let didTimeout = false;
    let cleanedUp = false;
    const forwardAbort = (): void => controller.abort(request.signal?.reason);

    if (request.signal?.aborted) {
      forwardAbort();
    } else {
      request.signal?.addEventListener("abort", forwardAbort, {once: true});
    }

    const timeout = setTimeout(() => {
      didTimeout = true;
      controller.abort();
    }, this.timeoutMs);
    const cleanup = (): void => {
      if (cleanedUp) {
        return;
      }
      cleanedUp = true;
      clearTimeout(timeout);
      request.signal?.removeEventListener("abort", forwardAbort);
    };

    try {
      const response = await fetch(request.url, {
        method: "POST",
        headers: request.headers,
        body: JSON.stringify(request.body),
        signal: controller.signal
      });
      if (!response.body) {
        cleanup();
        return response;
      }
      return wrapResponseStream(response, {
        ...(request.signal ? {signal: request.signal} : {}),
        timeoutMs: this.timeoutMs,
        didTimeout: () => didTimeout,
        cleanup
      });
    } catch (error) {
      cleanup();
      throw classifyTransportFailure(error, request.signal, didTimeout, this.timeoutMs);
    }
  }
}

function wrapResponseStream(
  response: Response,
  lifecycle: {
    signal?: AbortSignal;
    timeoutMs: number;
    didTimeout(): boolean;
    cleanup(): void;
  }
): Response {
  const reader = response.body!.getReader();
  const body = new ReadableStream<Uint8Array>({
    async pull(controller) {
      try {
        const {done, value} = await reader.read();
        if (done) {
          lifecycle.cleanup();
          controller.close();
          return;
        }
        controller.enqueue(value);
      } catch (error) {
        lifecycle.cleanup();
        controller.error(
          classifyTransportFailure(
            error,
            lifecycle.signal,
            lifecycle.didTimeout(),
            lifecycle.timeoutMs
          )
        );
      }
    },
    async cancel(reason) {
      lifecycle.cleanup();
      await reader.cancel(reason);
    }
  });

  return new Response(body, {
    status: response.status,
    statusText: response.statusText,
    headers: response.headers
  });
}

function classifyTransportFailure(
  error: unknown,
  externalSignal: AbortSignal | undefined,
  didTimeout: boolean,
  timeoutMs: number
): unknown {
  if (isAbortError(error)) {
    if (externalSignal?.aborted) {
      return error;
    }
    if (didTimeout) {
      return new TransportError(
        "timeout",
        `Provider request exceeded the ${timeoutMs}ms deadline.`,
        {cause: error}
      );
    }
  }
  return new TransportError("network", "Provider request failed before the stream completed.", {
    cause: error
  });
}

function isAbortError(error: unknown): boolean {
  return error instanceof DOMException
    ? error.name === "AbortError"
    : error instanceof Error && error.name === "AbortError";
}
