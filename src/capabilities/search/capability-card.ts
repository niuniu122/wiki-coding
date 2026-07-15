import type {CapabilityDescriptor} from "../types.js";

export interface CapabilityCard {
  readonly id: string;
  readonly name: string;
  readonly description: string;
  readonly safetyClass: string;
  readonly executionKind: string;
}

export function buildCapabilityCards(descriptors: readonly CapabilityDescriptor[], inputBudgetTokens: number): readonly CapabilityCard[] {
  const hardLimit = Math.min(1200, Math.max(0, Math.floor(inputBudgetTokens * 0.05)));
  const cards: CapabilityCard[] = [];
  let used = 0;
  for (const descriptor of descriptors.slice(0, 5)) {
    const card = Object.freeze({id: descriptor.id, name: descriptor.name, description: descriptor.description.slice(0, 300), safetyClass: descriptor.safetyClass, executionKind: descriptor.execution.kind});
    const estimate = Math.ceil(JSON.stringify(card).length / 4);
    if (used + estimate > hardLimit) break;
    cards.push(card); used += estimate;
  }
  return Object.freeze(cards);
}
