import type {EmbeddingProvider, EmbeddingProviderFactory} from "./embedding-provider.js";
import type {EmbeddingResourceLocator, EmbeddingResourceResult} from "./embedding-resource-locator.js";

export class GraniteEmbeddingRuntime {
  private provider: EmbeddingProvider | undefined;
  private resource: EmbeddingResourceResult | undefined;
  constructor(private readonly locator: EmbeddingResourceLocator, private readonly factory: EmbeddingProviderFactory) {}
  async initialize(): Promise<EmbeddingResourceResult> {
    this.resource = await this.locator.locate();
    if (this.resource.status === "ready") {
      try { this.provider = await this.factory.create(this.resource.directory); }
      catch { this.resource = {status: "unavailable", reason: "invalid_manifest"}; }
    }
    return this.resource;
  }
  get available(): boolean { return Boolean(this.provider); }
  async embed(texts: readonly string[], signal?: AbortSignal): Promise<readonly (readonly number[])[]> {
    if (!this.provider) throw new Error("Embedding runtime is unavailable.");
    return this.provider.embed(texts, signal);
  }
  async dispose(): Promise<void> { await this.provider?.dispose(); this.provider = undefined; }
}
