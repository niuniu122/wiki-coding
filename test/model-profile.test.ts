import assert from "node:assert/strict";
import test from "node:test";
import {
  MINIMAX_BUILTIN_ADAPTER_ID,
  PROVIDER_FEATURE_KEYS,
  ProviderContractError
} from "../src/providers/provider-adapter.js";
import {parseModelFeatureProfile, parseModelProfile} from "../src/providers/model-profile.js";
import {parseProviderProfile} from "../src/providers/provider-profile.js";

function featureMatrix(value = false): Record<string, boolean> {
  return Object.fromEntries(PROVIDER_FEATURE_KEYS.map((feature) => [feature, value]));
}

function validProviderProfile(): Record<string, unknown> {
  return {
    schemaVersion: 1,
    providerProfileId: "provider:minimax/official",
    adapterId: MINIMAX_BUILTIN_ADAPTER_ID,
    displayName: "MiniMax Official",
    enabled: true,
    transport: {
      baseUrl: "https://api.minimax.io/v1",
      protocol: "responses"
    },
    authentication: {
      kind: "bearer",
      envBinding: "MINIMAX_API_KEY"
    }
  };
}

function validModelFeatureProfile(): Record<string, unknown> {
  return {
    schemaVersion: 1,
    features: featureMatrix(),
    contextWindow: 128000,
    maxOutputTokens: 8192
  };
}

function validModelProfile(): Record<string, unknown> {
  return {
    schemaVersion: 1,
    modelProfileId: "model:minimax/minimax-m3",
    providerProfileId: "provider:minimax/official",
    displayName: "MiniMax M3",
    model: "MiniMax-M3",
    enabled: true,
    featureProfile: validModelFeatureProfile()
  };
}

function isContractError(code: ProviderContractError["code"], path?: string) {
  return (error: unknown): boolean =>
    error instanceof ProviderContractError &&
    error.code === code &&
    (path === undefined || error.path === path);
}

test("provider profiles validate required transport and authentication metadata", () => {
  const profile = parseProviderProfile(validProviderProfile());
  assert.equal(profile.providerProfileId, "provider:minimax/official");
  assert.equal(profile.adapterId, MINIMAX_BUILTIN_ADAPTER_ID);
  assert.deepEqual(profile.transport.publicHeaders, {});
  assert.equal(profile.transport.allowInsecureLoopback, false);

  const incomplete = validProviderProfile();
  delete incomplete.transport;
  assert.throws(
    () => parseProviderProfile(incomplete),
    isContractError("missing_field", "providerProfile.transport")
  );
});

test("provider profiles describe authentication without carrying credential values", () => {
  assert.throws(
    () => parseProviderProfile({...validProviderProfile(), apiKey: "SECRET_VALUE"}),
    isContractError("unknown_field", "providerProfile.apiKey")
  );
  const profile = validProviderProfile();
  profile.authentication = {
    kind: "bearer",
    envBinding: "MINIMAX_API_KEY",
    secret: "SECRET_VALUE"
  };
  assert.throws(
    () => parseProviderProfile(profile),
    isContractError("unknown_field", "providerProfile.authentication.secret")
  );
  assert.throws(
    () =>
      parseProviderProfile({
        ...validProviderProfile(),
        transport: {
          baseUrl: "https://api.minimax.io/v1",
          protocol: "responses",
          publicHeaders: {Authorization: "Bearer SECRET_VALUE"}
        }
      }),
    isContractError("invalid_value", "providerProfile.transport.publicHeaders")
  );
});

test("provider and model profile references reject another identity kind", () => {
  assert.throws(
    () =>
      parseProviderProfile({
        ...validProviderProfile(),
        adapterId: "provider:minimax/official"
      }),
    isContractError("identifier_kind_mismatch", "providerProfile.adapterId")
  );
  assert.throws(
    () =>
      parseModelProfile({
        ...validModelProfile(),
        providerProfileId: "adapter:minimax/builtin"
      }),
    isContractError("identifier_kind_mismatch", "modelProfile.providerProfileId")
  );
});

test("model feature profiles reject unknown versions and invalid limits", () => {
  assert.deepEqual(parseModelFeatureProfile(validModelFeatureProfile()), {
    schemaVersion: 1,
    features: featureMatrix(),
    contextWindow: 128000,
    maxOutputTokens: 8192
  });
  assert.throws(
    () => parseModelFeatureProfile({...validModelFeatureProfile(), schemaVersion: 2}),
    isContractError("unsupported_schema_version", "modelFeatureProfile.schemaVersion")
  );
  assert.throws(
    () => parseModelFeatureProfile({...validModelFeatureProfile(), maxOutputTokens: 200000}),
    isContractError("invalid_value", "modelFeatureProfile.maxOutputTokens")
  );
});

test("model profiles cannot carry local permission or tool authority", () => {
  const profile = parseModelProfile(validModelProfile());
  assert.equal(profile.modelProfileId, "model:minimax/minimax-m3");
  assert.equal(profile.providerProfileId, "provider:minimax/official");

  for (const forbidden of ["permissionMode", "allowedTools", "workspaceRoots"]) {
    assert.throws(
      () => parseModelProfile({...validModelProfile(), [forbidden]: "full_access"}),
      isContractError("unknown_field", `modelProfile.${forbidden}`)
    );
  }
});
