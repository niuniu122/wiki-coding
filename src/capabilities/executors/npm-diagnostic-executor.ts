import {spawn, type ChildProcessWithoutNullStreams} from "node:child_process";
import {existsSync} from "node:fs";
import {basename, dirname, join, resolve} from "node:path";
import type {CapabilityInvocation} from "../capability-invocation.js";
import type {CapabilityExecutionLimits} from "../execution-limits.js";
import type {CapabilityDescriptor} from "../types.js";

export type NpmDiagnosticResult =
  | {readonly status: "succeeded"; readonly exitCode: 0; readonly output: string}
  | {readonly status: "failed"; readonly code: "invalid_arguments" | "spawn_failed" | "nonzero_exit" | "output_limit"; readonly exitCode?: number}
  | {readonly status: "timed_out"}
  | {readonly status: "cancelled"};

export interface DiagnosticProcessLauncher {
  launch(command: string, args: readonly string[], options: {cwd: string; env: NodeJS.ProcessEnv}): ChildProcessWithoutNullStreams;
}

const DEFAULT_LAUNCHER: DiagnosticProcessLauncher = {
  launch(command, args, options) {
    return spawn(command, [...args], {
      cwd: options.cwd,
      env: options.env,
      shell: false,
      windowsHide: true,
      stdio: ["pipe", "pipe", "pipe"]
    });
  }
};

export class NpmDiagnosticExecutor {
  constructor(
    workspaceRoot: string,
    private readonly limits: CapabilityExecutionLimits,
    private readonly launcher: DiagnosticProcessLauncher = DEFAULT_LAUNCHER
  ) {
    this.cwd = resolve(workspaceRoot);
  }

  private readonly cwd: string;

  async execute(descriptor: CapabilityDescriptor, invocation: CapabilityInvocation, signal?: AbortSignal): Promise<NpmDiagnosticResult> {
    if (descriptor.execution.kind !== "npm_script" || Object.keys(invocation.arguments).length !== 0) {
      return {status: "failed", code: "invalid_arguments"};
    }
    if (signal?.aborted) return {status: "cancelled"};
    const npm = resolveNpmInvocation();
    const args = [...npm.prefixArgs, "run", descriptor.execution.script, "--", ...descriptor.execution.argv];
    let child: ChildProcessWithoutNullStreams;
    try {
      child = this.launcher.launch(npm.command, args, {cwd: this.cwd, env: safeEnvironment()});
      child.stdin.end();
    } catch {
      return {status: "failed", code: "spawn_failed"};
    }
    return await monitor(child, this.limits, signal);
  }
}

async function monitor(child: ChildProcessWithoutNullStreams, limits: CapabilityExecutionLimits, signal?: AbortSignal): Promise<NpmDiagnosticResult> {
  return await new Promise<NpmDiagnosticResult>((resolveResult) => {
    const chunks: Buffer[] = [];
    let bytes = 0;
    let settled = false;
    let terminal: "timed_out" | "cancelled" | "output_limit" | undefined;
    let timer: NodeJS.Timeout | undefined;
    const finish = (result: NpmDiagnosticResult) => {
      if (settled) return;
      settled = true;
      if (timer) clearTimeout(timer);
      signal?.removeEventListener("abort", abort);
      resolveResult(result);
    };
    const stop = (reason: typeof terminal) => {
      if (terminal) return;
      terminal = reason;
      child.kill();
    };
    const collect = (chunk: Buffer | string, preserve: boolean) => {
      const value = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk);
      bytes += value.byteLength;
      if (bytes > limits.maxOutputBytes) {
        stop("output_limit");
      } else if (preserve) {
        chunks.push(value);
      }
    };
    child.stdout.on("data", (chunk: Buffer) => collect(chunk, true));
    // stderr contributes to the limit but is deliberately not persisted or returned.
    child.stderr.on("data", (chunk: Buffer) => collect(chunk, false));
    child.once("error", () => finish({status: "failed", code: "spawn_failed"}));
    child.once("close", (code) => {
      if (terminal === "timed_out") return finish({status: "timed_out"});
      if (terminal === "cancelled") return finish({status: "cancelled"});
      if (terminal === "output_limit") return finish({status: "failed", code: "output_limit"});
      if (code === 0) return finish({status: "succeeded", exitCode: 0, output: Buffer.concat(chunks).toString("utf8")});
      return finish({status: "failed", code: "nonzero_exit", ...(typeof code === "number" ? {exitCode: code} : {})});
    });
    timer = setTimeout(() => stop("timed_out"), limits.timeoutMs);
    timer.unref();
    const abort = () => stop("cancelled");
    signal?.addEventListener("abort", abort, {once: true});
  });
}

function safeEnvironment(): NodeJS.ProcessEnv {
  const allowed = ["PATH", "Path", "SystemRoot", "ComSpec", "PATHEXT", "TEMP", "TMP", "HOME", "USERPROFILE", "APPDATA", "LOCALAPPDATA"];
  return Object.fromEntries(allowed.flatMap((key) => process.env[key] === undefined ? [] : [[key, process.env[key]]]));
}

function resolveNpmInvocation(): {command: string; prefixArgs: readonly string[]} {
  if (process.platform !== "win32") return {command: "npm", prefixArgs: []};
  const inherited = process.env.npm_execpath;
  if (inherited && basename(inherited).toLocaleLowerCase("en-US") === "npm-cli.js" && existsSync(inherited)) {
    return {command: process.execPath, prefixArgs: [inherited]};
  }
  const bundled = join(dirname(process.execPath), "node_modules", "npm", "bin", "npm-cli.js");
  return existsSync(bundled)
    ? {command: process.execPath, prefixArgs: [bundled]}
    : {command: "npm.cmd", prefixArgs: []};
}
