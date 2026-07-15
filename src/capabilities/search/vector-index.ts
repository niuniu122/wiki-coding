import type {CapabilityDescriptor} from "../types.js";

export interface VectorSearchResult {readonly descriptor: CapabilityDescriptor; readonly score: number}

export class CapabilityVectorIndex {
  constructor(private readonly entries: readonly {descriptor: CapabilityDescriptor; vector: readonly number[]}[]) {}
  search(query: readonly number[], limit = 5): readonly VectorSearchResult[] {
    const results = this.entries.map((entry) => ({descriptor: entry.descriptor, score: cosine(query, entry.vector)}))
      .filter((entry) => Number.isFinite(entry.score) && entry.score > 0)
      .sort((left, right) => right.score - left.score || left.descriptor.id.localeCompare(right.descriptor.id));
    return Object.freeze(results.slice(0, limit));
  }
}

function cosine(left: readonly number[], right: readonly number[]): number {
  if (left.length !== right.length || !left.length) return 0;
  let dot = 0, leftNorm = 0, rightNorm = 0;
  for (let index = 0; index < left.length; index += 1) {
    const a = left[index] ?? 0, b = right[index] ?? 0;
    dot += a * b; leftNorm += a * a; rightNorm += b * b;
  }
  return leftNorm && rightNorm ? dot / Math.sqrt(leftNorm * rightNorm) : 0;
}
