import type {CapabilityDescriptor} from "../types.js";
import {normalizeQuery} from "./query-normalizer.js";

export class CapabilityFacetIndex {
  private readonly index = new Map<string, Set<string>>();
  constructor(descriptors: readonly CapabilityDescriptor[]) {
    for (const descriptor of descriptors.filter((item) => item.availability === "available")) {
      for (const [kind, values] of Object.entries(descriptor.facets)) {
        for (const value of values) {
          const key = `${kind}:${normalizeQuery(value)}`;
          const ids = this.index.get(key) ?? new Set<string>();
          ids.add(descriptor.id);
          this.index.set(key, ids);
        }
      }
    }
  }
  filter(kind: "domain" | "action" | "object", value: string): readonly string[] {
    return Object.freeze([...(this.index.get(`${kind}:${normalizeQuery(value)}`) ?? [])].sort());
  }
}
