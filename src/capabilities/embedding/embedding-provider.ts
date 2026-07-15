export interface EmbeddingProvider {
  readonly dimensions: number;
  embed(texts: readonly string[], signal?: AbortSignal): Promise<readonly (readonly number[])[]>;
  dispose(): Promise<void>;
}

export interface EmbeddingProviderFactory {
  create(resourceDirectory: string): Promise<EmbeddingProvider>;
}
