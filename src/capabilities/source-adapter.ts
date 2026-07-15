import type {CapabilityDescriptor, CapabilityIssue, CapabilitySourceKind} from "./types.js";

export interface CapabilitySourceResult {
  readonly descriptors: readonly CapabilityDescriptor[];
  readonly issues: readonly CapabilityIssue[];
}

export interface CapabilitySourceAdapter {
  readonly kind: CapabilitySourceKind;
  scan(): Promise<CapabilitySourceResult>;
}
