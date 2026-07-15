import {join, resolve} from "node:path";
import type {ModelProfileId, ProviderProfileId} from "../types.js";
import {parseModelProfile, type ModelProfile} from "../providers/model-profile.js";
import {
  parseProviderProfile,
  type ProviderProfile
} from "../providers/provider-profile.js";
import {readJsonFile, writeJsonFile} from "../utils/jsonl.js";
import {resolveUserConfigRoot} from "./user-config-root.js";

export const USER_PROFILE_STORE_SCHEMA_VERSION = 1 as const;

interface RawProfileEnvelope {
  readonly schemaVersion: 1;
  readonly profiles: readonly unknown[];
}

export type UserProfileIssueCode =
  | "invalid_profile"
  | "duplicate_profile_id"
  | "store_recovery_required";

export interface UserProfileIssue {
  readonly kind: "provider" | "model" | "store";
  readonly code: UserProfileIssueCode;
  readonly file: "provider-profiles.json" | "model-profiles.json";
  readonly profileId?: string;
}

export interface UserProfileSnapshot {
  readonly providerProfiles: readonly ProviderProfile[];
  readonly modelProfiles: readonly ModelProfile[];
  readonly issues: readonly UserProfileIssue[];
}

export interface UserProfileStoreOptions {
  readonly userConfigDir?: string;
}

export class UserProfileStoreRecoveryError extends Error {
  constructor(readonly profileFile: string) {
    super(`User profile store requires recovery: ${profileFile}.`);
    this.name = "UserProfileStoreRecoveryError";
  }
}

const EMPTY_ENVELOPE: RawProfileEnvelope = Object.freeze({
  schemaVersion: USER_PROFILE_STORE_SCHEMA_VERSION,
  profiles: Object.freeze([])
});

export class UserProfileStore {
  readonly providerProfilesPath: string;
  readonly modelProfilesPath: string;

  constructor(options: UserProfileStoreOptions = {}) {
    const root = resolve(options.userConfigDir ?? resolveUserConfigRoot());
    this.providerProfilesPath = join(root, "provider-profiles.json");
    this.modelProfilesPath = join(root, "model-profiles.json");
  }

  async load(): Promise<UserProfileSnapshot> {
    const [providerFile, modelFile] = await Promise.all([
      this.loadEnvelope(this.providerProfilesPath, "provider-profiles.json"),
      this.loadEnvelope(this.modelProfilesPath, "model-profiles.json")
    ]);
    const providers = validateProfileCollection(
      providerFile.envelope.profiles,
      parseProviderProfile,
      (profile) => profile.providerProfileId,
      "provider",
      "provider-profiles.json"
    );
    const models = validateProfileCollection(
      modelFile.envelope.profiles,
      parseModelProfile,
      (profile) => profile.modelProfileId,
      "model",
      "model-profiles.json"
    );

    return Object.freeze({
      providerProfiles: Object.freeze(providers.profiles),
      modelProfiles: Object.freeze(models.profiles),
      issues: Object.freeze([
        ...providerFile.issues,
        ...modelFile.issues,
        ...providers.issues,
        ...models.issues
      ])
    });
  }

  async saveProviderProfile(profile: ProviderProfile): Promise<void> {
    const parsed = parseProviderProfile(profile);
    await this.upsertProfile(
      this.providerProfilesPath,
      "provider-profiles.json",
      "providerProfileId",
      parsed.providerProfileId,
      parsed
    );
  }

  async saveModelProfile(profile: ModelProfile): Promise<void> {
    const parsed = parseModelProfile(profile);
    await this.upsertProfile(
      this.modelProfilesPath,
      "model-profiles.json",
      "modelProfileId",
      parsed.modelProfileId,
      parsed
    );
  }

  private async loadEnvelope(
    path: string,
    file: UserProfileIssue["file"]
  ): Promise<{envelope: RawProfileEnvelope; issues: readonly UserProfileIssue[]}> {
    try {
      const envelope = await readJsonFile(path, EMPTY_ENVELOPE, {
        parse: parseProfileEnvelope
      });
      return {envelope, issues: []};
    } catch {
      return {
        envelope: EMPTY_ENVELOPE,
        issues: [{kind: "store", code: "store_recovery_required", file}]
      };
    }
  }

  private async upsertProfile(
    path: string,
    file: UserProfileIssue["file"],
    idKey: "providerProfileId" | "modelProfileId",
    id: ProviderProfileId | ModelProfileId,
    profile: ProviderProfile | ModelProfile
  ): Promise<void> {
    let envelope: RawProfileEnvelope;
    try {
      envelope = await readJsonFile(path, EMPTY_ENVELOPE, {
        parse: parseProfileEnvelope
      });
    } catch {
      throw new UserProfileStoreRecoveryError(file);
    }
    const retained = envelope.profiles.filter(
      (entry) => !isRecord(entry) || entry[idKey] !== id
    );
    await writeJsonFile(
      path,
      {
        schemaVersion: USER_PROFILE_STORE_SCHEMA_VERSION,
        profiles: [...retained, profile]
      },
      {mode: 0o600}
    );
  }
}

function parseProfileEnvelope(value: unknown): RawProfileEnvelope {
  if (!isRecord(value)) {
    throw new Error("Profile store root must be an object.");
  }
  const keys = Object.keys(value);
  if (
    keys.some((key) => key !== "schemaVersion" && key !== "profiles") ||
    value.schemaVersion !== USER_PROFILE_STORE_SCHEMA_VERSION ||
    !Array.isArray(value.profiles)
  ) {
    throw new Error("Profile store schema is invalid.");
  }
  return Object.freeze({
    schemaVersion: USER_PROFILE_STORE_SCHEMA_VERSION,
    profiles: Object.freeze([...value.profiles])
  });
}

function validateProfileCollection<T>(
  rawProfiles: readonly unknown[],
  parse: (value: unknown) => T,
  idOf: (profile: T) => string,
  kind: "provider" | "model",
  file: UserProfileIssue["file"]
): {profiles: T[]; issues: UserProfileIssue[]} {
  const parsed: Array<{profile: T; id: string}> = [];
  const issues: UserProfileIssue[] = [];
  for (const raw of rawProfiles) {
    try {
      const profile = parse(raw);
      parsed.push({profile, id: idOf(profile)});
    } catch {
      issues.push({kind, code: "invalid_profile", file});
    }
  }

  const counts = new Map<string, number>();
  for (const {id} of parsed) {
    counts.set(id, (counts.get(id) ?? 0) + 1);
  }
  const duplicateIds = new Set(
    [...counts.entries()].filter(([, count]) => count > 1).map(([id]) => id)
  );
  for (const profileId of duplicateIds) {
    issues.push({kind, code: "duplicate_profile_id", file, profileId});
  }
  return {
    profiles: parsed
      .filter(({id}) => !duplicateIds.has(id))
      .map(({profile}) => profile),
    issues
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
