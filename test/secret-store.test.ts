import assert from "node:assert/strict";
import {access, mkdir, mkdtemp, readFile, rm, writeFile} from "node:fs/promises";
import {tmpdir} from "node:os";
import {join} from "node:path";
import test from "node:test";
import {SecretStore} from "../src/config/secret-store.js";

test("fallback credentials are stored in a user-level directory, not project state", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-secret-user-"));
  const legacyRoot = join(root, "project", ".mini-codex");
  const userConfigDir = join(root, "user-config");
  const store = new SecretStore(legacyRoot, {userConfigDir, keytar: null});

  try {
    const location = await store.setApiKey("test-user-secret", "test-provider");
    const stored = JSON.parse(
      await readFile(join(userConfigDir, "credentials.json"), "utf8")
    ) as {providers: Record<string, string>};

    assert.equal(location, "user-file");
    assert.equal(stored.providers["test-provider"], "test-user-secret");
    await assert.rejects(access(join(legacyRoot, "secrets.local.json")));
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("legacy project credentials migrate only after the user-level write succeeds", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-secret-migrate-"));
  const legacyRoot = join(root, "project", ".mini-codex");
  const legacyPath = join(legacyRoot, "secrets.local.json");
  const userConfigDir = join(root, "user-config");

  try {
    await mkdir(legacyRoot, {recursive: true});
    await writeFile(
      legacyPath,
      JSON.stringify({providers: {"test-provider": "legacy-secret"}}),
      "utf8"
    );
    const store = new SecretStore(legacyRoot, {userConfigDir, keytar: null});

    assert.equal(await store.getApiKey("test-provider", "UNUSED_TEST_PROVIDER_KEY"), "legacy-secret");
    const migrated = await store.getApiKey("test-provider", "UNUSED_TEST_PROVIDER_KEY");

    assert.equal(migrated, "legacy-secret");
    assert.equal(
      (JSON.parse(await readFile(join(userConfigDir, "credentials.json"), "utf8")) as {
        providers: Record<string, string>;
      }).providers["test-provider"],
      "legacy-secret"
    );
    await assert.rejects(access(legacyPath));
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("an empty normalized API key is rejected without creating a credential file", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-secret-empty-"));
  const userConfigDir = join(root, "user-config");
  const store = new SecretStore(join(root, "project", ".mini-codex"), {
    userConfigDir,
    keytar: null
  });

  try {
    await assert.rejects(store.setApiKey("  Bearer   ", "test-provider"), /empty/i);
    await assert.rejects(access(join(userConfigDir, "credentials.json")));
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});
