import assert from "node:assert/strict";
import {mkdtemp, rm, writeFile} from "node:fs/promises";
import {tmpdir} from "node:os";
import {join} from "node:path";
import test from "node:test";
import {ConfigManager, DEFAULT_CONFIG} from "../src/config/config-manager.js";

test("config validation rejects an invalid context limit with its field path", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-config-invalid-"));

  try {
    await writeFile(
      join(root, "config.json"),
      JSON.stringify({context: {workingContextLimit: "many"}}),
      "utf8"
    );
    await assert.rejects(
      new ConfigManager(root).load(),
      /context\.workingContextLimit/
    );
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("config validation rejects an explicitly selected unknown provider", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-config-provider-"));

  try {
    await writeFile(
      join(root, "config.json"),
      JSON.stringify({modelProvider: "typo-provider"}),
      "utf8"
    );
    await assert.rejects(new ConfigManager(root).load(), /modelProvider.*typo-provider/i);
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("config manager recovers a schema-invalid primary from its valid backup", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-config-backup-"));
  const manager = new ConfigManager(root);

  try {
    await manager.save(DEFAULT_CONFIG);
    await manager.save({...DEFAULT_CONFIG, model: "MiniMax-M3-new"});
    await writeFile(
      join(root, "config.json"),
      JSON.stringify({context: {workingContextLimit: -1}}),
      "utf8"
    );

    const recovered = await manager.load();
    assert.equal(recovered.model, DEFAULT_CONFIG.model);
    assert.equal(recovered.context.workingContextLimit, DEFAULT_CONFIG.context.workingContextLimit);
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});
