import type {HybridCapabilityRetriever} from "../search/hybrid-retriever.js";

export interface RetrievalCase {readonly query: string; readonly expectedIds: readonly string[]; readonly noMatch?: boolean}
export interface RetrievalMetrics {readonly cases: number; readonly recallAt5: number; readonly top1: number; readonly mrr: number; readonly noMatchPrecision: number; readonly idValidity: number}

export async function evaluateRetrieval(retriever: HybridCapabilityRetriever, cases: readonly RetrievalCase[], validIds: ReadonlySet<string>): Promise<RetrievalMetrics> {
  let recall = 0, top1 = 0, reciprocal = 0, noMatchCorrect = 0, noMatchReturned = 0, valid = 0, returned = 0;
  for (const item of cases) {
    const result = await retriever.retrieve(item.query);
    const ids = result.descriptors.map((descriptor) => descriptor.id);
    returned += ids.length; valid += ids.filter((id) => validIds.has(id)).length;
    if (item.noMatch) { noMatchReturned += result.confident ? 0 : 1; noMatchCorrect += result.confident ? 0 : 1; continue; }
    const rank = ids.findIndex((id) => item.expectedIds.includes(id));
    if (rank >= 0) { recall += 1; reciprocal += 1 / (rank + 1); }
    if (rank === 0) top1 += 1;
  }
  const positives = cases.filter((item) => !item.noMatch).length || 1;
  return Object.freeze({cases: cases.length, recallAt5: recall / positives, top1: top1 / positives, mrr: reciprocal / positives, noMatchPrecision: noMatchReturned ? noMatchCorrect / noMatchReturned : 1, idValidity: returned ? valid / returned : 1});
}
