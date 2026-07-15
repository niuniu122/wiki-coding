export const GRANITE_EMBEDDING_MODEL_ID = "ibm-granite/granite-embedding-97m-multilingual-r2";
export const GRANITE_RESOURCE_PACKAGE_ID = "@minimax-codex/embedding-granite-97m-r2-avx2";

export interface EmbeddingResourceManifest {
  readonly schemaVersion: 1;
  readonly packageId: typeof GRANITE_RESOURCE_PACKAGE_ID;
  readonly modelId: typeof GRANITE_EMBEDDING_MODEL_ID;
  readonly modelRevision: string;
  readonly runtimeAbi: string;
  readonly architecture: "x64-avx2";
  readonly quantization: "qint8";
  readonly license: string;
  readonly tokenizerVersion: string;
  readonly files: Readonly<Record<string, string>>;
}

export function parseEmbeddingResourceManifest(value: unknown): EmbeddingResourceManifest {
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error("Invalid embedding resource manifest.");
  const item = value as Record<string, unknown>;
  const allowed = new Set(["schemaVersion", "packageId", "modelId", "modelRevision", "runtimeAbi", "architecture", "quantization", "license", "tokenizerVersion", "files"]);
  if (Object.keys(item).some((key) => !allowed.has(key)) || item.schemaVersion !== 1 ||
    item.packageId !== GRANITE_RESOURCE_PACKAGE_ID || item.modelId !== GRANITE_EMBEDDING_MODEL_ID ||
    item.architecture !== "x64-avx2" || item.quantization !== "qint8") {
    throw new Error("Invalid embedding resource manifest.");
  }
  for (const key of ["modelRevision", "runtimeAbi", "license", "tokenizerVersion"] as const) {
    if (typeof item[key] !== "string" || !(item[key] as string).trim()) throw new Error("Invalid embedding resource manifest.");
  }
  if (!item.files || typeof item.files !== "object" || Array.isArray(item.files)) throw new Error("Invalid embedding resource manifest.");
  for (const [name, hash] of Object.entries(item.files as Record<string, unknown>)) {
    if (!name || name.includes("..") || name.includes("\\") || name.startsWith("/") || typeof hash !== "string" || !/^[a-f0-9]{64}$/.test(hash)) {
      throw new Error("Invalid embedding resource manifest.");
    }
  }
  return Object.freeze(item) as unknown as EmbeddingResourceManifest;
}
