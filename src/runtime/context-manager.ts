import {ConservativeTokenEstimator} from "./token-estimator.js";

export {
  ContextEngine,
  ContextEngine as ContextManager,
  type BuildContextParams,
  type BuiltContext
} from "./context-engine.js";

const compatibilityEstimator = new ConservativeTokenEstimator();

export function estimateTokens(text: string): number {
  return compatibilityEstimator.estimateText(text);
}
