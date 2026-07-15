import type {CapabilitySnapshot} from "./capability-snapshot.js";
import {HybridCapabilityRetriever} from "./search/hybrid-retriever.js";

export interface CapabilityReportItem {
  readonly id: string;
  readonly name: string;
  readonly source: string;
  readonly status: string;
  readonly safetyClass: string;
  readonly shadowedBy?: string;
}

export interface CapabilitySearchItem extends CapabilityReportItem {
  readonly matchPath: string;
}

export class CapabilityReportService {
  private readonly retriever: HybridCapabilityRetriever;
  constructor(private readonly snapshot: CapabilitySnapshot) {
    this.retriever = new HybridCapabilityRetriever(
      snapshot.entries.map((entry) => entry.descriptor).filter((item) => item.availability === "available")
    );
  }
  list(): {snapshotVersion: string; health: string; mode: "exact+bm25"; capabilities: readonly CapabilityReportItem[]} {
    return Object.freeze({
      snapshotVersion: this.snapshot.version,
      health: this.snapshot.health.status,
      mode: "exact+bm25",
      capabilities: Object.freeze(this.snapshot.entries.map((entry) => Object.freeze({
        id: entry.descriptor.id,
        name: entry.descriptor.name,
        source: `${entry.descriptor.source.kind}:${entry.descriptor.source.scope}`,
        status: entry.descriptor.availability,
        safetyClass: entry.descriptor.safetyClass,
        ...(entry.winnerId ? {shadowedBy: entry.winnerId} : {})
      })))
    });
  }
  async search(query: string): Promise<{snapshotVersion: string; health: string; mode: string; fallbackReason?: string; candidates: readonly CapabilitySearchItem[]}> {
    const result = await this.retriever.retrieve(query);
    return Object.freeze({
      snapshotVersion: this.snapshot.version,
      health: this.snapshot.health.status,
      mode: result.path,
      ...(result.fallbackReason ? {fallbackReason: result.fallbackReason} : {}),
      candidates: Object.freeze(result.descriptors.map((descriptor) => Object.freeze({
        id: descriptor.id,
        name: descriptor.name,
        source: `${descriptor.source.kind}:${descriptor.source.scope}`,
        status: descriptor.availability,
        safetyClass: descriptor.safetyClass,
        matchPath: result.path
      })))
    });
  }
}
