import assert from "node:assert/strict";
import test from "node:test";
import {CapabilityDispatcher, type CapabilityInvocationRecorder} from "../src/capabilities/capability-dispatcher.js";
import {createCapabilityInvocation} from "../src/capabilities/capability-invocation.js";
import {createCapabilitySnapshot} from "../src/capabilities/capability-snapshot.js";
import {CapabilityCatalog} from "../src/capabilities/capability-catalog.js";
import {parseCapabilityManifest} from "../src/capabilities/capability-manifest.js";
import type {PermissionMode} from "../src/runtime/permission-service.js";

function diagnostic() {
  return parseCapabilityManifest({
    schemaVersion: 1,
    id: "capability:test/diagnostic",
    name: "diagnostic",
    description: "bounded local diagnostic",
    safetyClass: "local_diagnostic",
    execution: {kind: "npm_script", script: "test", argv: []}
  }, {kind: "minimax", scope: "builtin", root: "builtin", file: "diagnostic.json"});
}

test("dispatcher persists request before execution and result after it with one invocation id", async () => {
  const events: string[] = [];
  const recorder: CapabilityInvocationRecorder = {
    async recordRequest(invocation) { events.push(`request:${invocation.invocationId}`); },
    async recordResult(invocation) { events.push(`result:${invocation.invocationId}`); }
  };
  const descriptor = diagnostic();
  const snapshot = createCapabilitySnapshot(CapabilityCatalog.build([descriptor]).entries(), {version: "v1"});
  const dispatcher = new CapabilityDispatcher({
    workspaceRoot: process.cwd(), getSnapshot: () => snapshot, getPermissionMode: () => "full_access", recorder,
    npmDiagnosticExecutor: {async execute() { events.push("execute"); return {status: "succeeded", exitCode: 0, output: "ok"}; }}
  });
  const invocation = createCapabilityInvocation({invocationId: "invocation-stable", capabilityId: descriptor.id, snapshotVersion: "v1", arguments: {}, approved: false});
  assert.deepEqual(await dispatcher.dispatch(invocation), {status: "succeeded", invocationId: "invocation-stable", output: "ok"});
  assert.deepEqual(events, ["request:invocation-stable", "execute", "result:invocation-stable"]);
});

test("confirmation mode never starts an unapproved diagnostic", async () => {
  let executions = 0;
  const descriptor = diagnostic();
  const snapshot = createCapabilitySnapshot(CapabilityCatalog.build([descriptor]).entries(), {version: "v1"});
  const records: string[] = [];
  const dispatcher = new CapabilityDispatcher({
    workspaceRoot: process.cwd(), getSnapshot: () => snapshot, getPermissionMode: () => "confirm", recorder: recorder(records),
    npmDiagnosticExecutor: {async execute() { executions += 1; return {status: "succeeded", exitCode: 0, output: "bad"}; }}
  });
  const invocation = createCapabilityInvocation({capabilityId: descriptor.id, snapshotVersion: "v1", arguments: {}, approved: false});
  assert.equal((await dispatcher.dispatch(invocation)).status, "confirmation_required");
  assert.equal(executions, 0);
  assert.deepEqual(records, ["request", "result"]);
});

test("dispatcher rejects stale snapshots, unsupported execution and user-added argv", async () => {
  let mode: PermissionMode = "full_access";
  const descriptor = diagnostic();
  const snapshot = createCapabilitySnapshot(CapabilityCatalog.build([descriptor]).entries(), {version: "v2"});
  let executions = 0;
  const dispatcher = new CapabilityDispatcher({
    workspaceRoot: process.cwd(), getSnapshot: () => snapshot, getPermissionMode: () => mode, recorder: recorder([]),
    npmDiagnosticExecutor: {async execute(_descriptor, invocation) { executions += 1; return Object.keys(invocation.arguments).length ? {status: "failed", code: "invalid_arguments"} : {status: "succeeded", exitCode: 0, output: "ok"}; }}
  });
  const stale = createCapabilityInvocation({capabilityId: descriptor.id, snapshotVersion: "v1", arguments: {}, approved: true});
  assert.deepEqual(await dispatcher.dispatch(stale), {status: "denied", invocationId: stale.invocationId, reason: "snapshot_mismatch"});
  const injected = createCapabilityInvocation({capabilityId: descriptor.id, snapshotVersion: "v2", arguments: {argv: ["--evil"]}, approved: true});
  assert.deepEqual(await dispatcher.dispatch(injected), {status: "failed", invocationId: injected.invocationId, code: "invalid_arguments"});
  assert.equal(executions, 1);
  mode = "confirm";
});

function recorder(events: string[]): CapabilityInvocationRecorder {
  return {async recordRequest() { events.push("request"); }, async recordResult() { events.push("result"); }};
}
