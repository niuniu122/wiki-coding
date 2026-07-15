import assert from "node:assert/strict";
import {mkdir, mkdtemp, rm, writeFile} from "node:fs/promises";
import {tmpdir} from "node:os";
import {join} from "node:path";
import test from "node:test";
import {MiniMaxCapabilitySource} from "../src/capabilities/sources/minimax-source.js";
import {CodexCapabilitySource, CodexPluginCapabilitySource} from "../src/capabilities/sources/codex-source.js";
import {ClawCodeCapabilitySource} from "../src/capabilities/sources/claw-code-source.js";

test("installed MiniMax, Codex, and Claw Code metadata normalize without executing code", async () => {
  const root = await mkdtemp(join(tmpdir(), "capability-sources-"));
  try {
    const minimax = join(root, "minimax");
    const codex = join(root, "codex");
    const claw = join(root, "claw");
    const plugins = join(root, "plugins");
    await mkdir(join(codex, "review"), {recursive: true});
    await mkdir(minimax, {recursive: true});
    await mkdir(claw, {recursive: true});
    await mkdir(join(plugins, "formatter", ".codex-plugin"), {recursive: true});
    await writeFile(join(minimax, "read.json"), JSON.stringify({
      schemaVersion: 1,
      id: "capability:minimax/read-file",
      name: "Read file",
      description: "Read workspace text",
      aliases: [], commands: [], safetyClass: "workspace_read",
      execution: {kind: "workspace_read", operation: "read_file"}
    }));
    await writeFile(join(codex, "review", "SKILL.md"), "---\nname: Review\ndescription: Review local code\n---\nNever executed by discovery.\n");
    await writeFile(join(claw, "doctor.md"), "---\nname: Doctor\ndescription: Inspect local setup\n---\n");
    await writeFile(join(minimax, "invalid.json"), "{not-json");
    await writeFile(join(plugins, "formatter", ".codex-plugin", "plugin.json"), JSON.stringify({name: "Formatter", description: "Installed formatting plugin", version: "1.0.0"}));

    const results = await Promise.all([
      new MiniMaxCapabilitySource(minimax, "project_native").scan(),
      new CodexCapabilitySource(codex, "user_compat").scan(),
      new ClawCodeCapabilitySource(claw, "user_compat").scan(),
      new CodexPluginCapabilitySource(plugins, "user_compat").scan()
    ]);
    assert.deepEqual(results.map((result) => result.descriptors.length), [1, 1, 1, 1]);
    assert.equal(results[0]?.issues.length, 1);
    assert.equal(results[1]?.descriptors[0]?.execution.kind, "metadata_only");
    assert.deepEqual(results[2]?.descriptors[0]?.commands, ["/doctor"]);
    assert.equal(results[3]?.descriptors[0]?.source.kind, "codex_plugin");
    assert.equal(results[3]?.descriptors[0]?.execution.kind, "metadata_only");
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});
