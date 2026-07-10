import {homedir} from "node:os";
import {join, resolve} from "node:path";
import {rm} from "node:fs/promises";
import {readJsonFile, writeJsonFile} from "../utils/jsonl.js";

interface SecretFile {
  minimaxApiKey?: string;
  providers?: Record<string, string>;
}

export interface KeytarLike {
  getPassword(service: string, account: string): Promise<string | null>;
  setPassword(service: string, account: string, password: string): Promise<void>;
}

export interface SecretStoreOptions {
  userConfigDir?: string;
  keytar?: KeytarLike | null;
}

export type SecretLocation = "keychain" | "user-file";

const SERVICE = "minimax-codex";
const ACCOUNT = "minimax-api-key";

export class SecretStore {
  private readonly userConfigDir: string;
  private readonly keytarOverride: KeytarLike | null | undefined;

  constructor(
    private readonly legacyRootDir: string,
    options: SecretStoreOptions = {}
  ) {
    this.userConfigDir = options.userConfigDir ?? resolveUserConfigDir();
    this.keytarOverride = options.keytar;
  }

  async getApiKey(
    providerId = "minimax-official",
    envKey = "MINIMAX_API_KEY"
  ): Promise<string | null> {
    if (envKey && process.env[envKey]) {
      return process.env[envKey] ?? null;
    }
    if (providerId === "minimax-official" && process.env.MINIMAX_API_KEY) {
      return process.env.MINIMAX_API_KEY;
    }

    const keytar = await this.loadKeytar();
    if (keytar) {
      const value = await keytar.getPassword(SERVICE, providerAccount(providerId));
      if (value) {
        return value;
      }
      if (providerId === "minimax-official") {
        const legacyValue = await keytar.getPassword(SERVICE, ACCOUNT);
        if (legacyValue) {
          return legacyValue;
        }
      }
    }

    const userFile = await readJsonFile<SecretFile>(this.userSecretPath(), {}, {
      validate: isSecretFile
    });
    const userValue = readProviderSecret(userFile, providerId);
    if (userValue) {
      return userValue;
    }

    const legacyFile = await readJsonFile<SecretFile>(this.legacySecretPath(), {}, {
      validate: isSecretFile
    });
    const migrated = collectProviderSecrets(legacyFile);
    if (Object.keys(migrated).length === 0) {
      return null;
    }

    await this.persistAll(migrated, keytar);
    await rm(this.legacySecretPath(), {force: true});
    await rm(`${this.legacySecretPath()}.bak`, {force: true});
    return migrated[providerId] ?? null;
  }

  async setApiKey(
    apiKey: string,
    providerId = "minimax-official"
  ): Promise<SecretLocation> {
    const normalizedApiKey = normalizeApiKey(apiKey);
    if (!normalizedApiKey) {
      throw new Error("API key is empty after normalization.");
    }
    const keytar = await this.loadKeytar();
    if (keytar) {
      await keytar.setPassword(SERVICE, providerAccount(providerId), normalizedApiKey);
      return "keychain";
    }

    await this.persistAll({[providerId]: normalizedApiKey}, null);
    return "user-file";
  }

  private async persistAll(
    providers: Record<string, string>,
    keytar: KeytarLike | null
  ): Promise<void> {
    if (keytar) {
      for (const [providerId, apiKey] of Object.entries(providers)) {
        await keytar.setPassword(SERVICE, providerAccount(providerId), normalizeApiKey(apiKey));
      }
      return;
    }

    const current = await readJsonFile<SecretFile>(this.userSecretPath(), {}, {
      validate: isSecretFile
    });
    const normalized = Object.fromEntries(
      Object.entries(providers).map(([providerId, apiKey]) => [providerId, normalizeApiKey(apiKey)])
    );
    await writeJsonFile(
      this.userSecretPath(),
      {
        providers: {
          ...current.providers,
          ...normalized
        }
      } satisfies SecretFile,
      {mode: 0o600}
    );
  }

  private async loadKeytar(): Promise<KeytarLike | null> {
    return this.keytarOverride === undefined ? tryLoadKeytar() : this.keytarOverride;
  }

  private userSecretPath(): string {
    return join(this.userConfigDir, "credentials.json");
  }

  private legacySecretPath(): string {
    return join(this.legacyRootDir, "secrets.local.json");
  }
}

function resolveUserConfigDir(): string {
  const override = process.env.MINIMAX_CODEX_HOME?.trim();
  if (override) {
    return resolve(override);
  }
  if (process.platform === "win32") {
    return join(process.env.APPDATA?.trim() || join(homedir(), "AppData", "Roaming"), "minimax-codex");
  }
  if (process.platform === "darwin") {
    return join(homedir(), "Library", "Application Support", "minimax-codex");
  }
  return join(process.env.XDG_CONFIG_HOME?.trim() || join(homedir(), ".config"), "minimax-codex");
}

function collectProviderSecrets(file: SecretFile): Record<string, string> {
  const providers = Object.fromEntries(
    Object.entries(file.providers ?? {})
      .map(
        ([providerId, apiKey]): [string, string] => [providerId, normalizeApiKey(apiKey)]
      )
      .filter((entry) => entry[1].length > 0)
  );
  const legacyMiniMaxKey = file.minimaxApiKey ? normalizeApiKey(file.minimaxApiKey) : "";
  if (legacyMiniMaxKey && !providers["minimax-official"]) {
    providers["minimax-official"] = legacyMiniMaxKey;
  }
  return providers;
}

function readProviderSecret(file: SecretFile, providerId: string): string | null {
  return (
    file.providers?.[providerId] ??
    (providerId === "minimax-official" ? file.minimaxApiKey : undefined) ??
    null
  );
}

function isSecretFile(value: unknown): value is SecretFile {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    return false;
  }
  const file = value as Record<string, unknown>;
  if (file.minimaxApiKey !== undefined && typeof file.minimaxApiKey !== "string") {
    return false;
  }
  if (file.providers === undefined) {
    return true;
  }
  return (
    typeof file.providers === "object" &&
    file.providers !== null &&
    !Array.isArray(file.providers) &&
    Object.values(file.providers).every((secret) => typeof secret === "string")
  );
}

function providerAccount(providerId: string): string {
  return `${ACCOUNT}:${providerId}`;
}

export function normalizeApiKey(apiKey: string): string {
  return apiKey
    .trim()
    .replace(/^["']|["']$/g, "")
    .replace(/^Bearer(?:\s+|$)/i, "")
    .trim();
}

async function tryLoadKeytar(): Promise<KeytarLike | null> {
  try {
    const dynamicImport = new Function("specifier", "return import(specifier)") as (
      specifier: string
    ) => Promise<unknown>;
    const mod = (await dynamicImport("keytar")) as Partial<KeytarLike> & {
      default?: KeytarLike;
    };
    const candidate = mod.default ?? mod;
    if (
      typeof candidate.getPassword === "function" &&
      typeof candidate.setPassword === "function"
    ) {
      return candidate as KeytarLike;
    }
  } catch {
    return null;
  }
  return null;
}
