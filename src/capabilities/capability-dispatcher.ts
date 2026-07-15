import type {PermissionMode} from "../runtime/permission-service.js";
import type {CapabilityInvocation} from "./capability-invocation.js";
import type {CapabilitySnapshot} from "./capability-snapshot.js";
import {DEFAULT_CAPABILITY_EXECUTION_LIMITS, validateExecutionLimits, type CapabilityExecutionLimits} from "./execution-limits.js";
import {NpmDiagnosticExecutor, type NpmDiagnosticResult} from "./executors/npm-diagnostic-executor.js";
import {WorkspaceReadExecutor, type WorkspaceReadResult} from "./executors/workspace-read-executor.js";
import {CapabilityPolicyEngine} from "./policy-engine.js";
import type {CapabilityDescriptor} from "./types.js";

export type CapabilityDispatchResult =
  | {readonly status: "succeeded"; readonly invocationId: string; readonly output: string}
  | {readonly status: "confirmation_required"; readonly invocationId: string}
  | {readonly status: "denied"; readonly invocationId: string; readonly reason: string}
  | {readonly status: "failed"; readonly invocationId: string; readonly code: string; readonly exitCode?: number}
  | {readonly status: "timed_out" | "cancelled"; readonly invocationId: string};

export interface CapabilityInvocationRecorder {
  recordRequest(invocation: CapabilityInvocation, descriptor?: CapabilityDescriptor): Promise<void>;
  recordResult(invocation: CapabilityInvocation, result: CapabilityDispatchResult): Promise<void>;
}

export interface WorkspaceReadExecutorPort {
  execute(descriptor: CapabilityDescriptor, invocation: CapabilityInvocation): Promise<WorkspaceReadResult>;
}

export interface NpmDiagnosticExecutorPort {
  execute(descriptor: CapabilityDescriptor, invocation: CapabilityInvocation, signal?: AbortSignal): Promise<NpmDiagnosticResult>;
}

export interface CapabilityDispatcherOptions {
  readonly workspaceRoot: string;
  readonly getSnapshot: () => CapabilitySnapshot;
  readonly getPermissionMode: () => PermissionMode;
  readonly recorder: CapabilityInvocationRecorder;
  readonly policy?: CapabilityPolicyEngine;
  readonly limits?: CapabilityExecutionLimits;
  readonly workspaceReadExecutor?: WorkspaceReadExecutorPort;
  readonly npmDiagnosticExecutor?: NpmDiagnosticExecutorPort;
}

export class CapabilityDispatcher {
  private readonly policy: CapabilityPolicyEngine;
  private readonly workspaceRead: WorkspaceReadExecutorPort;
  private readonly npmDiagnostic: NpmDiagnosticExecutorPort;

  constructor(private readonly options: CapabilityDispatcherOptions) {
    const limits = validateExecutionLimits(options.limits ?? DEFAULT_CAPABILITY_EXECUTION_LIMITS);
    this.policy = options.policy ?? new CapabilityPolicyEngine();
    this.workspaceRead = options.workspaceReadExecutor ?? new WorkspaceReadExecutor(options.workspaceRoot, limits);
    this.npmDiagnostic = options.npmDiagnosticExecutor ?? new NpmDiagnosticExecutor(options.workspaceRoot, limits);
  }

  async dispatch(invocation: CapabilityInvocation, signal?: AbortSignal): Promise<CapabilityDispatchResult> {
    const snapshot = this.options.getSnapshot();
    const descriptor = snapshot.entries
      .map((entry) => entry.descriptor)
      .find((candidate) => candidate.id.toLocaleLowerCase("en-US") === invocation.capabilityId.toLocaleLowerCase("en-US") && candidate.availability === "available");
    await this.options.recorder.recordRequest(invocation, descriptor);
    if (!descriptor) {
      const result = {status: "denied", invocationId: invocation.invocationId, reason: "unknown_capability"} as const;
      await this.options.recorder.recordResult(invocation, result);
      return result;
    }
    const decision = this.policy.decide({
      descriptor,
      permissionMode: this.options.getPermissionMode(),
      invocationSnapshotVersion: invocation.snapshotVersion,
      currentSnapshotVersion: snapshot.version
    });
    let result: CapabilityDispatchResult;
    if (decision.decision === "deny") {
      result = {status: "denied", invocationId: invocation.invocationId, reason: decision.reason};
    } else if (decision.decision === "confirm" && !invocation.approved) {
      result = {status: "confirmation_required", invocationId: invocation.invocationId};
    } else {
      result = await this.execute(descriptor, invocation, signal);
    }
    await this.options.recorder.recordResult(invocation, result);
    return result;
  }

  private async execute(descriptor: CapabilityDescriptor, invocation: CapabilityInvocation, signal?: AbortSignal): Promise<CapabilityDispatchResult> {
    if (descriptor.safetyClass === "catalog_read" && descriptor.execution.kind === "metadata_only") {
      return {status: "succeeded", invocationId: invocation.invocationId, output: `${descriptor.name}\n${descriptor.description}`};
    }
    if (descriptor.safetyClass === "workspace_read" && descriptor.execution.kind === "workspace_read") {
      return mapExecutionResult(invocation.invocationId, await this.workspaceRead.execute(descriptor, invocation));
    }
    if (descriptor.safetyClass === "local_diagnostic" && descriptor.execution.kind === "npm_script") {
      return mapExecutionResult(invocation.invocationId, await this.npmDiagnostic.execute(descriptor, invocation, signal));
    }
    return {status: "denied", invocationId: invocation.invocationId, reason: "unsupported_execution"};
  }
}

function mapExecutionResult(invocationId: string, result: WorkspaceReadResult | NpmDiagnosticResult): CapabilityDispatchResult {
  if (result.status === "succeeded") return {status: "succeeded", invocationId, output: result.output};
  if (result.status === "timed_out" || result.status === "cancelled") return {status: result.status, invocationId};
  return {status: "failed", invocationId, code: result.code, ...("exitCode" in result && typeof result.exitCode === "number" ? {exitCode: result.exitCode} : {})};
}
