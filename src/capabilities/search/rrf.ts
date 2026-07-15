export interface RankedId {readonly id: string; readonly score: number}

export function reciprocalRankFusion(rankings: readonly (readonly string[])[], constant = 60): readonly RankedId[] {
  const scores = new Map<string, number>();
  for (const ranking of rankings) ranking.forEach((id, index) => scores.set(id, (scores.get(id) ?? 0) + 1 / (constant + index + 1)));
  return Object.freeze([...scores].map(([id, score]) => ({id, score})).sort((left, right) => right.score - left.score || left.id.localeCompare(right.id)));
}
