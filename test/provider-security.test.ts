import assert from "node:assert/strict";
import {mkdtemp, readFile, rm, writeFile} from "node:fs/promises";
import {tmpdir} from "node:os";
import {join} from "node:path";
import test from "node:test";
import {ConfigManager} from "../src/config/config-manager.js";
import {CredentialStore, type KeyringBackend} from "../src/config/credential-store.js";
import {
  createCredentialTarget,
  normalizeProviderEndpoint,
  resolveTrustedCredentialBinding
} from "../src/config/provider-security.js";
import {
  StrictProviderGateway,
  type ProviderGatewayEvent
} from "../src/providers/provider-gateway.js";
import type {HttpStreamRequest, HttpStreamTransport} from "../src/providers/http-transport.js";
import type {AppConfig, ModelProviderConfig} from "../src/types.js";

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

const official = (overrides: Partial<ModelProviderConfig> = {}): ModelProviderConfig => ({
  name: "MiniMax Official",
  baseUrl: "https://api.minimax.io/v1",
  protocol: "responses",
  envKey: "MINIMAX_API_KEY",
  defaultModel: "MiniMax-M3",
  ...overrides
});

const MIGRATION_FAILURE_MESSAGE =
  "OS keyring access failed. Retry or review the OS credential service.";

async function assertSafeMigrationFailure(
  operation: () => Promise<unknown>,
  secrets: readonly string[]
): Promise<void> {
  await assert.rejects(operation, (error: unknown) => {
    assert.equal(error instanceof Error, true);
    const message = error instanceof Error ? error.message : String(error);
    assert.equal(message, MIGRATION_FAILURE_MESSAGE);
    for (const secret of secrets) {
      assert.equal(message.includes(secret), false);
    }
    return true;
  });
}

test("credential targets isolate the same provider ID by endpoint", async () => {
  const keyring = new MemoryKeyring();
  const store = new CredentialStore({keyring, env: {}});
  const first = createCredentialTarget("minimax-official", official());
  const redirected = createCredentialTarget(
    "minimax-official",
    official({baseUrl: "https://attacker.example/v1"})
  );

  await store.saveToKeyring(first, "endpoint-a-secret");

  assert.equal(await store.get(first), "endpoint-a-secret");
  assert.equal(await store.get(redirected), null);
});

test("credential storage derives scope identity instead of trusting a supplied fingerprint", async () => {
  const keyring = new MemoryKeyring();
  const store = new CredentialStore({keyring, env: {}});
  const original = createCredentialTarget("minimax-official", official());
  const redirected = createCredentialTarget(
    "minimax-official",
    official({baseUrl: "https://attacker.example/v1"})
  );
  await store.saveToKeyring(original, "original-secret");

  assert.equal(
    await store.get({...redirected, fingerprint: original.fingerprint}),
    null
  );
});

test("canonical endpoint spellings produce the same credential target", () => {
  assert.equal(
    normalizeProviderEndpoint("HTTPS://API.MINIMAX.IO:443/v1/"),
    normalizeProviderEndpoint("https://api.minimax.io/v1")
  );
});

test("canonical configuration, credential scope, and transport use one endpoint", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-canonical-provider-"));
  const path = join(root, "config.json");
  const transport = new CapturingTransport();

  try {
    await writeFile(path, JSON.stringify({
      schemaVersion: 1,
      modelProvider: "minimax-official",
      modelProviders: {
        "minimax-official": {
          ...official(),
          baseUrl: "HTTPS://API.MINIMAX.IO:443/v1///",
          headers: {accept: "application/json"}
        }
      },
      model: "MiniMax-M3",
      context: {
        workingContextLimit: 1000,
        autoCompactRatio: 0.9,
        maxCompletionTokens: 100
      }
    }), "utf8");

    const config = await new ConfigManager(root).load();
    const provider = config.modelProviders["minimax-official"]!;
    const target = createCredentialTarget("minimax-official", provider);
    assert.equal(provider.baseUrl, "https://api.minimax.io/v1");
    assert.deepEqual(provider.headers, {Accept: "application/json"});
    assert.equal(target.endpoint, "https://api.minimax.io/v1");
    assert.notEqual(resolveTrustedCredentialBinding(target), undefined);

    await collectGateway(new StrictProviderGateway(transport), provider);
    assert.equal(transport.requests[0]?.url, "https://api.minimax.io/v1/responses");

    await new ConfigManager(root).save(config);
    const persisted = JSON.parse(await readFile(path, "utf8")) as AppConfig;
    assert.equal(
      persisted.modelProviders["minimax-official"]?.baseUrl,
      "https://api.minimax.io/v1"
    );
    assert.deepEqual(
      persisted.modelProviders["minimax-official"]?.headers,
      {Accept: "application/json"}
    );
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("redirected built-ins cannot read trusted environment or provider-only legacy keys", async () => {
  const keyring = new MemoryKeyring();
  keyring.values.set(
    "minimax-codex:minimax-api-key:minimax-official",
    "legacy-provider-secret"
  );
  const store = new CredentialStore({
    keyring,
    env: {MINIMAX_API_KEY: "environment-secret"}
  });
  const redirected = createCredentialTarget(
    "minimax-official",
    official({baseUrl: "https://attacker.example/v1"})
  );

  assert.equal(resolveTrustedCredentialBinding(redirected), undefined);
  assert.equal(await store.get(redirected), null);
});

test("an exact built-in target migrates its provider-only legacy key", async () => {
  const keyring = new MemoryKeyring();
  keyring.values.set(
    "minimax-codex:minimax-api-key:minimax-official",
    "legacy-provider-secret"
  );
  const store = new CredentialStore({keyring, env: {}});
  const target = createCredentialTarget("minimax-official", official());

  assert.equal(await store.get(target), "legacy-provider-secret");
  assert.equal(
    [...keyring.values.entries()].some(
      ([account, value]) => account.includes("minimax-api-key:v2:") && value === "legacy-provider-secret"
    ),
    true
  );
  assert.equal(
    keyring.values.has("minimax-codex:minimax-api-key:minimax-official"),
    false
  );
});

test("keyring migration retries legacy cleanup after scoped write succeeds", async () => {
  class FlakyDeleteKeyring extends MemoryKeyring {
    deleteAttempts = 0;

    override async deletePassword(service: string, account: string): Promise<void> {
      this.deleteAttempts += 1;
      if (this.deleteAttempts === 1) {
        throw new Error("simulated legacy cleanup failure");
      }
      await super.deletePassword(service, account);
    }
  }

  const keyring = new FlakyDeleteKeyring();
  const legacyAccount = "minimax-codex:minimax-api-key:minimax-official";
  keyring.values.set(legacyAccount, "legacy-provider-secret");
  const store = new CredentialStore({keyring, env: {}});
  const target = createCredentialTarget("minimax-official", official());

  await assert.rejects(() => store.get(target), /OS keyring access failed/i);
  assert.equal(
    [...keyring.values.entries()].some(
      ([account, value]) => account.includes("minimax-api-key:v2:") && value === "legacy-provider-secret"
    ),
    true
  );
  assert.equal(keyring.values.has(legacyAccount), true);

  assert.equal(await store.get(target), "legacy-provider-secret");
  assert.equal(keyring.values.has(legacyAccount), false);
  assert.equal(keyring.deleteAttempts, 2);
});

test("conflicting legacy keyring aliases prevent scoped migration", async () => {
  class WriteCountingKeyring extends MemoryKeyring {
    setAttempts = 0;

    override async setPassword(service: string, account: string, value: string): Promise<void> {
      this.setAttempts += 1;
      await super.setPassword(service, account, value);
    }
  }

  const keyring = new WriteCountingKeyring();
  const providerLegacy = "minimax-codex:minimax-api-key:minimax-official";
  const globalLegacy = "minimax-codex:minimax-api-key";
  const target = createCredentialTarget("minimax-official", official());
  const scopedAccount = `minimax-codex:minimax-api-key:v2:${target.fingerprint}`;
  keyring.values.set(providerLegacy, "provider-legacy-secret");
  keyring.values.set(globalLegacy, "global-legacy-secret");
  const store = new CredentialStore({keyring, env: {}});

  await assertSafeMigrationFailure(
    () => store.get(target),
    [
      "provider-legacy-secret",
      "global-legacy-secret",
      providerLegacy,
      globalLegacy
    ]
  );
  assert.equal(keyring.setAttempts, 0);
  assert.equal(keyring.values.get(providerLegacy), "provider-legacy-secret");
  assert.equal(keyring.values.get(globalLegacy), "global-legacy-secret");
  assert.equal(keyring.values.has(scopedAccount), false);
});

test("a scoped credential matching only one conflicting legacy alias fails closed", async () => {
  const keyring = new MemoryKeyring();
  const providerLegacy = "minimax-codex:minimax-api-key:minimax-official";
  const globalLegacy = "minimax-codex:minimax-api-key";
  const target = createCredentialTarget("minimax-official", official());
  const scopedAccount = `minimax-codex:minimax-api-key:v2:${target.fingerprint}`;
  keyring.values.set(providerLegacy, "provider-legacy-secret");
  keyring.values.set(globalLegacy, "global-legacy-secret");
  keyring.values.set(scopedAccount, "provider-legacy-secret");
  const store = new CredentialStore({keyring, env: {}});

  await assertSafeMigrationFailure(
    () => store.get(target),
    ["provider-legacy-secret", "global-legacy-secret"]
  );
  assert.equal(keyring.values.get(providerLegacy), "provider-legacy-secret");
  assert.equal(keyring.values.get(globalLegacy), "global-legacy-secret");
  assert.equal(keyring.values.get(scopedAccount), "provider-legacy-secret");
});

test("matching legacy keyring aliases migrate and clean up together", async () => {
  const keyring = new MemoryKeyring();
  const providerLegacy = "minimax-codex:minimax-api-key:minimax-official";
  const globalLegacy = "minimax-codex:minimax-api-key";
  const target = createCredentialTarget("minimax-official", official());
  const scopedAccount = `minimax-codex:minimax-api-key:v2:${target.fingerprint}`;
  keyring.values.set(providerLegacy, "same-legacy-secret");
  keyring.values.set(globalLegacy, "same-legacy-secret");
  const store = new CredentialStore({keyring, env: {}});

  assert.equal(await store.get(target), "same-legacy-secret");
  assert.equal(keyring.values.has(providerLegacy), false);
  assert.equal(keyring.values.has(globalLegacy), false);
  assert.equal(keyring.values.get(scopedAccount), "same-legacy-secret");
});

test("matching legacy aliases clean up an equal preexisting scoped credential", async () => {
  const keyring = new MemoryKeyring();
  const providerLegacy = "minimax-codex:minimax-api-key:minimax-official";
  const globalLegacy = "minimax-codex:minimax-api-key";
  const target = createCredentialTarget("minimax-official", official());
  const scopedAccount = `minimax-codex:minimax-api-key:v2:${target.fingerprint}`;
  keyring.values.set(providerLegacy, "same-secret");
  keyring.values.set(globalLegacy, "same-secret");
  keyring.values.set(scopedAccount, "same-secret");
  const store = new CredentialStore({keyring, env: {}});

  assert.equal(await store.get(target), "same-secret");
  assert.equal(keyring.values.has(providerLegacy), false);
  assert.equal(keyring.values.has(globalLegacy), false);
  assert.equal(keyring.values.get(scopedAccount), "same-secret");
});

test("empty legacy keyring aliases are ignored during consistency checks", async () => {
  const keyring = new MemoryKeyring();
  const providerLegacy = "minimax-codex:minimax-api-key:minimax-official";
  const globalLegacy = "minimax-codex:minimax-api-key";
  const target = createCredentialTarget("minimax-official", official());
  const scopedAccount = `minimax-codex:minimax-api-key:v2:${target.fingerprint}`;
  keyring.values.set(providerLegacy, "   ");
  keyring.values.set(globalLegacy, " usable-secret ");
  const store = new CredentialStore({keyring, env: {}});

  assert.equal(await store.get(target), "usable-secret");
  assert.equal(keyring.values.has(globalLegacy), false);
  assert.equal(keyring.values.get(scopedAccount), "usable-secret");
});

test("keyring migration rolls back a newly written mismatched scoped credential", async () => {
  class WrongWriteOnceKeyring extends MemoryKeyring {
    wrongWrites = 1;

    override async setPassword(service: string, account: string, value: string): Promise<void> {
      await super.setPassword(
        service,
        account,
        this.wrongWrites-- > 0 ? "wrong-scoped-secret" : value
      );
    }
  }

  const keyring = new WrongWriteOnceKeyring();
  const legacyAccount = "minimax-codex:minimax-api-key:minimax-official";
  const target = createCredentialTarget("minimax-official", official());
  const scopedAccount = `minimax-codex:minimax-api-key:v2:${target.fingerprint}`;
  keyring.values.set(legacyAccount, "legacy-provider-secret");
  const store = new CredentialStore({keyring, env: {}});

  await assertSafeMigrationFailure(
    () => store.get(target),
    ["legacy-provider-secret", "wrong-scoped-secret"]
  );
  assert.equal(keyring.values.get(legacyAccount), "legacy-provider-secret");
  assert.equal(keyring.values.has(scopedAccount), false);

  assert.equal(await store.get(target), "legacy-provider-secret");
  assert.equal(keyring.values.has(legacyAccount), false);
  assert.equal(keyring.values.get(scopedAccount), "legacy-provider-secret");
});

test("failed keyring migration rollback leaves a retry-safe conflict", async () => {
  class WrongWriteWithFailedRollbackKeyring extends MemoryKeyring {
    scopedDeleteAttempts = 0;

    override async setPassword(service: string, account: string): Promise<void> {
      await super.setPassword(service, account, "wrong-scoped-secret");
    }

    override async deletePassword(service: string, account: string): Promise<void> {
      if (account.includes("minimax-api-key:v2:")) {
        this.scopedDeleteAttempts += 1;
        throw new Error("simulated scoped rollback failure");
      }
      await super.deletePassword(service, account);
    }
  }

  const keyring = new WrongWriteWithFailedRollbackKeyring();
  const legacyAccount = "minimax-codex:minimax-api-key:minimax-official";
  const target = createCredentialTarget("minimax-official", official());
  const scopedAccount = `minimax-codex:minimax-api-key:v2:${target.fingerprint}`;
  keyring.values.set(legacyAccount, "legacy-provider-secret");
  const store = new CredentialStore({keyring, env: {}});

  await assertSafeMigrationFailure(
    () => store.get(target),
    ["legacy-provider-secret", "wrong-scoped-secret", "simulated scoped rollback failure"]
  );
  assert.equal(keyring.values.get(legacyAccount), "legacy-provider-secret");
  assert.equal(keyring.values.get(scopedAccount), "wrong-scoped-secret");
  assert.equal(keyring.scopedDeleteAttempts, 1);

  await assertSafeMigrationFailure(
    () => store.get(target),
    ["legacy-provider-secret", "wrong-scoped-secret"]
  );
  assert.equal(keyring.values.get(legacyAccount), "legacy-provider-secret");
  assert.equal(keyring.values.get(scopedAccount), "wrong-scoped-secret");
  assert.equal(keyring.scopedDeleteAttempts, 1);
});

test("preexisting scoped and legacy keyring mismatch fails closed without cleanup", async () => {
  const keyring = new MemoryKeyring();
  const legacyAccount = "minimax-codex:minimax-api-key:minimax-official";
  const target = createCredentialTarget("minimax-official", official());
  const scopedAccount = `minimax-codex:minimax-api-key:v2:${target.fingerprint}`;
  keyring.values.set(legacyAccount, "legacy-provider-secret");
  keyring.values.set(scopedAccount, "different-scoped-secret");
  const store = new CredentialStore({keyring, env: {}});

  await assertSafeMigrationFailure(
    () => store.get(target),
    ["legacy-provider-secret", "different-scoped-secret"]
  );
  assert.equal(keyring.values.get(legacyAccount), "legacy-provider-secret");
  assert.equal(keyring.values.get(scopedAccount), "different-scoped-secret");
});

test("preexisting scoped credential cleans matching legacy keyring entries", async () => {
  const keyring = new MemoryKeyring();
  const legacyAccount = "minimax-codex:minimax-api-key:minimax-official";
  const target = createCredentialTarget("minimax-official", official());
  const scopedAccount = `minimax-codex:minimax-api-key:v2:${target.fingerprint}`;
  keyring.values.set(legacyAccount, "same-secret");
  keyring.values.set(scopedAccount, "same-secret");
  const store = new CredentialStore({keyring, env: {}});

  assert.equal(await store.get(target), "same-secret");
  assert.equal(keyring.values.has(legacyAccount), false);
  assert.equal(keyring.values.get(scopedAccount), "same-secret");
});

for (const [readBackFailure, description] of [
  ["null", "returns null"],
  ["mismatch", "does not match"],
  ["throw", "throws"]
] as const) {
  test(`legacy keyring migration preserves legacy accounts when scoped read-back ${description}`, async () => {
    class FaultyReadBackKeyring extends MemoryKeyring {
      failReadBack = true;
      private scopedReads = 0;

      override async getPassword(service: string, account: string): Promise<string | null> {
        if (account.includes("minimax-api-key:v2:")) {
          this.scopedReads += 1;
          if (this.failReadBack && this.scopedReads > 1) {
            if (readBackFailure === "throw") {
              throw new Error("simulated scoped read-back failure");
            }
            return readBackFailure === "mismatch" ? "different-secret" : null;
          }
        }
        return super.getPassword(service, account);
      }
    }

    const keyring = new FaultyReadBackKeyring();
    const providerLegacy = "minimax-codex:minimax-api-key:minimax-official";
    const globalLegacy = "minimax-codex:minimax-api-key";
    keyring.values.set(providerLegacy, "legacy-provider-secret");
    keyring.values.set(globalLegacy, "legacy-provider-secret");
    const store = new CredentialStore({keyring, env: {}});
    const target = createCredentialTarget("minimax-official", official());

    await assert.rejects(() => store.get(target), /keyring|migration|verification/i);
    assert.equal(keyring.values.get(providerLegacy), "legacy-provider-secret");
    assert.equal(keyring.values.get(globalLegacy), "legacy-provider-secret");

    keyring.failReadBack = false;
    assert.equal(await store.get(target), "legacy-provider-secret");
    assert.equal(keyring.values.has(providerLegacy), false);
    assert.equal(keyring.values.has(globalLegacy), false);
  });
}

test("plaintext migration retries backup cleanup even when scoped credential exists", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-scoped-cleanup-retry-"));
  const path = join(root, "credentials.json");
  const target = createCredentialTarget("minimax-official", official());
  let removals = 0;
  const store = new CredentialStore({
    keyring: null,
    userConfigDir: root,
    env: {},
    removeFile: async (filePath: string) => {
      removals += 1;
      if (removals === 1) {
        throw new Error("simulated backup cleanup failure");
      }
      await rm(filePath, {force: true});
    }
  });

  try {
    await writeFile(path, JSON.stringify({
      providers: {"minimax-official": "legacy-secret"},
      targets: {[target.fingerprint]: "scoped-secret"}
    }), "utf8");
    await writeFile(`${path}.bak`, JSON.stringify({
      providers: {"minimax-official": "legacy-secret"}
    }), "utf8");

    await assert.rejects(() => store.get(target), /backup cleanup failure/i);
    assert.equal(await store.get(target), "scoped-secret");
    const persisted = await readFile(path, "utf8");
    assert.equal(persisted.includes("legacy-secret"), false);
    await assert.rejects(readFile(`${path}.bak`, "utf8"));
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("scoped plaintext fallback never crosses endpoints", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-scoped-file-"));
  const store = new CredentialStore({keyring: null, userConfigDir: root, env: {}});
  const first = createCredentialTarget("custom", {
    name: "Custom",
    baseUrl: "https://one.example/v1",
    protocol: "responses"
  });
  const second = createCredentialTarget("custom", {
    name: "Custom",
    baseUrl: "https://two.example/v1",
    protocol: "responses"
  });

  try {
    await store.saveToUserFile(first, "file-secret", store.createPlaintextConsent());
    assert.equal(await store.get(first), "file-secret");
    assert.equal(await store.get(second), null);
    const persisted = await readFile(join(root, "credentials.json"), "utf8");
    assert.equal(persisted.includes('"custom"'), false);
    assert.equal(persisted.includes("file-secret"), true);
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("provider config enforces HTTPS or explicitly enabled loopback HTTP", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-provider-policy-"));
  const path = join(root, "config.json");
  const load = async (baseUrl: string, extra: Record<string, unknown> = {}) => {
    await rm(`${path}.bak`, {force: true});
    await writeFile(path, JSON.stringify({
      modelProvider: "custom",
      modelProviders: {
        custom: {name: "Custom", baseUrl, protocol: "responses", ...extra}
      }
    }), "utf8");
    return new ConfigManager(root).load();
  };

  try {
    await assert.rejects(load("http://remote.example/v1"), /HTTPS|loopback/i);
    await assert.rejects(
      load("http://remote.example/v1", {allowInsecureLoopback: true}),
      /loopback|HTTPS/i
    );
    await assert.rejects(
      load("http://0.0.0.0:11434/v1", {allowInsecureLoopback: true}),
      /loopback|HTTPS/i
    );
    await assert.rejects(
      load("http://192.168.1.10:11434/v1", {allowInsecureLoopback: true}),
      /loopback|HTTPS/i
    );
    await assert.rejects(load("http://127.0.0.1:11434/v1"), /allowInsecureLoopback/i);
    assert.equal(
      (await load("http://127.0.0.1:11434/v1", {allowInsecureLoopback: true}))
        .modelProviders.custom?.baseUrl,
      "http://127.0.0.1:11434/v1"
    );
    await assert.rejects(load("https://user:pass@example.test/v1"), /userinfo/i);
    await assert.rejects(load("https://example.test/v1?key=value"), /query/i);
    await assert.rejects(load("https://example.test/v1#fragment"), /fragment/i);
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("provider config rejects sensitive headers case-insensitively and preserves public headers", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-provider-headers-"));
  const path = join(root, "config.json");
  const load = async (headers: Record<string, string>) => {
    await rm(`${path}.bak`, {force: true});
    await writeFile(path, JSON.stringify({
      modelProvider: "custom",
      modelProviders: {
        custom: {
          name: "Custom",
          baseUrl: "https://example.test/v1",
          protocol: "responses",
          headers
        }
      }
    }), "utf8");
    return new ConfigManager(root).load();
  };

  try {
    for (const name of [
      "aUtHoRiZaTiOn",
      "X-API-Key",
      "XApiKey",
      "Authentication",
      "Content_Type",
      "Cookie",
      "X-Session-Token",
      "X-Arbitrary-Metadata"
    ]) {
      await assert.rejects(load({[name]: "must-not-pass"}), /header.*not allowed/i);
    }
    await assert.rejects(load({"X-Bad\r\nInjected": "value"}), /header/i);
    await assert.rejects(load({"X-Title": "safe\r\nInjected: value"}), /header/i);
    await assert.rejects(load({"X-Title": "safe\0value"}), /header/i);
    await assert.rejects(load({"X-Title": "safe\u0001value"}), /header/i);
    await assert.rejects(load({"X-Title": "safe\u000bvalue"}), /header/i);
    await assert.rejects(load({"X-Title": "safe\u007fvalue"}), /header/i);
    await assert.rejects(load({"X-Title": "safe\tvalue"}), /header/i);
    const safe = await load({accept: "application/json", "openai-BETA": "responses=v1"});
    assert.deepEqual(safe.modelProviders.custom?.headers, {
      Accept: "application/json",
      "OpenAI-Beta": "responses=v1"
    });
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

class CapturingTransport implements HttpStreamTransport {
  readonly requests: HttpStreamRequest[] = [];

  async postStream(request: HttpStreamRequest): Promise<Response> {
    this.requests.push(request);
    return new Response(
      'data: {"type":"response.completed","response":{}}\n\n',
      {status: 200, headers: {"Content-Type": "text/event-stream"}}
    );
  }
}

async function collectGateway(
  gateway: StrictProviderGateway,
  provider: ModelProviderConfig
): Promise<ProviderGatewayEvent[]> {
  const events: ProviderGatewayEvent[] = [];
  for await (const event of gateway.stream({
    config: {
      schemaVersion: 1,
      modelProvider: "custom",
      modelProviders: {custom: provider},
      model: "test-model",
      context: {
        workingContextLimit: 1000,
        autoCompactRatio: 0.9,
        maxCompletionTokens: 100
      }
    },
    apiKey: "runtime-secret",
    messages: []
  })) {
    events.push(event);
  }
  return events;
}

test("gateway revalidates transport policy before invoking a supplied transport", async () => {
  const transport = new CapturingTransport();
  const gateway = new StrictProviderGateway(transport);

  await assert.rejects(
    () => collectGateway(gateway, {
      name: "Unsafe",
      baseUrl: "http://remote.example/v1",
      protocol: "responses"
    }),
    /HTTPS|loopback/i
  );
  assert.equal(transport.requests.length, 0);
});

test("gateway owns Authorization and preserves safe public headers", async () => {
  const transport = new CapturingTransport();
  const gateway = new StrictProviderGateway(transport);

  await collectGateway(gateway, {
    name: "Safe",
    baseUrl: "https://example.test/v1",
    protocol: "responses",
    headers: {"OpenAI-Beta": "responses=v1"}
  });

  assert.equal(transport.requests[0]?.headers.Authorization, "Bearer runtime-secret");
  assert.equal(transport.requests[0]?.headers["OpenAI-Beta"], "responses=v1");
  await assert.rejects(
    () => collectGateway(gateway, {
      name: "Unsafe header",
      baseUrl: "https://example.test/v1",
      protocol: "responses",
      headers: {authorization: "workspace-secret"}
    }),
    /header.*not allowed/i
  );
  assert.equal(transport.requests.length, 1);
  for (const headers of [
    {XApiKey: "workspace-secret"},
    {"X-Title": "safe\r\nAuthorization: workspace-secret"},
    {"X-Title": "safe\u000bvalue"},
    {"X-Title": "safe\u007fvalue"}
  ]) {
    await assert.rejects(
      () => collectGateway(gateway, {
        name: "Unsafe header",
        baseUrl: "https://example.test/v1",
        protocol: "responses",
        headers
      }),
      /header/i
    );
  }
  assert.equal(transport.requests.length, 1);
});

test("programmatic provider endpoints use canonical request URLs", async () => {
  for (const [provider, expectedUrl] of [
    [
      {
        name: "Programmatic HTTPS",
        baseUrl: "https://example.test/v1///",
        protocol: "responses" as const
      },
      "https://example.test/v1/responses"
    ],
    [
      {
        name: "Programmatic root",
        baseUrl: "https://example.test///",
        protocol: "responses" as const
      },
      "https://example.test/responses"
    ],
    [
      {
        name: "Programmatic loopback",
        baseUrl: "http://127.0.0.1:11434///",
        protocol: "responses" as const,
        allowInsecureLoopback: true
      },
      "http://127.0.0.1:11434/responses"
    ]
  ] as const) {
    const transport = new CapturingTransport();
    await collectGateway(new StrictProviderGateway(transport), provider);
    assert.equal(transport.requests[0]?.url, expectedUrl);
  }
});
