import assert from "node:assert/strict";
import {access, mkdir, mkdtemp, readFile, rm, writeFile} from "node:fs/promises";
import {tmpdir} from "node:os";
import {join} from "node:path";
import test from "node:test";
import {SecretStore} from "../src/config/secret-store.js";
import {BUILT_IN_MODEL_PROVIDERS} from "../src/config/config-manager.js";
import {createCredentialTarget} from "../src/config/provider-security.js";
import type {KeyringBackend} from "../src/config/credential-store.js";

const minimaxTarget = createCredentialTarget(
  "minimax-official",
  BUILT_IN_MODEL_PROVIDERS["minimax-official"]!
);

class MemoryKeyring implements KeyringBackend {
  readonly values = new Map<string, string>();

  async getPassword(service: string, account: string): Promise<string | null> {
    return this.values.get(`${service}:${account}`) ?? null;
  }

  async setPassword(service: string, account: string, value: string): Promise<void> {
    this.values.set(`${service}:${account}`, value);
  }

  async deletePassword(service: string, account: string): Promise<void> {
    this.values.delete(`${service}:${account}`);
  }
}

test("fallback credentials are stored in a user-level directory, not project state", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-secret-user-"));
  const legacyRoot = join(root, "project", ".mini-codex");
  const userConfigDir = join(root, "user-config");
  const store = new SecretStore(legacyRoot, {userConfigDir, keytar: null});

  try {
    await assert.rejects(
      () => store.setApiKey("test-user-secret", "minimax-official"),
      /consent/i
    );
    const location = await store.setApiKey(
      "test-user-secret",
      "minimax-official",
      store.createPlaintextConsent()
    );
    const stored = JSON.parse(
      await readFile(join(userConfigDir, "credentials.json"), "utf8")
    ) as {targets: Record<string, string>};

    assert.equal(location, "user-file");
    assert.equal(stored.targets[minimaxTarget.fingerprint], "test-user-secret");
    await assert.rejects(access(join(legacyRoot, "secrets.local.json")));
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("legacy project credentials defer when the keyring is unavailable", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-secret-migrate-"));
  const legacyRoot = join(root, "project", ".mini-codex");
  const legacyPath = join(legacyRoot, "secrets.local.json");
  const userConfigDir = join(root, "user-config");

  try {
    await mkdir(legacyRoot, {recursive: true});
    await writeFile(
      legacyPath,
      JSON.stringify({providers: {"minimax-official": "legacy-secret"}}),
      "utf8"
    );
    const store = new SecretStore(legacyRoot, {userConfigDir, keytar: null});

    const migrated = await store.getApiKey(
      "minimax-official",
      "MINIMAX_API_KEY",
      store.createPlaintextConsent()
    );

    assert.equal(migrated, null);
    await assert.rejects(access(join(userConfigDir, "credentials.json")));
    assert.equal((await readFile(legacyPath, "utf8")).includes("legacy-secret"), true);
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("a failed legacy credential migration retains the complete workspace source", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-secret-migrate-failure-"));
  const legacyRoot = join(root, "project", ".mini-codex");
  const legacyPath = join(legacyRoot, "secrets.local.json");

  try {
    await mkdir(legacyRoot, {recursive: true});
    await writeFile(
      legacyPath,
      JSON.stringify({
        providers: {
          "minimax-official": "first-secret",
          hashsight: "second-secret"
        }
      }),
      "utf8"
    );
    const store = new SecretStore(legacyRoot, {
      userConfigDir: join(root, "user-config"),
      env: {},
      keytar: {
        async getPassword() {
          return null;
        },
        async setPassword() {
          throw new Error("simulated keyring write failure");
        },
        async deletePassword() {
          return;
        }
      }
    });

    await assert.rejects(
      () => store.getApiKey("minimax-official", "UNUSED_TEST_PROVIDER_KEY"),
      /OS keyring access failed/
    );
    assert.equal((await readFile(legacyPath, "utf8")).includes("first-secret"), true);
    assert.equal((await readFile(legacyPath, "utf8")).includes("second-secret"), true);
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
    await assert.rejects(
      store.setApiKey(
        "  Bearer   ",
        "minimax-official",
        store.createPlaintextConsent()
      ),
      /empty/i
    );
    await assert.rejects(access(join(userConfigDir, "credentials.json")));
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("legacy SecretStore refuses provider-ID-only access for custom providers", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-secret-custom-scope-"));
  const store = new SecretStore(join(root, ".mini-codex"), {
    userConfigDir: join(root, "user-config"),
    keytar: null
  });

  try {
    await assert.rejects(
      () => store.getApiKey("custom-provider", "CUSTOM_API_KEY"),
      /cannot scope custom provider/i
    );
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("SecretStore cleans legacy source and backup when a scoped credential already exists", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-secret-existing-scoped-"));
  const legacyRoot = join(root, ".mini-codex");
  const legacyPath = join(legacyRoot, "secrets.local.json");
  const keyring = new MemoryKeyring();
  const store = new SecretStore(legacyRoot, {
    userConfigDir: join(root, "user-config"),
    keytar: keyring,
    env: {}
  });

  try {
    await store.setApiKey("scoped-secret", "minimax-official");
    await mkdir(legacyRoot, {recursive: true});
    const legacy = JSON.stringify({providers: {"minimax-official": "legacy-secret"}});
    await writeFile(legacyPath, legacy, "utf8");
    await writeFile(`${legacyPath}.bak`, legacy, "utf8");

    assert.equal(await store.getApiKey("minimax-official"), "scoped-secret");
    await assert.rejects(access(legacyPath));
    await assert.rejects(access(`${legacyPath}.bak`));
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});
