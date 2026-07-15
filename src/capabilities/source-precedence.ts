import type {CapabilitySourceScope} from "./types.js";

const PRECEDENCE: Readonly<Record<CapabilitySourceScope, number>> = Object.freeze({
  builtin: 500,
  project_native: 400,
  user_native: 300,
  project_compat: 200,
  user_compat: 100
});

export function capabilitySourcePrecedence(scope: CapabilitySourceScope): number {
  return PRECEDENCE[scope];
}
