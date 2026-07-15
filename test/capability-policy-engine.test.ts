import assert from "node:assert/strict";
import test from "node:test";
import {CapabilityPolicyEngine} from "../src/capabilities/policy-engine.js";
import type {CapabilityDescriptor, CapabilitySafetyClass} from "../src/capabilities/types.js";

function descriptor(safetyClass: CapabilitySafetyClass, availability: CapabilityDescriptor["availability"] = "available"): CapabilityDescriptor {
  return {
    schemaVersion: 1,
    id: `capability:test/${safetyClass}`,
    name: safetyClass,
    description: "test descriptor",
    aliases: [],
    commands: [],
    safetyClass,
    availability,
    execution: {kind: "metadata_only"},
    idempotent: false,
    facets: {domain: [], action: [], object: []},
    intentDocument: safetyClass,
    source: {kind: "minimax", scope: "builtin", root: "builtin", file: "test.json"}
  };
}

test("policy permits local reads without escalating session authority", () => {
  const policy = new CapabilityPolicyEngine();
  for (const safetyClass of ["catalog_read", "workspace_read"] as const) {
    assert.equal(policy.decide({
      descriptor: descriptor(safetyClass),
      permissionMode: "confirm",
      invocationSnapshotVersion: "v1",
      currentSnapshotVersion: "v1"
    }).decision, "allow");
  }
});

test("local diagnostics require confirmation unless this session has full access", () => {
  const policy = new CapabilityPolicyEngine();
  const input = {
    descriptor: descriptor("local_diagnostic"),
    invocationSnapshotVersion: "v1",
    currentSnapshotVersion: "v1"
  } as const;
  assert.equal(policy.decide({...input, permissionMode: "confirm"}).decision, "confirm");
  assert.equal(policy.decide({...input, permissionMode: "workspace_read"}).decision, "confirm");
  assert.equal(policy.decide({...input, permissionMode: "full_access"}).decision, "allow");
});

test("policy fail-closes stale, unavailable, network and write capabilities", () => {
  const policy = new CapabilityPolicyEngine();
  const base = {permissionMode: "full_access", invocationSnapshotVersion: "v1", currentSnapshotVersion: "v1"} as const;
  assert.deepEqual(policy.decide({...base, descriptor: descriptor("workspace_read", "disabled")}), {decision: "deny", reason: "unavailable"});
  assert.deepEqual(policy.decide({...base, descriptor: descriptor("workspace_read"), currentSnapshotVersion: "v2"}), {decision: "deny", reason: "snapshot_mismatch"});
  assert.deepEqual(policy.decide({...base, descriptor: descriptor("network")}), {decision: "deny", reason: "network_forbidden"});
  assert.deepEqual(policy.decide({...base, descriptor: descriptor("workspace_write")}), {decision: "deny", reason: "unsupported_safety_class"});
});
