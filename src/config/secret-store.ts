import {join} from "node:path";
import {BUILT_IN_MODEL_PROVIDERS} from "./config-manager.js";
import {
  CredentialStore,
  type CredentialStoreOptions,
  type KeyringBackend,
  type PlaintextConsent,
  isKeyringUnavailableError
} from "./credential-store.js";
import {createCredentialTarget} from "./provider-security.js";

export type KeytarLike = KeyringBackend;

export interface SecretStoreOptions {
  userConfigDir?: string;
  keytar?: KeytarLike | null;
  env?: CredentialStoreOptions["env"];
  credentialStore?: CredentialStore;
}

export type SecretLocation = "keychain" | "user-file";

export class SecretStore {
  private readonly credentials: CredentialStore;

  constructor(
    private readonly legacyRootDir: string,
    options: SecretStoreOptions = {}
  ) {
    this.credentials =
      options.credentialStore ??
      new CredentialStore({
        ...(options.userConfigDir ? {userConfigDir: options.userConfigDir} : {}),
        ...(options.keytar !== undefined ? {keyring: options.keytar} : {}),
        ...(options.env ? {env: options.env} : {})
      });
  }

  createPlaintextConsent(): PlaintextConsent {
    return this.credentials.createPlaintextConsent();
  }

  async getApiKey(
    providerId = "minimax-official",
    envKey = "MINIMAX_API_KEY",
    consent?: PlaintextConsent
  ): Promise<string | null> {
    void envKey;
    void consent;
    const target = trustedLegacyTarget(providerId);
    await this.credentials.migrateLegacyWorkspaceCredential(
      target,
      this.legacySecretPath()
    );
    return this.credentials.get(target);
  }

  async setApiKey(
    apiKey: string,
    providerId = "minimax-official",
    consent?: PlaintextConsent
  ): Promise<SecretLocation> {
    const target = trustedLegacyTarget(providerId);
    try {
      await this.credentials.saveToKeyring(target, apiKey);
      return "keychain";
    } catch (error) {
      if (!isKeyringUnavailableError(error)) {
        throw error;
      }
      await this.credentials.saveToUserFile(target, apiKey, consent);
      return "user-file";
    }
  }

  private legacySecretPath(): string {
    return join(this.legacyRootDir, "secrets.local.json");
  }
}

function trustedLegacyTarget(providerId: string) {
  const provider = BUILT_IN_MODEL_PROVIDERS[providerId];
  if (!provider) {
    throw new Error(
      `Legacy SecretStore cannot scope custom provider ${providerId}; configure it through ProviderService.`
    );
  }
  return createCredentialTarget(providerId, provider);
}

export {normalizeApiKey} from "./credential-store.js";
