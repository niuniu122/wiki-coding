import {join, resolve} from "node:path";
import {rm} from "node:fs/promises";
import {readJsonFile, writeJsonFile} from "../utils/jsonl.js";
import {
  credentialTargetFingerprint,
  resolveTrustedCredentialBinding,
  type CredentialTarget
} from "./provider-security.js";
import {resolveUserConfigRoot} from "./user-config-root.js";

export type CredentialBackend =
  | "environment"
  | "os-keyring"
  | "user-file"
  | "unavailable";

export interface CredentialStatus {
  backend: CredentialBackend;
  hasCredential: boolean;
  userFilePath: string;
}

export type KeyringFailureKind = "unavailable" | "locked" | "denied" | "unknown";
export type KeyringOperation = "load" | "read" | "write";

export interface KeyringBackend {
  getPassword(service: string, account: string): Promise<string | null>;
  setPassword(service: string, account: string, value: string): Promise<void>;
  deletePassword(service: string, account: string): Promise<void>;
}

export interface CredentialStoreOptions {
  userConfigDir?: string;
  keyring?: KeyringBackend | null;
  env?: Readonly<Record<string, string | undefined>>;
  removeFile?: (path: string) => Promise<void>;
}

export type LegacyCredentialMigrationResult =
  | {status: "none"}
  | {status: "migrated"}
  | {
      status: "reentry_required";
      path: string;
      hasUsableCredential: boolean;
    };

interface CredentialFile {
  minimaxApiKey?: string;
  providers?: Record<string, string>;
  targets?: Record<string, string>;
}

interface NativeEntry {
  getPassword(): string | null | Promise<string | null>;
  setPassword(value: string): void | Promise<void>;
  deletePassword(): unknown | Promise<unknown>;
}

type NativeEntryConstructor = new (service: string, account: string) => NativeEntry;

const SERVICE = "minimax-codex";
const LEGACY_ACCOUNT = "minimax-api-key";
const CONSENT_ISSUER = Symbol("PlaintextConsentIssuer");

export class PlaintextConsent {
  #used = false;

  constructor(issuer: typeof CONSENT_ISSUER) {
    if (issuer !== CONSENT_ISSUER) {
      throw new Error("Plaintext credential consent must be created by CredentialStore.");
    }
  }

  consume(): void {
    if (this.#used) {
      throw new Error("Plaintext credential consent was already used.");
    }
    this.#used = true;
  }
}

const KEYRING_ERROR_MESSAGES: Record<KeyringFailureKind, string> = {
  unavailable: "OS keyring is unavailable.",
  locked: "OS keyring is locked. Unlock it and retry.",
  denied: "OS keyring access was denied. Review OS credential permissions and retry.",
  unknown: "OS keyring access failed. Retry or review the OS credential service."
};

export class KeyringAccessError extends Error {
  constructor(
    readonly kind: KeyringFailureKind,
    readonly operation: KeyringOperation,
    options: {cause?: unknown} = {}
  ) {
    super(KEYRING_ERROR_MESSAGES[kind], options.cause === undefined ? {} : {cause: options.cause});
    this.name = "KeyringAccessError";
  }
}

export class KeyringUnavailableError extends KeyringAccessError {
  constructor(operation: KeyringOperation = "load", options: {cause?: unknown} = {}) {
    super("unavailable", operation, options);
    this.name = "KeyringUnavailableError";
  }
}

export class CredentialStore {
  private readonly userConfigDir: string;
  private readonly env: Readonly<Record<string, string | undefined>>;
  private readonly keyringOverride: KeyringBackend | null | undefined;
  private readonly removeFile: (path: string) => Promise<void>;
  private keyringPromise: Promise<KeyringBackend | null> | undefined;

  constructor(options: CredentialStoreOptions = {}) {
    this.userConfigDir = resolve(options.userConfigDir ?? resolveUserConfigRoot());
    this.env = options.env ?? process.env;
    this.keyringOverride = options.keyring;
    this.removeFile = options.removeFile ?? ((path) => rm(path, {force: true}));
  }

  createPlaintextConsent(): PlaintextConsent {
    return new PlaintextConsent(CONSENT_ISSUER);
  }

  async inspect(target: CredentialTarget): Promise<CredentialStatus> {
    if (this.readEnvironment(target)) {
      return this.status("environment", true);
    }

    const {keyring, value: keyringValue} = await this.readAvailableKeyring(target);
    const userFileValue = await this.readUserFileCredential(target, !keyringValue);
    if (keyringValue) {
      return this.status("os-keyring", true);
    }

    if (userFileValue) {
      return this.status("user-file", true);
    }

    return this.status(keyring ? "os-keyring" : "unavailable", false);
  }

  async get(target: CredentialTarget): Promise<string | null> {
    const environment = this.readEnvironment(target);
    if (environment) {
      return environment;
    }

    const {value: keyringValue} = await this.readAvailableKeyring(target);
    const userFileValue = await this.readUserFileCredential(target, !keyringValue);
    return keyringValue ?? userFileValue;
  }

  async peek(target: CredentialTarget): Promise<string | null> {
    const environment = this.readEnvironment(target);
    if (environment) {
      return environment;
    }

    let keyringValue: string | null = null;
    try {
      const keyring = await this.loadKeyringFor("read");
      keyringValue = keyring
        ? await this.readKeyringCredentialReadOnly(keyring, target)
        : null;
    } catch (error) {
      const normalized = normalizeKeyringAccessError(error, "read");
      if (normalized.kind !== "unavailable") {
        throw normalized;
      }
    }
    return keyringValue ?? this.readUserFileCredentialReadOnly(target);
  }

  async saveToKeyring(target: CredentialTarget, value: string): Promise<void> {
    const normalized = requireCredential(value);
    const keyring = await this.loadKeyringFor("write");
    if (!keyring) {
      throw new KeyringUnavailableError("write");
    }
    await callKeyring("write", () =>
      keyring.setPassword(SERVICE, scopedAccount(target), normalized)
    );
  }

  async saveToUserFile(
    target: CredentialTarget,
    value: string,
    consent?: PlaintextConsent
  ): Promise<void> {
    const normalized = requireCredential(value);
    requireConsent(consent).consume();
    const current = await this.readUserFile();
    await this.writeCredentialFile({
      ...current,
      targets: {...current.targets, [credentialTargetFingerprint(target)]: normalized}
    });
  }

  async migrateLegacyWorkspaceCredential(
    target: CredentialTarget,
    legacyPath: string
  ): Promise<LegacyCredentialMigrationResult> {
    const binding = resolveTrustedCredentialBinding(target);
    if (!binding) {
      return {status: "none"};
    }
    const legacyFile = await readJsonFile<CredentialFile>(legacyPath, {}, {
      validate: isCredentialFile
    });
    const legacy = readLegacyProviderSecret(legacyFile, binding.legacyProviderId);
    if (!legacy) {
      await this.cleanupLegacyFile(legacyPath, legacyFile, binding.legacyProviderId);
      return {status: "none"};
    }

    const persisted = await this.readPersistedCredential(target);
    if (!persisted.value) {
      if (!persisted.keyring) {
        return {
          status: "reentry_required",
          path: resolve(legacyPath),
          hasUsableCredential: Boolean(this.readEnvironment(target))
        };
      }
      await callKeyring("write", () =>
        persisted.keyring!.setPassword(SERVICE, scopedAccount(target), legacy)
      );
      const verified = normalizeStoredCredential(
        await callKeyring("read", () =>
          persisted.keyring!.getPassword(SERVICE, scopedAccount(target))
        )
      );
      if (verified !== legacy) {
        throw new Error("Scoped keyring credential verification failed during migration.");
      }
    }
    await this.cleanupLegacyFile(legacyPath, legacyFile, binding.legacyProviderId);
    return {status: "migrated"};
  }

  get plaintextPath(): string {
    return join(this.userConfigDir, "credentials.json");
  }

  get configRoot(): string {
    return this.userConfigDir;
  }

  private status(backend: CredentialBackend, hasCredential: boolean): CredentialStatus {
    return {backend, hasCredential, userFilePath: this.plaintextPath};
  }

  private readEnvironment(target: CredentialTarget): string | null {
    const binding = resolveTrustedCredentialBinding(target);
    if (!binding) {
      return null;
    }
    const value = this.env[binding.environmentKey];
    return value ? normalizeApiKey(value) || null : null;
  }

  private async readPersistedCredential(
    target: CredentialTarget
  ): Promise<{value: string | null; keyring: KeyringBackend | null}> {
    const {keyring, value: keyringValue} = await this.readAvailableKeyring(target);
    const userFileValue = await this.readUserFileCredential(target, !keyringValue);
    return {value: keyringValue ?? userFileValue, keyring};
  }

  private async readKeyringCredential(
    keyring: KeyringBackend,
    target: CredentialTarget
  ): Promise<string | null> {
    const scoped = normalizeStoredCredential(
      await callKeyring("read", () =>
        keyring.getPassword(SERVICE, scopedAccount(target))
      )
    );
    const binding = resolveTrustedCredentialBinding(target);
    if (!binding) {
      return scoped;
    }
    const {entries: legacyEntries, value: legacy} = normalizeLegacyKeyringEntries(
      await readLegacyKeyringEntries(keyring, binding.legacyProviderId)
    );
    let verifiedScoped = scoped;
    if (!verifiedScoped && legacy) {
      await callKeyring("write", () =>
        keyring.setPassword(SERVICE, scopedAccount(target), legacy)
      );
      try {
        verifiedScoped = normalizeStoredCredential(
          await callKeyring("read", () =>
            keyring.getPassword(SERVICE, scopedAccount(target))
          )
        );
      } catch {
        await rollbackScopedKeyringCredential(keyring, target);
        throw keyringMigrationFailure();
      }
      if (verifiedScoped !== legacy) {
        await rollbackScopedKeyringCredential(keyring, target);
        throw keyringMigrationFailure();
      }
    }
    if (legacy && verifiedScoped !== legacy) {
      throw keyringMigrationFailure();
    }
    for (const entry of legacyEntries) {
      if (entry.value) {
        await callKeyring("write", () =>
          keyring.deletePassword(SERVICE, entry.account)
        );
      }
    }
    return verifiedScoped ?? legacy;
  }

  private async readKeyringCredentialReadOnly(
    keyring: KeyringBackend,
    target: CredentialTarget
  ): Promise<string | null> {
    const scoped = normalizeStoredCredential(
      await callKeyring("read", () =>
        keyring.getPassword(SERVICE, scopedAccount(target))
      )
    );
    const binding = resolveTrustedCredentialBinding(target);
    if (!binding) {
      return scoped;
    }
    const legacy = normalizeLegacyKeyringEntries(
      await readLegacyKeyringEntries(keyring, binding.legacyProviderId)
    ).value;
    if (scoped && legacy && scoped !== legacy) {
      throw keyringMigrationFailure();
    }
    return scoped ?? legacy;
  }

  private async readUserFileCredential(
    target: CredentialTarget,
    migrateLegacy: boolean
  ): Promise<string | null> {
    const file = await this.readUserFile();
    const fingerprint = credentialTargetFingerprint(target);
    const scoped = normalizeStoredCredential(file.targets?.[fingerprint]);
    const binding = resolveTrustedCredentialBinding(target);
    if (!binding) {
      return scoped;
    }
    const legacy = readLegacyProviderSecret(file, binding.legacyProviderId);
    const providers = {...file.providers};
    delete providers[binding.legacyProviderId];
    const hadLegacyEntry =
      Object.hasOwn(file.providers ?? {}, binding.legacyProviderId) ||
      (binding.legacyProviderId === "minimax-official" && file.minimaxApiKey !== undefined);
    if (hadLegacyEntry) {
      const targets = {...file.targets};
      if (!scoped && legacy && migrateLegacy) {
        targets[fingerprint] = legacy;
      }
      await this.writeCredentialFile({
        ...(binding.legacyProviderId !== "minimax-official" && file.minimaxApiKey
          ? {minimaxApiKey: file.minimaxApiKey}
          : {}),
        ...(Object.keys(providers).length > 0 ? {providers} : {}),
        ...(Object.keys(targets).length > 0 ? {targets} : {})
      });
    } else {
      await this.removeFile(`${this.plaintextPath}.bak`);
    }
    return scoped ?? (migrateLegacy ? legacy : null);
  }

  private async readUserFileCredentialReadOnly(
    target: CredentialTarget
  ): Promise<string | null> {
    const file = await this.readUserFile();
    const scoped = normalizeStoredCredential(
      file.targets?.[credentialTargetFingerprint(target)]
    );
    if (scoped) {
      return scoped;
    }
    const binding = resolveTrustedCredentialBinding(target);
    return binding
      ? readLegacyProviderSecret(file, binding.legacyProviderId)
      : null;
  }

  private async readUserFile(): Promise<CredentialFile> {
    return readJsonFile<CredentialFile>(this.plaintextPath, {}, {
      validate: isCredentialFile
    });
  }

  private async writeCredentialFile(file: CredentialFile): Promise<void> {
    await writeJsonFile(this.plaintextPath, file, {mode: 0o600, backup: false});
    await this.removeFile(`${this.plaintextPath}.bak`);
  }

  private async cleanupLegacyFile(
    path: string,
    file: CredentialFile,
    providerId: string
  ): Promise<void> {
    const providers = {...file.providers};
    const hadLegacyEntry =
      Object.hasOwn(providers, providerId) ||
      (providerId === "minimax-official" && file.minimaxApiKey !== undefined);
    delete providers[providerId];
    if (hadLegacyEntry) {
      const remaining: CredentialFile = {
        ...(providerId !== "minimax-official" && file.minimaxApiKey
          ? {minimaxApiKey: file.minimaxApiKey}
          : {}),
        ...(Object.keys(providers).length > 0 ? {providers} : {})
      };
      if (Object.keys(remaining).length === 0) {
        await this.removeFile(path);
      } else {
        await writeJsonFile(path, remaining, {mode: 0o600, backup: false});
      }
    }
    await this.removeFile(`${path}.bak`);
  }

  private async readAvailableKeyring(
    target: CredentialTarget
  ): Promise<{keyring: KeyringBackend | null; value: string | null}> {
    try {
      const keyring = await this.loadKeyringFor("read");
      return {
        keyring,
        value: keyring ? await this.readKeyringCredential(keyring, target) : null
      };
    } catch (error) {
      const normalized = normalizeKeyringAccessError(error, "read");
      if (normalized.kind === "unavailable") {
        return {keyring: null, value: null};
      }
      throw normalized;
    }
  }

  private async loadKeyringFor(operation: KeyringOperation): Promise<KeyringBackend | null> {
    try {
      return await this.loadKeyring();
    } catch (error) {
      throw normalizeKeyringAccessError(error, operation);
    }
  }

  private loadKeyring(): Promise<KeyringBackend | null> {
    if (!this.keyringPromise) {
      this.keyringPromise =
        this.keyringOverride === undefined
          ? tryLoadNativeKeyring()
          : Promise.resolve(this.keyringOverride);
    }
    return this.keyringPromise;
  }
}

export class NapiKeyringBackend implements KeyringBackend {
  constructor(private readonly Entry: NativeEntryConstructor) {}

  async getPassword(service: string, account: string): Promise<string | null> {
    return callKeyring("read", () =>
      Promise.resolve(new this.Entry(service, account).getPassword())
    );
  }

  async setPassword(service: string, account: string, value: string): Promise<void> {
    await callKeyring("write", () =>
      Promise.resolve(new this.Entry(service, account).setPassword(value))
    );
  }

  async deletePassword(service: string, account: string): Promise<void> {
    await callKeyring("write", () =>
      Promise.resolve(new this.Entry(service, account).deletePassword()).then(() => undefined)
    );
  }
}

async function tryLoadNativeKeyring(): Promise<KeyringBackend | null> {
  try {
    const dynamicImport = new Function("specifier", "return import(specifier)") as (
      specifier: string
    ) => Promise<unknown>;
    const loaded = await dynamicImport("@napi-rs/keyring");
    const module = loaded as {
      Entry?: NativeEntryConstructor;
      default?: {Entry?: NativeEntryConstructor} | NativeEntryConstructor;
    };
    const defaultEntry =
      typeof module.default === "function" ? module.default : module.default?.Entry;
    const Entry = module.Entry ?? defaultEntry;
    if (typeof Entry !== "function") {
      return null;
    }
    return new NapiKeyringBackend(Entry);
  } catch (error) {
    const normalized = normalizeKeyringAccessError(error, "load");
    if (normalized.kind === "unavailable") {
      return null;
    }
    throw normalized;
  }
}

async function readLegacyKeyringEntries(
  keyring: KeyringBackend,
  providerId: string
): Promise<Array<{account: string; value: string | null}>> {
  const accounts = [legacyProviderAccount(providerId)];
  if (providerId === "minimax-official") {
    accounts.push(LEGACY_ACCOUNT);
  }
  return Promise.all(
    accounts.map(async (account) => ({
      account,
      value: await callKeyring("read", () => keyring.getPassword(SERVICE, account))
    }))
  );
}

function normalizeLegacyKeyringEntries(
  entries: Array<{account: string; value: string | null}>
): {entries: Array<{account: string; value: string | null}>; value: string | null} {
  const normalizedEntries = entries.map((entry) => ({
    ...entry,
    value: normalizeStoredCredential(entry.value)
  }));
  const values = new Set(
    normalizedEntries.flatMap((entry) => entry.value ? [entry.value] : [])
  );
  if (values.size > 1) {
    throw keyringMigrationFailure();
  }
  return {
    entries: normalizedEntries,
    value: values.values().next().value ?? null
  };
}

async function rollbackScopedKeyringCredential(
  keyring: KeyringBackend,
  target: CredentialTarget
): Promise<void> {
  try {
    await callKeyring("write", () =>
      keyring.deletePassword(SERVICE, scopedAccount(target))
    );
  } catch {
    // Best effort only: rollback failure must not replace the fixed migration failure.
  }
}

function keyringMigrationFailure(): KeyringAccessError {
  return new KeyringAccessError("unknown", "read");
}

function requireConsent(consent: PlaintextConsent | undefined): PlaintextConsent {
  if (!(consent instanceof PlaintextConsent)) {
    throw new Error("Explicit plaintext credential consent is required.");
  }
  return consent;
}

function scopedAccount(target: CredentialTarget): string {
  return `${LEGACY_ACCOUNT}:v2:${credentialTargetFingerprint(target)}`;
}

function legacyProviderAccount(providerId: string): string {
  return `${LEGACY_ACCOUNT}:${providerId}`;
}

function readLegacyProviderSecret(file: CredentialFile, providerId: string): string | null {
  const value =
    file.providers?.[providerId] ??
    (providerId === "minimax-official" ? file.minimaxApiKey : undefined);
  return value ? normalizeApiKey(value) || null : null;
}

function requireCredential(value: string): string {
  const normalized = normalizeApiKey(value);
  if (!normalized) {
    throw new Error("API key is empty after normalization.");
  }
  return normalized;
}

function normalizeStoredCredential(value: string | null | undefined): string | null {
  return value ? normalizeApiKey(value) || null : null;
}

function isStringRecord(value: unknown): value is Record<string, string> {
  return (
    typeof value === "object" &&
    value !== null &&
    !Array.isArray(value) &&
    Object.values(value).every((secret) => typeof secret === "string")
  );
}

function isCredentialFile(value: unknown): value is CredentialFile {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    return false;
  }
  const file = value as Record<string, unknown>;
  return (
    (file.minimaxApiKey === undefined || typeof file.minimaxApiKey === "string") &&
    (file.providers === undefined || isStringRecord(file.providers)) &&
    (file.targets === undefined || isStringRecord(file.targets))
  );
}

export function resolveUserConfigDir(): string {
  return resolveUserConfigRoot();
}

export function normalizeApiKey(apiKey: string): string {
  return apiKey
    .trim()
    .replace(/^["']|["']$/g, "")
    .replace(/^Bearer(?:\s+|$)/i, "")
    .trim();
}

export function isKeyringUnavailableError(
  error: unknown
): error is KeyringAccessError & {kind: "unavailable"} {
  return error instanceof KeyringAccessError && error.kind === "unavailable";
}

export function normalizeKeyringAccessError(
  error: unknown,
  operation: KeyringOperation
): KeyringAccessError {
  try {
    return classifyKeyringAccessError(error, operation);
  } catch {
    return new KeyringAccessError("unknown", operation, {cause: error});
  }
}

function classifyKeyringAccessError(
  error: unknown,
  operation: KeyringOperation
): KeyringAccessError {
  if (error instanceof KeyringAccessError) {
    return error;
  }

  const code = errorField(error, "code").toUpperCase();
  const name = errorField(error, "name").toLowerCase();
  const message = errorField(error, "message").toLowerCase();

  if (
    code === "EACCES" ||
    code === "EPERM" ||
    name.includes("notallowed") ||
    name.includes("permissiondenied") ||
    name.includes("accessdenied")
  ) {
    return new KeyringAccessError("denied", operation, {cause: error});
  }
  if (
    code.includes("KEYRING_LOCKED") ||
    code.includes("KEYCHAIN_LOCKED") ||
    name.includes("keyringlocked") ||
    name.includes("keychainlocked") ||
    name === "lockederror"
  ) {
    return new KeyringAccessError("locked", operation, {cause: error});
  }
  if (
    code === "ERR_MODULE_NOT_FOUND" ||
    code === "MODULE_NOT_FOUND" ||
    code === "ENOENT" ||
    code === "ECONNREFUSED" ||
    code.includes("DBUS.ERROR.SERVICEUNKNOWN") ||
    code.includes("DBUS.ERROR.NOSERVER") ||
    name.includes("keyringunavailable") ||
    name.includes("keychainunavailable") ||
    name.includes("serviceunavailable")
  ) {
    return new KeyringUnavailableError(operation, {cause: error});
  }
  if (/key(?:ring|chain).{0,24}locked|locked.{0,24}key(?:ring|chain)/i.test(message)) {
    return new KeyringAccessError("locked", operation, {cause: error});
  }
  if (/permission denied|access denied|not allowed|operation not permitted/i.test(message)) {
    return new KeyringAccessError("denied", operation, {cause: error});
  }
  if (
    /key(?:ring|chain).{0,24}unavailable|secret service.{0,24}(?:unavailable|not running)|session bus|dbus.{0,24}(?:unavailable|not running)|connection refused/i.test(
      message
    ) ||
    message.includes("org.freedesktop.dbus.error.serviceunknown") ||
    message.includes("org.freedesktop.dbus.error.noserver") ||
    message.includes(
      "the name org.freedesktop.secrets was not provided by any .service files"
    ) ||
    message.includes("cannot autolaunch d-bus")
  ) {
    return new KeyringUnavailableError(operation, {cause: error});
  }
  return new KeyringAccessError("unknown", operation, {cause: error});
}

async function callKeyring<T>(
  operation: Exclude<KeyringOperation, "load">,
  invoke: () => T | Promise<T>
): Promise<T> {
  try {
    return await invoke();
  } catch (error) {
    throw normalizeKeyringAccessError(error, operation);
  }
}

function errorField(error: unknown, field: "code" | "name" | "message"): string {
  try {
    if (typeof error !== "object" || error === null) {
      return field === "message" ? String(error) : "";
    }
    const value = Reflect.get(error, field);
    return typeof value === "string" ? value : "";
  } catch {
    return "";
  }
}
