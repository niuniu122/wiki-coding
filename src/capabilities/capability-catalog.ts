import {capabilitySourcePrecedence} from "./source-precedence.js";
import type {CapabilityDescriptor} from "./types.js";

export interface CapabilityCatalogEntry {
  readonly descriptor: CapabilityDescriptor;
  readonly winnerId?: string;
}

export class CapabilityCatalog {
  private constructor(private readonly catalogEntries: readonly CapabilityCatalogEntry[]) {}

  static build(descriptors: readonly CapabilityDescriptor[]): CapabilityCatalog {
    const grouped = new Map<string, CapabilityDescriptor[]>();
    for (const descriptor of descriptors) {
      const key = normalizeCapabilityId(descriptor.id);
      const values = grouped.get(key) ?? [];
      values.push(descriptor);
      grouped.set(key, values);
    }
    const entries: CapabilityCatalogEntry[] = [];
    for (const candidates of grouped.values()) {
      candidates.sort((left, right) =>
        capabilitySourcePrecedence(right.source.scope) -
          capabilitySourcePrecedence(left.source.scope) ||
        left.source.file.localeCompare(right.source.file)
      );
      const builtin = candidates.find((candidate) => candidate.source.scope === "builtin");
      const winner = builtin ?? candidates[0]!;
      entries.push({descriptor: winner});
      for (const loser of candidates.filter((candidate) => candidate !== winner)) {
        entries.push({
          descriptor: Object.freeze({...loser, availability: "shadowed"}),
          winnerId: winner.id
        });
      }
    }
    entries.sort((left, right) =>
      left.descriptor.id.localeCompare(right.descriptor.id) ||
      left.descriptor.source.file.localeCompare(right.descriptor.source.file)
    );
    return new CapabilityCatalog(Object.freeze(entries));
  }

  entries(): readonly CapabilityCatalogEntry[] {
    return this.catalogEntries;
  }

  candidates(): readonly CapabilityDescriptor[] {
    return Object.freeze(
      this.catalogEntries
        .map((entry) => entry.descriptor)
        .filter((descriptor) => descriptor.availability === "available")
    );
  }

  get(id: string): CapabilityDescriptor | undefined {
    const normalized = normalizeCapabilityId(id);
    return this.candidates().find((descriptor) => normalizeCapabilityId(descriptor.id) === normalized);
  }
}

export function normalizeCapabilityId(id: string): string {
  return id.normalize("NFC").toLocaleLowerCase("en-US");
}
