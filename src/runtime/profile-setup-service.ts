import type {UserProfileStore} from "../config/user-profile-store.js";
import {findUnsupportedModelFeatures} from "../providers/builtin-provider-adapter.js";
import type {ProviderAdapterRegistry} from "../providers/provider-adapter-registry.js";
import {parseModelProfile, type ModelProfile} from "../providers/model-profile.js";
import {
  parseProviderProfile,
  type ProviderProfile
} from "../providers/provider-profile.js";

export type ProfileSetupErrorCode =
  | "adapter_unavailable"
  | "provider_unavailable"
  | "profile_invalid"
  | "conformance_fixture_missing"
  | "unsupported_feature";

export class ProfileSetupError extends Error {
  constructor(readonly code: ProfileSetupErrorCode) {
    super(`Profile setup failed (${code}).`);
    this.name = "ProfileSetupError";
  }
}

export class ProfileSetupService {
  constructor(
    private readonly store: UserProfileStore,
    private readonly adapters: ProviderAdapterRegistry
  ) {}

  async setupProvider(value: unknown): Promise<ProviderProfile> {
    let profile: ProviderProfile;
    try {
      profile = parseProviderProfile(value);
    } catch {
      throw new ProfileSetupError("profile_invalid");
    }
    const adapter = this.adapters.get(profile.adapterId);
    if (!adapter) {
      throw new ProfileSetupError("adapter_unavailable");
    }
    if (
      !this.adapters.hasConformanceFixture(
        profile.adapterId,
        profile.transport.protocol
      )
    ) {
      throw new ProfileSetupError("conformance_fixture_missing");
    }
    if (!adapter.validateProfile(profile).ok) {
      throw new ProfileSetupError("profile_invalid");
    }
    await this.store.saveProviderProfile(profile);
    return profile;
  }

  async setupModel(value: unknown): Promise<ModelProfile> {
    let profile: ModelProfile;
    try {
      profile = parseModelProfile(value);
    } catch {
      throw new ProfileSetupError("profile_invalid");
    }
    const snapshot = await this.store.load();
    const providerProfile = snapshot.providerProfiles.find(
      (provider) => provider.providerProfileId === profile.providerProfileId
    );
    if (!providerProfile) {
      throw new ProfileSetupError("provider_unavailable");
    }
    const adapter = this.adapters.get(providerProfile.adapterId);
    if (!adapter) {
      throw new ProfileSetupError("adapter_unavailable");
    }
    if (
      !this.adapters.hasConformanceFixture(
        providerProfile.adapterId,
        providerProfile.transport.protocol
      )
    ) {
      throw new ProfileSetupError("conformance_fixture_missing");
    }
    const unsupported = findUnsupportedModelFeatures(
      profile,
      adapter.describeFeatures(profile).features
    );
    if (unsupported.length > 0) {
      throw new ProfileSetupError("unsupported_feature");
    }
    await this.store.saveModelProfile(profile);
    return profile;
  }
}
