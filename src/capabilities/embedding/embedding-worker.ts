import type {EmbeddingProvider} from "./embedding-provider.js";

export class DeadlineEmbeddingProvider implements EmbeddingProvider {
  readonly dimensions: number;
  constructor(private readonly inner: EmbeddingProvider, private readonly deadlineMs = 150) { this.dimensions = inner.dimensions; }
  async embed(texts: readonly string[], signal?: AbortSignal): Promise<readonly (readonly number[])[]> {
    const controller = new AbortController();
    const onAbort = () => controller.abort(signal?.reason);
    signal?.addEventListener("abort", onAbort, {once: true});
    const timer = setTimeout(() => controller.abort(new Error("Embedding deadline exceeded.")), this.deadlineMs);
    try { return await this.inner.embed(texts, controller.signal); }
    finally { clearTimeout(timer); signal?.removeEventListener("abort", onAbort); }
  }
  dispose(): Promise<void> { return this.inner.dispose(); }
}
