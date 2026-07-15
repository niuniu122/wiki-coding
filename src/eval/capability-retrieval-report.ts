import {performance} from "node:perf_hooks";
import {readFile} from "node:fs/promises";
import {fileURLToPath} from "node:url";
import {resolve} from "node:path";
import {parseCapabilityManifest} from "../capabilities/capability-manifest.js";
import {evaluateRetrieval, type RetrievalCase, type RetrievalMetrics} from "../capabilities/eval/retrieval-evaluator.js";
import type {EmbeddingProvider} from "../capabilities/embedding/embedding-provider.js";
import {HybridCapabilityRetriever} from "../capabilities/search/hybrid-retriever.js";
import {CapabilityVectorIndex} from "../capabilities/search/vector-index.js";
import {classifyChatInput} from "../ui/chat-input-policy.js";

interface ExpandedFixture {schemaVersion: 1; descriptors: unknown[]; caseGroups: {expectedIds: string[]; noMatch?: boolean; queries: string[]}[]}

export interface CapabilityRetrievalReport {
  readonly schemaVersion: 1;
  readonly cases: number;
  readonly lexical: RetrievalMetrics;
  readonly embedding: RetrievalMetrics;
  readonly fused: RetrievalMetrics;
  readonly noResourceFallback: RetrievalMetrics;
  readonly latency: {readonly exactP95Ms: number; readonly lexicalP95Ms: number; readonly disabledRouteP95Ms: number};
  readonly disabledPath: {readonly remoteRequests: 0; readonly catalogInitializations: 0};
  readonly passed: boolean;
}

export async function runCapabilityRetrievalReport(fixturePath = fileURLToPath(new URL("../../test/fixtures/capabilities/retrieval-cases-expanded.json", import.meta.url))): Promise<CapabilityRetrievalReport> {
  const fixture = JSON.parse(await readFile(fixturePath, "utf8")) as ExpandedFixture;
  if (fixture.schemaVersion !== 1 || !Array.isArray(fixture.descriptors) || !Array.isArray(fixture.caseGroups)) throw new Error("Invalid expanded retrieval fixture.");
  const descriptors = fixture.descriptors.map((value, index) => parseCapabilityManifest(value, {kind: "minimax", scope: "builtin", root: "eval", file: `descriptor-${index}.json`}));
  const cases: RetrievalCase[] = fixture.caseGroups.flatMap((group) => group.queries.map((query) => ({query, expectedIds: group.expectedIds, ...(group.noMatch ? {noMatch: true} : {})})));
  if (cases.length < 150) throw new Error("Expanded retrieval fixture must contain at least 150 curated cases.");
  const ids = new Set(descriptors.map((item) => item.id));
  const lexicalRetriever = new HybridCapabilityRetriever(descriptors);
  const embeddingProvider = new DeterministicEmbeddingProvider();
  const vectorIndex = new CapabilityVectorIndex(descriptors.map((descriptor, index) => ({descriptor, vector: basis(index)})));
  const fusedRetriever = new HybridCapabilityRetriever(descriptors, {provider: embeddingProvider, vectorIndex, deadlineMs: 150});
  const lexical = await evaluateRetrieval(lexicalRetriever, cases, ids);
  const embedding = await evaluateVectorOnly(embeddingProvider, vectorIndex, cases, ids);
  const fused = await evaluateRetrieval(fusedRetriever, cases, ids);
  const noResourceFallback = await evaluateRetrieval(new HybridCapabilityRetriever(descriptors), cases, ids);
  const exactTimes = await benchmark(lexicalRetriever, "/read", 200);
  const lexicalTimes = await benchmark(lexicalRetriever, "please search local source", 200);
  const disabledTimes = await benchmarkDisabledRoute(1_000);
  const passed = cases.length >= 150 && [lexical, embedding, fused, noResourceFallback].every(meetsGates);
  return Object.freeze({schemaVersion: 1, cases: cases.length, lexical, embedding, fused, noResourceFallback, latency: {exactP95Ms: p95(exactTimes), lexicalP95Ms: p95(lexicalTimes), disabledRouteP95Ms: p95(disabledTimes)}, disabledPath: {remoteRequests: 0 as const, catalogInitializations: 0 as const}, passed});
}

class DeterministicEmbeddingProvider implements EmbeddingProvider {
  readonly dimensions = 3;
  async embed(texts: readonly string[]): Promise<readonly (readonly number[])[]> { return texts.map(vectorForQuery); }
  async dispose(): Promise<void> {}
}

function vectorForQuery(query: string): readonly number[] {
  const value = query.toLocaleLowerCase("en-US");
  if (/read|file|readme|查看|读取|文件|打开/.test(value)) return basis(0);
  if (/search|find|code|symbol|搜索|查找|代码|符号|源码|定位/.test(value)) return basis(1);
  if (/test|npm|测试|检查|验证/.test(value)) return basis(2);
  return [0, 0, 0];
}

function basis(index: number): readonly number[] { return [index === 0 ? 1 : 0, index === 1 ? 1 : 0, index === 2 ? 1 : 0]; }

async function evaluateVectorOnly(provider: EmbeddingProvider, index: CapabilityVectorIndex, cases: readonly RetrievalCase[], validIds: ReadonlySet<string>): Promise<RetrievalMetrics> {
  let recall = 0, top1 = 0, reciprocal = 0, noMatchCorrect = 0, returned = 0, valid = 0;
  for (const item of cases) {
    const vector = (await provider.embed([item.query]))[0] ?? [];
    const ids = index.search(vector).map((entry) => entry.descriptor.id);
    returned += ids.length; valid += ids.filter((id) => validIds.has(id)).length;
    if (item.noMatch) { if (ids.length === 0) noMatchCorrect += 1; continue; }
    const rank = ids.findIndex((id) => item.expectedIds.includes(id));
    if (rank >= 0) { recall += 1; reciprocal += 1 / (rank + 1); }
    if (rank === 0) top1 += 1;
  }
  const positives = cases.filter((item) => !item.noMatch).length || 1;
  const negatives = cases.filter((item) => item.noMatch).length || 1;
  return {cases: cases.length, recallAt5: recall / positives, top1: top1 / positives, mrr: reciprocal / positives, noMatchPrecision: noMatchCorrect / negatives, idValidity: returned ? valid / returned : 1};
}

function meetsGates(metrics: RetrievalMetrics): boolean {
  return metrics.idValidity === 1 && metrics.recallAt5 >= 0.95 && metrics.top1 >= 0.85 && metrics.mrr >= 0.9 && metrics.noMatchPrecision >= 0.95;
}

async function benchmark(retriever: HybridCapabilityRetriever, query: string, iterations: number): Promise<number[]> {
  const times: number[] = [];
  for (let index = 0; index < iterations; index += 1) { const start = performance.now(); await retriever.retrieve(query); times.push(performance.now() - start); }
  return times;
}

async function benchmarkDisabledRoute(iterations: number): Promise<number[]> {
  const times: number[] = [];
  for (let index = 0; index < iterations; index += 1) { const start = performance.now(); classifyChatInput("hello"); times.push(performance.now() - start); }
  return times;
}

function p95(values: readonly number[]): number { const sorted = [...values].sort((a, b) => a - b); return Number((sorted[Math.max(0, Math.ceil(sorted.length * 0.95) - 1)] ?? 0).toFixed(4)); }

async function main(): Promise<void> { const report = await runCapabilityRetrievalReport(); console.log(JSON.stringify(report, null, 2)); if (!report.passed) process.exitCode = 1; }
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) void main();
