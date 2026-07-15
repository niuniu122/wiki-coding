import assert from "node:assert/strict";
import {spawn} from "node:child_process";
import {fileURLToPath} from "node:url";
import test from "node:test";
import {createCapabilityInvocation} from "../src/capabilities/capability-invocation.js";
import {NpmDiagnosticExecutor, type DiagnosticProcessLauncher} from "../src/capabilities/executors/npm-diagnostic-executor.js";
import {parseCapabilityManifest} from "../src/capabilities/capability-manifest.js";

const fixtureRoot = fileURLToPath(new URL("fixtures/executors/", import.meta.url));

function descriptor(script: string, argv: string[] = []) {
  return parseCapabilityManifest({
    schemaVersion: 1,
    id: `capability:test/${script}`,
    name: script,
    description: "diagnostic fixture",
    safetyClass: "local_diagnostic",
    execution: {kind: "npm_script", script, argv}
  }, {kind: "minimax", scope: "builtin", root: "builtin", file: `${script}.json`});
}

function invocation(capabilityId: string, args: Record<string, unknown> = {}) {
  return createCapabilityInvocation({capabilityId, snapshotVersion: "v1", arguments: args, approved: true});
}

test("npm diagnostic runs only the manifest script and fixed argv", async () => {
  const executor = new NpmDiagnosticExecutor(fixtureRoot, {timeoutMs: 5_000, maxOutputBytes: 16_000, maxDirectoryEntries: 10});
  const declared = descriptor("diag-ok", ["fixed"]);
  const result = await executor.execute(declared, invocation(declared.id));
  assert.equal(result.status, "succeeded");
  if (result.status === "succeeded") assert.match(result.output, /diagnostic:fixed/);
  assert.deepEqual(await executor.execute(declared, invocation(declared.id, {script: "diag-large"})), {status: "failed", code: "invalid_arguments"});
});

test("npm diagnostic enforces output limit without returning stderr", async () => {
  const executor = new NpmDiagnosticExecutor(fixtureRoot, {timeoutMs: 5_000, maxOutputBytes: 1_024, maxDirectoryEntries: 10});
  const declared = descriptor("diag-large");
  assert.deepEqual(await executor.execute(declared, invocation(declared.id)), {status: "failed", code: "output_limit"});
});

test("npm diagnostic supports timeout and cancellation", async () => {
  const launcher: DiagnosticProcessLauncher = {
    launch(_command, _args, options) {
      return spawn(process.execPath, ["-e", "setTimeout(() => {}, 10000)"], {
        cwd: options.cwd,
        env: options.env,
        shell: false,
        windowsHide: true,
        stdio: ["pipe", "pipe", "pipe"]
      });
    }
  };
  const declared = descriptor("diag-slow");
  const timeoutExecutor = new NpmDiagnosticExecutor(fixtureRoot, {timeoutMs: 30, maxOutputBytes: 1_024, maxDirectoryEntries: 10}, launcher);
  assert.deepEqual(await timeoutExecutor.execute(declared, invocation(declared.id)), {status: "timed_out"});

  const cancelExecutor = new NpmDiagnosticExecutor(fixtureRoot, {timeoutMs: 5_000, maxOutputBytes: 1_024, maxDirectoryEntries: 10}, launcher);
  const controller = new AbortController();
  const running = cancelExecutor.execute(declared, invocation(declared.id), controller.signal);
  controller.abort();
  assert.deepEqual(await running, {status: "cancelled"});
});

test("pre-aborted diagnostics do not launch a process", async () => {
  let launches = 0;
  const launcher: DiagnosticProcessLauncher = {launch() { launches += 1; throw new Error("must not launch"); }};
  const executor = new NpmDiagnosticExecutor(fixtureRoot, {timeoutMs: 100, maxOutputBytes: 1_024, maxDirectoryEntries: 10}, launcher);
  const controller = new AbortController(); controller.abort();
  const declared = descriptor("diag-ok");
  assert.deepEqual(await executor.execute(declared, invocation(declared.id), controller.signal), {status: "cancelled"});
  assert.equal(launches, 0);
});
