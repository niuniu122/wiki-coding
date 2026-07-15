import assert from "node:assert/strict";
import {access, mkdtemp, readFile, rm} from "node:fs/promises";
import {tmpdir} from "node:os";
import {join} from "node:path";
import test from "node:test";
import {BUILT_IN_MODEL_PROVIDERS, ConfigManager} from "../src/config/config-manager.js";
import {
  CredentialStore,
  KeyringAccessError,
  KeyringUnavailableError,
  NapiKeyringBackend,
  normalizeKeyringAccessError,
  type KeyringBackend,
  type KeyringFailureKind
} from "../src/config/credential-store.js";
import {createCredentialTarget} from "../src/config/provider-security.js";
import {ProviderService} from "../src/runtime/provider-service.js";
import type {Command, RuntimeEvent} from "../src/protocol.js";
import {ApplicationKernel} from "../src/runtime/application-kernel.js";

const target = createCredentialTarget(
  "minimax-official",
  BUILT_IN_MODEL_PROVIDERS["minimax-official"]!
);

const SAFE_MESSAGES: Record<KeyringFailureKind, string> = {
  unavailable: "OS keyring is unavailable.",
  locked: "OS keyring is locked. Unlock it and retry.",
  denied: "OS keyring access was denied. Review OS credential permissions and retry.",
  unknown: "OS keyring access failed. Retry or review the OS credential service."
};

class FailingKeyring implements KeyringBackend {
  readCalls = 0;
  writeCalls = 0;

  constructor(
    private readonly readFailure: unknown | null,
    private readonly writeFailure: unknown | null = null
  ) {}

  async getPassword(): Promise<string | null> {
    this.readCalls += 1;
    if (this.readFailure !== null) {
      throw this.readFailure;
    }
    return null;
  }

  async setPassword(): Promise<void> {
    this.writeCalls += 1;
    if (this.writeFailure !== null) {
      throw this.writeFailure;
    }
  }

  async deletePassword(): Promise<void> {}
}

function nativeFailure(kind: KeyringFailureKind, marker: string): Error {
  const error = new Error(`${marker} native path C:/secret/keyring`);
  if (kind === "unavailable") {
    Object.assign(error, {code: "ECONNREFUSED"});
  } else if (kind === "locked") {
    Object.assign(error, {code: "ERR_KEYRING_LOCKED"});
  } else if (kind === "denied") {
    Object.assign(error, {code: "EACCES"});
  }
  return error;
}

const DBUS_ABSENCE_MESSAGES = [
  "org.freedesktop.DBus.Error.ServiceUnknown",
  "org.freedesktop.DBus.Error.NoServer",
  "The name org.freedesktop.secrets was not provided by any .service files",
  "Cannot autolaunch D-Bus without X11 $DISPLAY"
] as const;

function hostileIntrospectionFailure(marker: string): unknown {
  const target = Object.create(null, {
    code: {
      get() {
        throw new Error(`${marker} getter C:/secret/keyring`);
      }
    }
  });
  return new Proxy(target, {
    getPrototypeOf() {
      throw new Error(`${marker} prototype C:/secret/keyring`);
    },
    get(object, property, receiver) {
      if (property === "name" || property === "message") {
        throw new Error(`${marker} proxy C:/secret/keyring`);
      }
      return Reflect.get(object, property, receiver);
    }
  });
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

test("classifier preserves typed failures and prioritizes native code and name", () => {
  const typed = new KeyringAccessError("locked", "read");
  assert.equal(normalizeKeyringAccessError(typed, "write"), typed);

  const deniedDespiteMessage = Object.assign(new Error("keyring is locked"), {
    code: "EPERM"
  });
  assert.equal(normalizeKeyringAccessError(deniedDespiteMessage, "read").kind, "denied");
  assert.equal(
    normalizeKeyringAccessError(Object.assign(new Error("opaque"), {name: "NotAllowedError"}), "write").kind,
    "denied"
  );
  assert.equal(
    normalizeKeyringAccessError(Object.assign(new Error("opaque"), {name: "KeychainLockedError"}), "read").kind,
    "locked"
  );
  assert.equal(
    normalizeKeyringAccessError(Object.assign(new Error("opaque"), {code: "ENOENT"}), "load").kind,
    "unavailable"
  );
  assert.equal(
    normalizeKeyringAccessError(
      Object.assign(new Error("opaque"), {
        name: "DBusError",
        code: "org.freedesktop.DBus.Error.ServiceUnknown"
      }),
      "load"
    ).kind,
    "unavailable"
  );
  assert.equal(normalizeKeyringAccessError(new Error("opaque"), "read").kind, "unknown");
});

test("legacy unavailable error remains compatible with the typed taxonomy", () => {
  const error = new KeyringUnavailableError("write");
  assert.equal(error instanceof KeyringAccessError, true);
  assert.equal(error.kind, "unavailable");
  assert.equal(error.operation, "write");
  assert.equal(error.message, SAFE_MESSAGES.unavailable);
});

test("specific Linux DBus and Secret Service absence messages are unavailable", () => {
  for (const message of DBUS_ABSENCE_MESSAGES) {
    const normalized = normalizeKeyringAccessError(new Error(message), "read");
    assert.equal(normalized.kind, "unavailable", message);
    assert.equal(normalized.message, SAFE_MESSAGES.unavailable);
  }
  assert.equal(
    normalizeKeyringAccessError(new Error("unrelated service name failed"), "read").kind,
    "unknown"
  );
});

test("hostile instanceof and field introspection failures become fixed unknown errors", () => {
  const marker = "hostile-introspection-secret";
  const normalized = normalizeKeyringAccessError(
    hostileIntrospectionFailure(marker),
    "write"
  );

  assert.equal(normalized.kind, "unknown");
  assert.equal(normalized.operation, "write");
  assert.equal(normalized.message, SAFE_MESSAGES.unknown);
  assert.equal(normalized.message.includes(marker), false);
  assert.equal(normalized.message.includes("C:/secret/keyring"), false);
});

for (const kind of ["unavailable", "locked", "denied", "unknown"] as const) {
  test(`native adapter normalizes ${kind} read, write, and delete failures`, async () => {
    const marker = `native-${kind}-must-not-escape`;
    class ThrowingNativeEntry {
      constructor(_service: string, _account: string) {}
      getPassword(): string | null {
        throw nativeFailure(kind, marker);
      }
      setPassword(_value: string): void {
        throw nativeFailure(kind, marker);
      }
      deletePassword(): void {
        throw nativeFailure(kind, marker);
      }
    }
    const backend = new NapiKeyringBackend(ThrowingNativeEntry);

    for (const [operation, invoke] of [
      ["read", () => backend.getPassword("service", "account")],
      ["write", () => backend.setPassword("service", "account", "secret")],
      ["write", () => backend.deletePassword("service", "account")]
    ] as const) {
      await assert.rejects(invoke, (error: unknown) => {
        assert.equal(error instanceof KeyringAccessError, true);
        assert.equal((error as KeyringAccessError).kind, kind);
        assert.equal((error as KeyringAccessError).operation, operation);
        assert.equal((error as Error).message, SAFE_MESSAGES[kind]);
        assert.equal((error as Error).message.includes(marker), false);
        return true;
      });
    }
  });
}

test("message-only DBus absence falls through injected reads to scoped user-file", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-dbus-read-unavailable-"));
  const userConfigDir = join(root, "user-config");
  const fileStore = new CredentialStore({keyring: null, userConfigDir, env: {}});

  try {
    await fileStore.saveToUserFile(target, "scoped-file-secret", fileStore.createPlaintextConsent());
    const store = new CredentialStore({
      keyring: new FailingKeyring(new Error(DBUS_ABSENCE_MESSAGES[0])),
      userConfigDir,
      env: {}
    });
    assert.equal(await store.get(target), "scoped-file-secret");
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("message-only DBus absence permits only the existing YES write fallback", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-dbus-write-unavailable-"));
  const userConfigDir = join(root, "user-config");
  const store = new CredentialStore({
    keyring: new FailingKeyring(null, new Error(DBUS_ABSENCE_MESSAGES[1])),
    userConfigDir,
    env: {}
  });
  const service = new ProviderService(new ConfigManager(root), store);

  try {
    await service.init();
    assert.equal(await service.saveApiKey("new-secret"), "unavailable");
    await assert.rejects(access(join(userConfigDir, "credentials.json")));
    assert.equal(
      await service.saveApiKey("new-secret", store.createPlaintextConsent()),
      "user-file"
    );
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("message-only native DBus read and write failures are normalized as unavailable", async () => {
  class MessageOnlyNativeEntry {
    constructor(_service: string, _account: string) {}
    getPassword(): string | null {
      throw new Error(DBUS_ABSENCE_MESSAGES[2]);
    }
    setPassword(_value: string): void {
      throw new Error(DBUS_ABSENCE_MESSAGES[3]);
    }
    deletePassword(): void {}
  }
  const backend = new NapiKeyringBackend(MessageOnlyNativeEntry);

  await assert.rejects(
    () => backend.getPassword("service", "account"),
    (error: unknown) => error instanceof KeyringAccessError && error.kind === "unavailable"
  );
  await assert.rejects(
    () => backend.setPassword("service", "account", "secret"),
    (error: unknown) => error instanceof KeyringAccessError && error.kind === "unavailable"
  );
});

test("environment priority never touches a failing keyring", async () => {
  const keyring = new FailingKeyring(nativeFailure("locked", "must-not-run"));
  const store = new CredentialStore({
    keyring,
    env: {MINIMAX_API_KEY: "environment-secret"}
  });

  assert.equal(await store.get(target), "environment-secret");
  assert.equal((await store.inspect(target)).backend, "environment");
  assert.equal(keyring.readCalls, 0);
});

test("an unavailable read continues to an existing scoped user-file credential", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-keyring-read-unavailable-"));
  const userConfigDir = join(root, "user-config");
  const fileStore = new CredentialStore({keyring: null, userConfigDir, env: {}});

  try {
    await fileStore.saveToUserFile(target, "scoped-file-secret", fileStore.createPlaintextConsent());
    const store = new CredentialStore({
      keyring: new FailingKeyring(nativeFailure("unavailable", "read-raw")),
      userConfigDir,
      env: {}
    });

    assert.equal(await store.get(target), "scoped-file-secret");
    assert.equal((await store.inspect(target)).backend, "user-file");
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

for (const kind of ["locked", "denied", "unknown"] as const) {
  test(`${kind} read fails closed without reading or modifying plaintext`, async () => {
    const root = await mkdtemp(join(tmpdir(), `minimax-keyring-read-${kind}-`));
    const userConfigDir = join(root, "user-config");
    const fileStore = new CredentialStore({keyring: null, userConfigDir, env: {}});

    try {
      await fileStore.saveToUserFile(target, "existing-file-secret", fileStore.createPlaintextConsent());
      const credentialPath = join(userConfigDir, "credentials.json");
      const before = await readFile(credentialPath, "utf8");
      const marker = `read-${kind}-must-not-escape`;
      const store = new CredentialStore({
        keyring: new FailingKeyring(nativeFailure(kind, marker)),
        userConfigDir,
        env: {}
      });

      await assert.rejects(() => store.get(target), (error: unknown) => {
        assert.equal(error instanceof KeyringAccessError, true);
        assert.equal((error as KeyringAccessError).kind, kind);
        assert.equal((error as Error).message, SAFE_MESSAGES[kind]);
        assert.equal((error as Error).message.includes(marker), false);
        return true;
      });
      assert.equal(await readFile(credentialPath, "utf8"), before);
    } finally {
      await rm(root, {recursive: true, force: true});
    }
  });
}

test("hostile injected write errors stay safe through ProviderService and Kernel", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-hostile-keyring-error-"));
  const stateRoot = join(root, ".mini-codex");
  const userConfigDir = join(root, "user-config");
  const credentialPath = join(userConfigDir, "credentials.json");
  const fileStore = new CredentialStore({keyring: null, userConfigDir, env: {}});
  const marker = "hostile-native-secret";

  try {
    await fileStore.saveToUserFile(target, "existing-file-secret", fileStore.createPlaintextConsent());
    const before = await readFile(credentialPath, "utf8");

    const serviceStore = new CredentialStore({
      keyring: new FailingKeyring(null, hostileIntrospectionFailure(marker)),
      userConfigDir,
      env: {}
    });
    const service = new ProviderService(new ConfigManager(join(root, "service")), serviceStore);
    await service.init();
    await assert.rejects(
      () => service.saveApiKey("provider-secret", serviceStore.createPlaintextConsent()),
      (error: unknown) =>
        error instanceof KeyringAccessError &&
        error.kind === "unknown" &&
        error.message === SAFE_MESSAGES.unknown
    );
    assert.equal(await readFile(credentialPath, "utf8"), before);

    const app = new ApplicationKernel({
      cwd: root,
      stateRoot,
      credentialStore: new CredentialStore({
        keyring: new FailingKeyring(null, hostileIntrospectionFailure(marker)),
        userConfigDir,
        env: {}
      })
    });
    try {
      await app.init();
      const events = await collect(app, {
        type: "config.api_key.set",
        apiKey: "kernel-secret"
      });
      assert.deepEqual(events, [{type: "error", message: SAFE_MESSAGES.unknown}]);
      assert.equal(JSON.stringify(events).includes(marker), false);
      assert.equal(JSON.stringify(events).includes("C:/secret/keyring"), false);
      assert.equal(JSON.stringify(events).includes("kernel-secret"), false);
      assert.equal(await readFile(credentialPath, "utf8"), before);
    } finally {
      await app.shutdown("user");
    }
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("only unavailable writes enter the explicit consent fallback", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-keyring-write-unavailable-"));
  const userConfigDir = join(root, "user-config");
  const store = new CredentialStore({
    keyring: new FailingKeyring(null, nativeFailure("unavailable", "write-raw")),
    userConfigDir,
    env: {}
  });
  const service = new ProviderService(new ConfigManager(root), store);

  try {
    await service.init();
    assert.equal(await service.saveApiKey("new-secret"), "unavailable");
    await assert.rejects(access(join(userConfigDir, "credentials.json")));
    assert.equal(
      await service.saveApiKey("new-secret", store.createPlaintextConsent()),
      "user-file"
    );
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

for (const kind of ["locked", "denied", "unknown"] as const) {
  test(`${kind} write rejects consent fallback and preserves plaintext bytes`, async () => {
    const root = await mkdtemp(join(tmpdir(), `minimax-keyring-write-${kind}-`));
    const userConfigDir = join(root, "user-config");
    const fileStore = new CredentialStore({keyring: null, userConfigDir, env: {}});

    try {
      await fileStore.saveToUserFile(target, "existing-file-secret", fileStore.createPlaintextConsent());
      const credentialPath = join(userConfigDir, "credentials.json");
      const before = await readFile(credentialPath, "utf8");
      const marker = `write-${kind}-must-not-escape`;
      const store = new CredentialStore({
        keyring: new FailingKeyring(null, nativeFailure(kind, marker)),
        userConfigDir,
        env: {}
      });
      const service = new ProviderService(new ConfigManager(root), store);
      await service.init();

      await assert.rejects(
        () => service.saveApiKey("must-not-be-written", store.createPlaintextConsent()),
        (error: unknown) => {
          assert.equal(error instanceof KeyringAccessError, true);
          assert.equal((error as KeyringAccessError).kind, kind);
          assert.equal((error as Error).message, SAFE_MESSAGES[kind]);
          assert.equal((error as Error).message.includes(marker), false);
          return true;
        }
      );
      assert.equal(await readFile(credentialPath, "utf8"), before);
    } finally {
      await rm(root, {recursive: true, force: true});
    }
  });
}
