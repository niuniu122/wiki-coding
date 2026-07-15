import type {CapabilityDescriptor} from "../types.js";
import {Bm25CapabilityIndex, type ScoredCapability} from "./bm25-index.js";
import {ExactCapabilityIndex} from "./exact-index.js";

export interface LexicalRetrievalResult {
  readonly path: "exact" | "bm25" | "none";
  readonly results: readonly ScoredCapability[];
}

export class LexicalCapabilityRetriever {
  private readonly exact: ExactCapabilityIndex;
  private readonly bm25: Bm25CapabilityIndex;
  constructor(descriptors: readonly CapabilityDescriptor[]) {
    this.exact = new ExactCapabilityIndex(descriptors);
    this.bm25 = new Bm25CapabilityIndex(descriptors);
  }
  retrieve(query: string, limit = 5): LexicalRetrievalResult {
    const exact = this.exact.resolve(query);
    if (exact) return Object.freeze({path: "exact", results: Object.freeze([{descriptor: exact, score: Number.POSITIVE_INFINITY}])});
    const results = this.bm25.search(query, limit);
    return Object.freeze({path: results.length ? "bm25" : "none", results});
  }
}
