import assert from "node:assert/strict";
import {mkdtemp, readFile, rm, writeFile} from "node:fs/promises";
import {tmpdir} from "node:os";
import {join} from "node:path";
import test from "node:test";
import {ConfigManager, DEFAULT_CONFIG} from "../src/config/config-manager.js";

test("legacy api configuration migrates to one provider source", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-config-legacy-api-"));
  const configPath = join(root, "config.json");
  const manager = new ConfigManager(root);

  try {
    await writeFile(
      configPath,
      JSON.stringify({
        api: {
          provider: "hashsight",
          protocol: "chat_completions",
          baseUrl: "https://example.test/v1"
        }
      }),
      "utf8"
    );

    const config = await manager.load();

    assert.equal(config.schemaVersion, 1);
    assert.equal(config.modelProvider, "hashsight");
    assert.equal(config.modelProviders.hashsight?.baseUrl, "https://example.test/v1");
    assert.equal("api" in config, false);
    assert.equal("storage" in config, false);

    const saved = JSON.parse(await readFile(configPath, "utf8")) as Record<string, unknown>;
    assert.equal(saved.schemaVersion, 1);
    assert.equal("api" in saved, false);
    assert.equal("storage" in saved, false);
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("read-only legacy config projection never rewrites workspace bytes", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-config-read-only-"));
  const configPath = join(root, "config.json");
  const raw = JSON.stringify({
    api: {
      provider: "hashsight",
      protocol: "chat_completions",
      baseUrl: "https://example.test/v1"
    }
  });

  try {
    await writeFile(configPath, raw, "utf8");

    const config = await new ConfigManager(root).loadReadOnly();

    assert.equal(config.modelProvider, "hashsight");
    assert.equal(await readFile(configPath, "utf8"), raw);
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("sqlite is rejected during configuration migration", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-config-sqlite-"));

  try {
    await writeFile(
      join(root, "config.json"),
      JSON.stringify({storage: {driver: "sqlite"}}),
      "utf8"
    );

    await assert.rejects(
      () => new ConfigManager(root).load(),
      /SQLite is not supported.*jsonl/i
    );
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("an explicit sqlite selection is not silently recovered from a JSONL backup", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-config-sqlite-backup-"));
  const manager = new ConfigManager(root);

  try {
    await manager.save(DEFAULT_CONFIG);
    await manager.save({...DEFAULT_CONFIG, model: "MiniMax-M3-backup-source"});
    await writeFile(
      join(root, "config.json"),
      JSON.stringify({storage: {driver: "sqlite"}}),
      "utf8"
    );

    await assert.rejects(() => manager.load(), /SQLite is not supported.*jsonl/i);
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

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
