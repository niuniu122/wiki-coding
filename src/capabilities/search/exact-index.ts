import type {CapabilityDescriptor} from "../types.js";
import {normalizeQuery} from "./query-normalizer.js";

export class ExactCapabilityIndex {
  private readonly values = new Map<string, CapabilityDescriptor>();
  constructor(descriptors: readonly CapabilityDescriptor[]) {
    for (const descriptor of descriptors.filter((item) => item.availability === "available")) {
      for (const key of [descriptor.id, descriptor.name, ...descriptor.aliases, ...descriptor.commands]) {
        const normalized = normalizeQuery(key);
        if (normalized && !this.values.has(normalized)) this.values.set(normalized, descriptor);
      }
    }
  }
  resolve(query: string): CapabilityDescriptor | undefined {
    return this.values.get(normalizeQuery(query));
  }
}
