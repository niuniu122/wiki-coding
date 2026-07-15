import type {CapabilityDescriptor} from "../types.js";
import {tokenizeQuery} from "./query-normalizer.js";

export interface ScoredCapability {
  readonly descriptor: CapabilityDescriptor;
  readonly score: number;
}

interface Document {descriptor: CapabilityDescriptor; tokens: readonly string[]; counts: Map<string, number>}

export class Bm25CapabilityIndex {
  private readonly documents: readonly Document[];
  private readonly documentFrequency = new Map<string, number>();
  private readonly averageLength: number;

  constructor(descriptors: readonly CapabilityDescriptor[]) {
    this.documents = descriptors.filter((item) => item.availability === "available").map((descriptor) => {
      const tokens = tokenizeQuery(descriptor.intentDocument);
      const counts = new Map<string, number>();
      for (const token of tokens) counts.set(token, (counts.get(token) ?? 0) + 1);
      for (const token of counts.keys()) this.documentFrequency.set(token, (this.documentFrequency.get(token) ?? 0) + 1);
      return {descriptor, tokens, counts};
    });
    this.averageLength = this.documents.length
      ? this.documents.reduce((sum, doc) => sum + doc.tokens.length, 0) / this.documents.length
      : 1;
  }

  search(query: string, limit = 5): readonly ScoredCapability[] {
    const queryTokens = tokenizeQuery(query);
    const total = this.documents.length;
    if (!queryTokens.length || !total) return Object.freeze([]);
    const results = this.documents.map((document) => {
      let score = 0;
      for (const token of queryTokens) {
        const frequency = document.counts.get(token) ?? 0;
        if (!frequency) continue;
        const docsWithToken = this.documentFrequency.get(token) ?? 0;
        const idf = Math.log(1 + (total - docsWithToken + 0.5) / (docsWithToken + 0.5));
        const denominator = frequency + 1.2 * (1 - 0.75 + 0.75 * document.tokens.length / this.averageLength);
        score += idf * frequency * 2.2 / denominator;
      }
      return {descriptor: document.descriptor, score};
    }).filter((result) => result.score > 0);
    results.sort((left, right) => right.score - left.score || left.descriptor.id.localeCompare(right.descriptor.id));
    return Object.freeze(results.slice(0, limit));
  }
}
