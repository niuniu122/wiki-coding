import type {PermissionMode} from "../runtime/permission-service.js";
import type {CapabilityDescriptor} from "./types.js";

export type PolicyDecision =
  | {readonly decision: "allow"; readonly reason: "catalog_read" | "workspace_read" | "session_full_access"}
  | {readonly decision: "confirm"; readonly reason: "local_code_execution"}
  | {readonly decision: "deny"; readonly reason: "unavailable" | "snapshot_mismatch" | "unsupported_safety_class" | "network_forbidden"};

export class CapabilityPolicyEngine {
  decide(input: {descriptor: CapabilityDescriptor; permissionMode: PermissionMode; invocationSnapshotVersion: string; currentSnapshotVersion: string}): PolicyDecision {
    if (input.descriptor.availability !== "available") return {decision: "deny", reason: "unavailable"};
    if (input.invocationSnapshotVersion !== input.currentSnapshotVersion) return {decision: "deny", reason: "snapshot_mismatch"};
    switch (input.descriptor.safetyClass) {
      case "catalog_read": return {decision: "allow", reason: "catalog_read"};
      case "workspace_read": return {decision: "allow", reason: "workspace_read"};
      case "local_diagnostic": return input.permissionMode === "full_access" ? {decision: "allow", reason: "session_full_access"} : {decision: "confirm", reason: "local_code_execution"};
      case "network": return {decision: "deny", reason: "network_forbidden"};
      default: return {decision: "deny", reason: "unsupported_safety_class"};
    }
  }
}
