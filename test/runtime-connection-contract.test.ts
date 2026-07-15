import assert from "node:assert/strict";
import {access, mkdir, mkdtemp, readFile, rm, writeFile} from "node:fs/promises";
import {tmpdir} from "node:os";
import {join} from "node:path";
import test from "node:test";
import {
  CredentialStore,
  type KeyringBackend
} from "../src/config/credential-store.js";
import {ConfigManager, DEFAULT_CONFIG} from "../src/config/config-manager.js";
import type {
  ProviderGateway,
  ProviderGatewayEvent,
  ProviderRequest
} from "../src/providers/provider-gateway.js";
import type {Command, RuntimeEvent} from "../src/protocol.js";
import {ApplicationKernel} from "../src/runtime/application-kernel.js";
import {JsonlStorageProvider} from "../src/storage/jsonl-storage.js";

class OfflineProvider implements ProviderGateway {
  readonly requests: ProviderRequest[] = [];

  constructor(private readonly events: readonly ProviderGatewayEvent[]) {}

  async *stream(request: ProviderRequest): AsyncGenerator<ProviderGatewayEvent> {
    this.requests.push(request);
    for (const event of this.events) {
      yield event;
    }
  }
}

class ThrowingKeyring implements KeyringBackend {
  async getPassword(): Promise<string | null> {
    return null;
  }

  async setPassword(_service: string, _account: string, value: string): Promise<void> {
    const error = new Error(`native failure at C:/secret/keyring for ${value}`);
    Object.assign(error, {code: "EKEYRING_BROKEN"});
    throw error;
  }

  async deletePassword(): Promise<void> {}
}

class MigratingKeyring implements KeyringBackend {
  readonly values = new Map<string, string>();
  failWrite = false;

  async getPassword(service: string, account: string): Promise<string | null> {
    return this.values.get(`${service}:${account}`) ?? null;
  }

  async setPassword(service: string, account: string, value: string): Promise<void> {
    if (this.failWrite) {
      throw new Error("simulated migration write failure");
    }
    this.values.set(`${service}:${account}`, value);
  }

  async deletePassword(service: string, account: string): Promise<void> {
    this.values.delete(`${service}:${account}`);
  }
}

async function collect(
  application: ApplicationKernel,
  command: Command
): Promise<RuntimeEvent[]> {
  const events: RuntimeEvent[] = [];
  for await (const event of application.dispatch(command)) {
    events.push(event);
  }
  return events;
}

test("Command connects through the new kernel to Provider, storage, and RuntimeEvents", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-kernel-connection-"));
  const stateRoot = join(root, ".mini-codex");
  const provider = new OfflineProvider([
    {type: "text.delta", delta: "connected"},
    {type: "completed"}
  ]);
  const credentialStore = new CredentialStore({
    keyring: null,
    userConfigDir: join(root, "user-config"),
    env: {MINIMAX_API_KEY: "offline-test-key"}
  });
  const app = new ApplicationKernel({
    cwd: root,
    stateRoot,
    credentialStore,
    providerGateway: provider
  });

  try {
    const initialized = await app.init();
    assert.equal(initialized.at(-1)?.type, "runtime.ready");

    const events = await collect(app, {type: "turn.submit", input: "hello"});

    assert.equal(events.some((event) => event.type === "assistant.delta"), true);
    assert.equal(events.some((event) => event.type === "assistant.completed"), true);
    assert.equal(provider.requests.length, 1);
    assert.equal(provider.requests[0]?.apiKey, "offline-test-key");
    assert.equal(
      provider.requests[0]?.messages.some(
        (message) => message.role === "user" && message.content === "hello"
      ),
      true
    );

    const repository = new JsonlStorageProvider(stateRoot);
    const active = (await repository.listThreads()).find(
      (thread) => thread.status === "active"
    );
    assert.ok(active);
    assert.equal(
      (await repository.readThread(active.id)).items.at(-1)?.content,
      "connected"
    );
  } finally {
    await app.shutdown("user");
    await rm(root, {recursive: true, force: true});
  }
});

test("plaintext credential storage requires the routed confirmation command", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-kernel-consent-"));
  const stateRoot = join(root, ".mini-codex");
  const userConfigDir = join(root, "user-config");
  const credentialPath = join(userConfigDir, "credentials.json");
  const credentialStore = new CredentialStore({
    keyring: null,
    userConfigDir,
    env: {}
  });
  const app = new ApplicationKernel({
    cwd: root,
    stateRoot,
    credentialStore,
    providerGateway: new OfflineProvider([{type: "completed"}])
  });

  try {
    await app.init();
    const warning = await collect(app, {type: "config.api_key.request"});
    assert.deepEqual(warning, [
      {
        type: "config.api_key.plaintext_confirmation_required",
        path: credentialPath
      }
    ]);

    const secret = "offline-plaintext-secret";
    const refused = await collect(app, {
      type: "config.api_key.set",
      apiKey: secret
    });
    assert.equal(refused[0]?.type, "config.api_key.plaintext_confirmation_required");
    assert.equal(JSON.stringify(refused).includes(secret), false);
    await assert.rejects(access(credentialPath));

    const confirmed = await collect(app, {
      type: "config.api_key.plaintext.confirm"
    });
    assert.equal(confirmed[0]?.type, "config.api_key.plaintext_confirmed");
    const saved = await collect(app, {type: "config.api_key.set", apiKey: secret});
    assert.equal(saved[0]?.type, "config.api_key.saved");
    assert.equal(JSON.stringify(saved).includes(secret), false);
    assert.equal((await readFile(credentialPath, "utf8")).includes(secret), true);
  } finally {
    await app.shutdown("user");
    await rm(root, {recursive: true, force: true});
  }
});

test("credential failures are redacted at the kernel boundary", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-kernel-redaction-"));
  const secret = "kernel-secret-must-not-escape";
  const app = new ApplicationKernel({
    cwd: root,
    stateRoot: join(root, ".mini-codex"),
    credentialStore: new CredentialStore({
      keyring: new ThrowingKeyring(),
      userConfigDir: join(root, "user-config"),
      env: {}
    }),
    providerGateway: new OfflineProvider([{type: "completed"}])
  });

  try {
    await app.init();
    const events = await collect(app, {
      type: "config.api_key.set",
      apiKey: `Bearer ${secret}`
    });

    assert.equal(JSON.stringify(events).includes(secret), false);
    assert.deepEqual(events, [{
      type: "error",
      message: "OS keyring access failed. Retry or review the OS credential service."
    }]);
    assert.equal(JSON.stringify(events).includes("C:/secret/keyring"), false);
  } finally {
    await app.shutdown("user");
    await rm(root, {recursive: true, force: true});
  }
});

test("production kernel migrates a trusted legacy workspace credential", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-kernel-legacy-migrate-"));
  const stateRoot = join(root, ".mini-codex");
  const legacyPath = join(stateRoot, "secrets.local.json");
  const keyring = new MigratingKeyring();
  const app = new ApplicationKernel({
    cwd: root,
    stateRoot,
    credentialStore: new CredentialStore({
      keyring,
      userConfigDir: join(root, "user-config"),
      env: {}
    }),
    providerGateway: new OfflineProvider([{type: "completed"}])
  });

  try {
    await mkdir(stateRoot, {recursive: true});
    await writeFile(legacyPath, JSON.stringify({
      providers: {"minimax-official": "workspace-legacy-secret"}
    }), "utf8");

    const events = await app.init();
    assert.equal(events.at(-1)?.type, "runtime.ready");
    assert.equal(
      events.at(-1)?.type === "runtime.ready" && events.at(-1)?.hasApiKey,
      true
    );
    assert.equal(
      [...keyring.values.values()].includes("workspace-legacy-secret"),
      true
    );
    await assert.rejects(access(legacyPath));
  } finally {
    await app.shutdown("user");
    await rm(root, {recursive: true, force: true});
  }
});

test("workspace legacy migration ignores environment until the secret is persisted", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-kernel-env-legacy-"));
  const stateRoot = join(root, ".mini-codex");
  const legacyPath = join(stateRoot, "secrets.local.json");
  const keyring = new MigratingKeyring();
  const app = new ApplicationKernel({
    cwd: root,
    stateRoot,
    credentialStore: new CredentialStore({
      keyring,
      userConfigDir: join(root, "user-config"),
      env: {MINIMAX_API_KEY: "environment-secret"}
    })
  });

  try {
    await mkdir(stateRoot, {recursive: true});
    await writeFile(legacyPath, JSON.stringify({
      providers: {"minimax-official": "workspace-legacy-secret"}
    }), "utf8");

    const events = await app.init();
    assert.equal(events.at(-1)?.type, "runtime.ready");
    assert.equal(
      [...keyring.values.values()].includes("workspace-legacy-secret"),
      true
    );
    await assert.rejects(access(legacyPath));
  } finally {
    await app.shutdown("user");
    await rm(root, {recursive: true, force: true});
  }
});

test("unavailable keyring defers legacy migration with a secret-free RuntimeEvent", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-kernel-legacy-deferred-"));
  const stateRoot = join(root, ".mini-codex");
  const legacyPath = join(stateRoot, "secrets.local.json");
  const secret = "workspace-secret-must-not-escape";
  const app = new ApplicationKernel({
    cwd: root,
    stateRoot,
    credentialStore: new CredentialStore({
      keyring: null,
      userConfigDir: join(root, "user-config"),
      env: {}
    })
  });

  try {
    await mkdir(stateRoot, {recursive: true});
    await writeFile(legacyPath, JSON.stringify({
      providers: {"minimax-official": secret}
    }), "utf8");

    const events = await app.init();
    assert.equal(events.at(-1)?.type, "runtime.ready");
    assert.equal(
      events.some((event) => event.type === "config.legacy_credential.reentry_required"),
      true
    );
    assert.equal(JSON.stringify(events).includes(secret), false);
    const notice = events.find(
      (event) => event.type === "config.legacy_credential.reentry_required"
    ) as {path?: string; hasUsableCredential?: boolean} | undefined;
    assert.equal(notice?.path, legacyPath);
    assert.equal(notice?.hasUsableCredential, false);
    assert.equal((await readFile(legacyPath, "utf8")).includes(secret), true);
  } finally {
    await app.shutdown("user");
    await rm(root, {recursive: true, force: true});
  }
});

test("deferred migration reports an environment key without deleting the source", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-kernel-env-deferred-"));
  const stateRoot = join(root, ".mini-codex");
  const legacyPath = join(stateRoot, "secrets.local.json");
  const app = new ApplicationKernel({
    cwd: root,
    stateRoot,
    credentialStore: new CredentialStore({
      keyring: null,
      userConfigDir: join(root, "user-config"),
      env: {MINIMAX_API_KEY: "usable-environment-key"}
    })
  });

  try {
    await mkdir(stateRoot, {recursive: true});
    await writeFile(legacyPath, JSON.stringify({
      providers: {"minimax-official": "retained-legacy-secret"}
    }), "utf8");

    const events = await app.init();
    const notice = events.find(
      (event) => event.type === "config.legacy_credential.reentry_required"
    ) as {hasUsableCredential?: boolean} | undefined;
    assert.equal(notice?.hasUsableCredential, true);
    assert.equal(
      events.at(-1)?.type === "runtime.ready" && events.at(-1)?.hasApiKey,
      true
    );
    assert.equal((await readFile(legacyPath, "utf8")).includes("retained-legacy-secret"), true);
  } finally {
    await app.shutdown("user");
    await rm(root, {recursive: true, force: true});
  }
});

test("explicit YES re-entry persists a scoped key before deferred source cleanup", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-kernel-deferred-reentry-"));
  const stateRoot = join(root, ".mini-codex");
  const legacyPath = join(stateRoot, "secrets.local.json");
  const app = new ApplicationKernel({
    cwd: root,
    stateRoot,
    credentialStore: new CredentialStore({
      keyring: null,
      userConfigDir: join(root, "user-config"),
      env: {}
    })
  });

  try {
    await mkdir(stateRoot, {recursive: true});
    await writeFile(legacyPath, JSON.stringify({
      providers: {"minimax-official": "old-workspace-secret"}
    }), "utf8");
    await app.init();

    await collect(app, {type: "config.api_key.request"});
    await collect(app, {type: "config.api_key.plaintext.confirm"});
    const saved = await collect(app, {
      type: "config.api_key.set",
      apiKey: "new-explicit-secret"
    });

    assert.equal(saved[0]?.type, "config.api_key.saved");
    await assert.rejects(access(legacyPath));
    assert.equal(JSON.stringify(saved).includes("old-workspace-secret"), false);
  } finally {
    await app.shutdown("user");
    await rm(root, {recursive: true, force: true});
  }
});

test("deferred migration remains untouched when API re-entry is not saved", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-kernel-deferred-cancel-"));
  const stateRoot = join(root, ".mini-codex");
  const legacyPath = join(stateRoot, "secrets.local.json");
  const app = new ApplicationKernel({
    cwd: root,
    stateRoot,
    credentialStore: new CredentialStore({keyring: null, env: {}})
  });

  try {
    await mkdir(stateRoot, {recursive: true});
    await writeFile(legacyPath, JSON.stringify({
      providers: {"minimax-official": "retained-on-cancel"}
    }), "utf8");
    await app.init();
    await collect(app, {type: "config.api_key.request"});
    assert.equal((await readFile(legacyPath, "utf8")).includes("retained-on-cancel"), true);
  } finally {
    await app.shutdown("user");
    await rm(root, {recursive: true, force: true});
  }
});

test("a later startup automatically migrates once the keyring becomes available", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-kernel-deferred-restart-"));
  const stateRoot = join(root, ".mini-codex");
  const legacyPath = join(stateRoot, "secrets.local.json");
  const first = new ApplicationKernel({
    cwd: root,
    stateRoot,
    credentialStore: new CredentialStore({keyring: null, env: {}})
  });

  try {
    await mkdir(stateRoot, {recursive: true});
    await writeFile(legacyPath, JSON.stringify({
      providers: {"minimax-official": "restart-migration-secret"}
    }), "utf8");
    const deferred = await first.init();
    assert.equal(
      deferred.some((event) => event.type === "config.legacy_credential.reentry_required"),
      true
    );
    await first.shutdown("user");

    const keyring = new MigratingKeyring();
    const second = new ApplicationKernel({
      cwd: root,
      stateRoot,
      credentialStore: new CredentialStore({keyring, env: {}})
    });
    try {
      const migrated = await second.init();
      assert.equal(
        migrated.some((event) => event.type === "config.legacy_credential.reentry_required"),
        false
      );
      assert.equal([...keyring.values.values()].includes("restart-migration-secret"), true);
      await assert.rejects(access(legacyPath));
    } finally {
      await second.shutdown("user");
    }
  } finally {
    await first.shutdown("user");
    await rm(root, {recursive: true, force: true});
  }
});

test("production kernel fails closed and retains a legacy source when migration fails", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-kernel-legacy-failure-"));
  const stateRoot = join(root, ".mini-codex");
  const legacyPath = join(stateRoot, "secrets.local.json");
  const keyring = new MigratingKeyring();
  keyring.failWrite = true;
  const app = new ApplicationKernel({
    cwd: root,
    stateRoot,
    credentialStore: new CredentialStore({
      keyring,
      userConfigDir: join(root, "user-config"),
      env: {}
    })
  });

  try {
    await mkdir(stateRoot, {recursive: true});
    await writeFile(legacyPath, JSON.stringify({
      providers: {"minimax-official": "must-remain"}
    }), "utf8");

    await assert.rejects(
      () => app.init(),
      /OS keyring access failed\. Retry or review the OS credential service\./
    );
    assert.equal((await readFile(legacyPath, "utf8")).includes("must-remain"), true);
  } finally {
    await app.shutdown("user");
    await rm(root, {recursive: true, force: true});
  }
});

test("production kernel does not inspect a custom provider legacy workspace secret", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-kernel-custom-legacy-"));
  const stateRoot = join(root, ".mini-codex");
  const legacyPath = join(stateRoot, "secrets.local.json");
  const keyring = new MigratingKeyring();
  const config = {
    ...DEFAULT_CONFIG,
    modelProvider: "custom",
    modelProviders: {
      ...DEFAULT_CONFIG.modelProviders,
      custom: {
        name: "Custom",
        baseUrl: "https://custom.example/v1",
        protocol: "responses" as const
      }
    }
  };
  const app = new ApplicationKernel({
    cwd: root,
    stateRoot,
    credentialStore: new CredentialStore({
      keyring,
      userConfigDir: join(root, "user-config"),
      env: {}
    })
  });

  try {
    await new ConfigManager(stateRoot).save(config);
    await writeFile(legacyPath, JSON.stringify({
      providers: {custom: "custom-legacy-secret"}
    }), "utf8");

    const events = await app.init();
    assert.equal(
      events.at(-1)?.type === "runtime.ready" && events.at(-1)?.hasApiKey,
      false
    );
    assert.equal((await readFile(legacyPath, "utf8")).includes("custom-legacy-secret"), true);
    assert.equal(keyring.values.size, 0);
  } finally {
    await app.shutdown("user");
    await rm(root, {recursive: true, force: true});
  }
});
