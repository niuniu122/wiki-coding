import assert from "node:assert/strict";
import {access, mkdir, mkdtemp, readFile, rm, writeFile} from "node:fs/promises";
import {tmpdir} from "node:os";
import {join, resolve} from "node:path";
import test from "node:test";
import {ConfigManager} from "../src/config/config-manager.js";
import {BUILT_IN_MODEL_PROVIDERS} from "../src/config/config-manager.js";
import {
  CredentialStore,
  type KeyringBackend
} from "../src/config/credential-store.js";
import {ProviderService} from "../src/runtime/provider-service.js";
import {createCredentialTarget} from "../src/config/provider-security.js";
import type {AppConfig} from "../src/types.js";

class MemoryKeyring implements KeyringBackend {
  readonly values = new Map<string, string>();
  getCalls = 0;

  async getPassword(service: string, account: string): Promise<string | null> {
    this.getCalls += 1;
    return this.values.get(`${service}:${account}`) ?? null;
  }

  async setPassword(service: string, account: string, value: string): Promise<void> {
    this.values.set(`${service}:${account}`, value);
  }

  async deletePassword(service: string, account: string): Promise<void> {
    this.values.delete(`${service}:${account}`);
  }
}

class FailingSaveConfigManager extends ConfigManager {
  failSave = false;

  override async save(config: AppConfig): Promise<void> {
    if (this.failSave) {
      throw new Error("simulated config save failure");
    }
    await super.save(config);
  }
}

const minimaxTarget = createCredentialTarget(
  "minimax-official",
  BUILT_IN_MODEL_PROVIDERS["minimax-official"]!
);
const hashsightTarget = createCredentialTarget(
  "hashsight",
  BUILT_IN_MODEL_PROVIDERS.hashsight!
);

test("credential paths remain unchanged when using the shared user config root", async () => {
  const userConfigDir = join("relative", "credential-home");
  const store = new CredentialStore({keyring: null, userConfigDir, env: {}});

  const status = await store.inspect(minimaxTarget);

  assert.equal(status.userFilePath, join(resolve(userConfigDir), "credentials.json"));
});

test("plaintext storage requires a single-use consent token", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-consent-"));
  const userConfigDir = join(root, "user-config");
  const store = new CredentialStore({keyring: null, userConfigDir, env: {}});

  try {
    await assert.rejects(
      () => store.saveToUserFile(minimaxTarget, "secret", undefined),
      /consent/i
    );
    const consent = store.createPlaintextConsent();
    await store.saveToUserFile(minimaxTarget, "secret", consent);
    await assert.rejects(
      () => store.saveToUserFile(hashsightTarget, "another", consent),
      /already used/i
    );

    const stored = JSON.parse(
      await readFile(join(userConfigDir, "credentials.json"), "utf8")
    ) as {targets: Record<string, string>};
    assert.equal(stored.targets[minimaxTarget.fingerprint], "secret");
    assert.equal(stored.targets[hashsightTarget.fingerprint], undefined);
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("environment credentials win over every persisted backend", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-credential-order-"));
  const keyring = new MemoryKeyring();
  keyring.values.set(`minimax-codex:minimax-api-key:v2:${minimaxTarget.fingerprint}`, "keyring");
  const store = new CredentialStore({
    keyring,
    userConfigDir: join(root, "user-config"),
    env: {MINIMAX_API_KEY: "environment"}
  });

  try {
    assert.equal(
      await store.get(minimaxTarget),
      "environment"
    );
    assert.equal((await store.inspect(minimaxTarget)).backend, "environment");
    assert.equal(keyring.getCalls, 0);
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("OS keyring credentials win over an existing user-file credential", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-keyring-order-"));
  const userConfigDir = join(root, "user-config");
  const fileStore = new CredentialStore({keyring: null, userConfigDir, env: {}});
  const keyring = new MemoryKeyring();
  keyring.values.set(`minimax-codex:minimax-api-key:v2:${minimaxTarget.fingerprint}`, "keyring-secret");

  try {
    await fileStore.saveToUserFile(
      minimaxTarget,
      "file-secret",
      fileStore.createPlaintextConsent()
    );
    const store = new CredentialStore({keyring, userConfigDir, env: {}});

    assert.equal(await store.get(minimaxTarget), "keyring-secret");
    assert.equal((await store.inspect(minimaxTarget)).backend, "os-keyring");
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("saving another provider canonicalizes and preserves a legacy user-file MiniMax key", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-legacy-user-file-"));
  const userConfigDir = join(root, "user-config");
  const credentialPath = join(userConfigDir, "credentials.json");
  const store = new CredentialStore({keyring: null, userConfigDir, env: {}});

  try {
    await mkdir(userConfigDir, {recursive: true});
    await writeFile(
      credentialPath,
      JSON.stringify({minimaxApiKey: "legacy-minimax-secret"}),
      {encoding: "utf8", flag: "wx"}
    );
    await store.saveToUserFile(
      hashsightTarget,
      "new-hashsight-secret",
      store.createPlaintextConsent()
    );

    assert.equal(await store.get(minimaxTarget), "legacy-minimax-secret");
    const stored = JSON.parse(await readFile(credentialPath, "utf8")) as {
      minimaxApiKey?: string;
      targets: Record<string, string>;
    };
    assert.equal(stored.minimaxApiKey, undefined);
    assert.equal(stored.targets[minimaxTarget.fingerprint], "legacy-minimax-secret");
    assert.equal(stored.targets[hashsightTarget.fingerprint], "new-hashsight-secret");
    assert.equal(await store.get(hashsightTarget), "new-hashsight-secret");
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("model selection can inspect a legacy credential without migrating its bytes", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-credential-peek-"));
  const store = new CredentialStore({userConfigDir: root, keyring: null, env: {}});
  const raw = JSON.stringify({minimaxApiKey: "legacy-minimax-secret"});

  try {
    await writeFile(store.plaintextPath, raw, "utf8");

    assert.equal(await store.peek(minimaxTarget), "legacy-minimax-secret");
    assert.equal(await readFile(store.plaintextPath, "utf8"), raw);
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("model selection credential inspection fails closed on keyring conflicts", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-credential-peek-conflict-"));
  const keyring = new MemoryKeyring();
  const store = new CredentialStore({userConfigDir: root, keyring, env: {}});

  try {
    await store.saveToKeyring(minimaxTarget, "scoped-secret");
    keyring.values.set("minimax-codex:minimax-api-key", "conflicting-legacy-secret");
    const before = new Map(keyring.values);

    await assert.rejects(() => store.peek(minimaxTarget));
    assert.deepEqual(keyring.values, before);
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("migrating one legacy plaintext provider preserves other legacy provider keys", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-legacy-file-preserve-"));
  const userConfigDir = join(root, "user-config");
  const credentialPath = join(userConfigDir, "credentials.json");
  const store = new CredentialStore({keyring: null, userConfigDir, env: {}});

  try {
    await mkdir(userConfigDir, {recursive: true});
    await writeFile(credentialPath, JSON.stringify({
      minimaxApiKey: "minimax-legacy",
      providers: {hashsight: "hashsight-legacy"}
    }), "utf8");
    await writeFile(`${credentialPath}.bak`, JSON.stringify({
      providers: {hashsight: "hashsight-legacy"}
    }), "utf8");

    assert.equal(await store.get(hashsightTarget), "hashsight-legacy");
    assert.equal(await store.get(minimaxTarget), "minimax-legacy");
    await assert.rejects(access(`${credentialPath}.bak`));
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("an unavailable native keyring never writes plaintext implicitly", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-keyring-unavailable-"));
  const userConfigDir = join(root, "user-config");
  const store = new CredentialStore({keyring: null, userConfigDir, env: {}});

  try {
    assert.equal((await store.inspect(minimaxTarget)).backend, "unavailable");
    await assert.rejects(
      () => store.saveToKeyring(minimaxTarget, "must-not-fallback"),
      /keyring.*unavailable/i
    );
    await assert.rejects(access(join(userConfigDir, "credentials.json")));
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("ProviderService saves to the keyring and persists canonical provider switches", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-provider-service-"));
  const keyring = new MemoryKeyring();
  const credentialStore = new CredentialStore({
    keyring,
    userConfigDir: join(root, "user-config"),
    env: {}
  });
  const service = new ProviderService(new ConfigManager(root), credentialStore);

  try {
    await service.init();
    assert.equal(service.list().some((entry) => entry.includes("hashsight")), true);
    assert.equal(await service.saveApiKey("  Bearer service-secret  "), "os-keyring");
    assert.equal(await service.getApiKey(), "service-secret");
    assert.equal((await service.inspectCredential()).backend, "os-keyring");

    const summary = await service.switch("hashsight");
    assert.equal(summary.includes("hashsight"), true);
    assert.equal(service.config.modelProvider, "hashsight");

    const persisted = await new ConfigManager(root).load();
    assert.equal(persisted.modelProvider, "hashsight");
    assert.equal("api" in persisted, false);
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("ProviderService requires consent before unavailable keyring fallback", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-provider-consent-"));
  const userConfigDir = join(root, "user-config");
  const credentialStore = new CredentialStore({keyring: null, userConfigDir, env: {}});
  const service = new ProviderService(new ConfigManager(root), credentialStore);

  try {
    await service.init();
    assert.equal(await service.saveApiKey("pending-secret"), "unavailable");
    await assert.rejects(access(join(userConfigDir, "credentials.json")));

    const consent = credentialStore.createPlaintextConsent();
    assert.equal(await service.saveApiKey("pending-secret", consent), "user-file");
    assert.equal(await service.getApiKey(), "pending-secret");
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("a keyring set failure never falls back to or modifies the plaintext file", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-keyring-set-failure-"));
  const userConfigDir = join(root, "user-config");
  const fileStore = new CredentialStore({keyring: null, userConfigDir, env: {}});

  try {
    await fileStore.saveToUserFile(
      minimaxTarget,
      "existing-file-secret",
      fileStore.createPlaintextConsent()
    );
    const credentialPath = join(userConfigDir, "credentials.json");
    const before = await readFile(credentialPath, "utf8");
    const credentialStore = new CredentialStore({
      userConfigDir,
      env: {},
      keyring: {
        async getPassword() {
          return null;
        },
        async setPassword() {
          throw new Error("simulated keyring set failure");
        },
        async deletePassword() {
          return;
        }
      }
    });
    const service = new ProviderService(new ConfigManager(root), credentialStore);
    await service.init();

    await assert.rejects(
      () => service.saveApiKey("must-not-be-written", credentialStore.createPlaintextConsent()),
      /OS keyring access failed/
    );
    assert.equal(await readFile(credentialPath, "utf8"), before);
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("a failed Provider switch leaves memory and disk on the original provider", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-provider-switch-failure-"));
  const manager = new FailingSaveConfigManager(root);
  const credentialStore = new CredentialStore({
    keyring: null,
    userConfigDir: join(root, "user-config"),
    env: {}
  });
  const service = new ProviderService(manager, credentialStore);

  try {
    await manager.save({
      schemaVersion: 1,
      modelProvider: "minimax-official",
      modelProviders: {
        "minimax-official": {
          name: "MiniMax Official",
          baseUrl: "https://api.minimax.io/v1",
          protocol: "responses",
          envKey: "MINIMAX_API_KEY",
          defaultModel: "MiniMax-M3"
        },
        hashsight: {
          name: "HashSight",
          baseUrl: "https://example.test/v1",
          protocol: "chat_completions",
          envKey: "HASHSIGHT_API_KEY",
          defaultModel: "MiniMax-M3"
        }
      },
      model: "MiniMax-M3",
      context: {
        workingContextLimit: 128000,
        autoCompactRatio: 0.9,
        maxCompletionTokens: 8192
      }
    });
    await service.init();
    const before = await readFile(join(root, "config.json"), "utf8");
    manager.failSave = true;

    await assert.rejects(() => service.switch("hashsight"), /simulated config save failure/);
    assert.equal(service.config.modelProvider, "minimax-official");
    assert.equal(service.list().some((entry) => entry.includes("minimax-official") && entry.includes("active")), true);
    assert.equal(await readFile(join(root, "config.json"), "utf8"), before);
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});
