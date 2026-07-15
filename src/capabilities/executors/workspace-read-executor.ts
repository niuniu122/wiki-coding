import {open, readdir, realpath, stat} from "node:fs/promises";
import {isAbsolute, relative, resolve, sep} from "node:path";
import type {CapabilityInvocation} from "../capability-invocation.js";
import type {CapabilityExecutionLimits} from "../execution-limits.js";
import {isWithin} from "../path-policy.js";
import type {CapabilityDescriptor} from "../types.js";

export type WorkspaceReadResult =
  | {readonly status: "succeeded"; readonly output: string}
  | {readonly status: "failed"; readonly code: "invalid_arguments" | "invalid_path" | "wrong_file_type" | "binary_file" | "output_limit" | "read_failed"};

export class WorkspaceReadExecutor {
  constructor(
    private readonly workspaceRoot: string,
    private readonly limits: CapabilityExecutionLimits
  ) {}

  async execute(descriptor: CapabilityDescriptor, invocation: CapabilityInvocation): Promise<WorkspaceReadResult> {
    if (descriptor.execution.kind !== "workspace_read") {
      return {status: "failed", code: "invalid_arguments"};
    }
    const path = exactPathArgument(invocation.arguments);
    if (path === null) return {status: "failed", code: "invalid_arguments"};
    try {
      const target = await this.resolveWorkspaceTarget(path);
      return descriptor.execution.operation === "read_file"
        ? await this.readFile(target)
        : await this.listFiles(target);
    } catch (error) {
      return {status: "failed", code: error instanceof WorkspaceReadError ? error.code : "read_failed"};
    }
  }

  private async resolveWorkspaceTarget(candidate: string): Promise<string> {
    if (!candidate || candidate.includes("\0") || isAbsolute(candidate)) {
      throw new WorkspaceReadError("invalid_path");
    }
    const segments = candidate.split(/[\\/]+/u);
    if (segments.some((segment) => segment === "..")) {
      throw new WorkspaceReadError("invalid_path");
    }
    const rootReal = await realpath(resolve(this.workspaceRoot));
    const targetReal = await realpath(resolve(rootReal, candidate));
    if (!isWithin(rootReal, targetReal)) throw new WorkspaceReadError("invalid_path");
    return targetReal;
  }

  private async readFile(target: string): Promise<WorkspaceReadResult> {
    const info = await stat(target);
    if (!info.isFile()) throw new WorkspaceReadError("wrong_file_type");
    if (info.size > this.limits.maxOutputBytes) throw new WorkspaceReadError("output_limit");
    const handle = await open(target, "r");
    try {
      const opened = await handle.stat();
      if (!opened.isFile()) throw new WorkspaceReadError("wrong_file_type");
      if (opened.size > this.limits.maxOutputBytes) throw new WorkspaceReadError("output_limit");
      const content = await handle.readFile();
      if (content.byteLength > this.limits.maxOutputBytes) throw new WorkspaceReadError("output_limit");
      if (content.includes(0)) throw new WorkspaceReadError("binary_file");
      return {status: "succeeded", output: content.toString("utf8")};
    } finally {
      await handle.close();
    }
  }

  private async listFiles(target: string): Promise<WorkspaceReadResult> {
    const info = await stat(target);
    if (!info.isDirectory()) throw new WorkspaceReadError("wrong_file_type");
    const entries = await readdir(target, {withFileTypes: true});
    if (entries.length > this.limits.maxDirectoryEntries) throw new WorkspaceReadError("output_limit");
    const output = entries
      .sort((left, right) => left.name.localeCompare(right.name))
      .map((entry) => `${entry.isDirectory() ? "directory" : entry.isFile() ? "file" : "other"}\t${entry.name}`)
      .join("\n");
    if (Buffer.byteLength(output) > this.limits.maxOutputBytes) throw new WorkspaceReadError("output_limit");
    return {status: "succeeded", output};
  }
}

function exactPathArgument(args: Readonly<Record<string, unknown>>): string | null {
  if (Object.keys(args).some((key) => key !== "path")) return null;
  return typeof args.path === "string" ? args.path : null;
}

class WorkspaceReadError extends Error {
  constructor(readonly code: "invalid_path" | "wrong_file_type" | "binary_file" | "output_limit") {
    super(code);
  }
}

// Keep the containment primitive visible to static audits without accepting caller-supplied roots.
export function workspaceRelativePath(root: string, target: string): string | null {
  const value = relative(root, target);
  return value === "" || (!value.startsWith(`..${sep}`) && value !== ".." && !isAbsolute(value)) ? value : null;
}
