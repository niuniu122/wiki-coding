import type {EmbeddingProvider} from "../embedding/embedding-provider.js";
import type {CapabilityDescriptor} from "../types.js";
import {buildCapabilityCards, type CapabilityCard} from "./capability-card.js";
import {LexicalCapabilityRetriever} from "./lexical-retriever.js";
import {reciprocalRankFusion} from "./rrf.js";
import {CapabilityVectorIndex} from "./vector-index.js";

export interface HybridRetrievalResult {
  readonly path: "exact" | "lexical" | "fused" | "none";
  readonly descriptors: readonly CapabilityDescriptor[];
  readonly cards: readonly CapabilityCard[];
  readonly fallbackReason?: "embedding_unavailable" | "embedding_timeout";
  readonly confident: boolean;
}

export class HybridCapabilityRetriever {
  private readonly lexical: LexicalCapabilityRetriever;
  private readonly byId: Map<string, CapabilityDescriptor>;
  constructor(
    descriptors: readonly CapabilityDescriptor[],
    private readonly embedding?: {provider: EmbeddingProvider; vectorIndex: CapabilityVectorIndex; deadlineMs?: number}
  ) {
    this.lexical = new LexicalCapabilityRetriever(descriptors);
    this.byId = new Map(descriptors.map((descriptor) => [descriptor.id, descriptor]));
  }

  async retrieve(query: string, inputBudgetTokens = 24_000): Promise<HybridRetrievalResult> {
    const lexical = this.lexical.retrieve(query, 5);
    if (lexical.path === "exact") return result("exact", lexical.results.map((item) => item.descriptor), inputBudgetTokens, undefined, true);
    if (!this.embedding) return result(lexical.results.length ? "lexical" : "none", lexical.results.map((item) => item.descriptor), inputBudgetTokens, "embedding_unavailable", lexical.results[0]?.score !== undefined && lexical.results[0].score >= 0.35);
    let vectorIds: string[];
    try {
      const vectors = await withDeadline(this.embedding.provider.embed([query]), this.embedding.deadlineMs ?? 150);
      vectorIds = this.embedding.vectorIndex.search(vectors[0] ?? []).map((item) => item.descriptor.id);
    } catch {
      return result(lexical.results.length ? "lexical" : "none", lexical.results.map((item) => item.descriptor), inputBudgetTokens, "embedding_timeout", lexical.results[0]?.score !== undefined && lexical.results[0].score >= 0.35);
    }
    const lexicalIds = lexical.results.map((item) => item.descriptor.id);
    const fused = reciprocalRankFusion([lexicalIds, vectorIds]).slice(0, 5).map((item) => this.byId.get(item.id)).filter((item): item is CapabilityDescriptor => Boolean(item));
    const overlap = lexicalIds.length === 0 || vectorIds.length === 0 ? 0 : lexicalIds.filter((id) => vectorIds.includes(id)).length;
    return result(fused.length ? "fused" : "none", fused, inputBudgetTokens, undefined, overlap > 0 || fused.length === 1);
  }
}

function result(path: HybridRetrievalResult["path"], descriptors: readonly CapabilityDescriptor[], budget: number, fallbackReason: HybridRetrievalResult["fallbackReason"], confident: boolean): HybridRetrievalResult {
  return Object.freeze({path, descriptors: Object.freeze([...descriptors].slice(0, 5)), cards: buildCapabilityCards(descriptors, budget), ...(fallbackReason ? {fallbackReason} : {}), confident});
}

async function withDeadline<T>(promise: Promise<T>, deadlineMs: number): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([promise, new Promise<T>((_, reject) => { timer = setTimeout(() => reject(new Error("deadline")), deadlineMs); })]);
  } finally { if (timer) clearTimeout(timer); }
}
