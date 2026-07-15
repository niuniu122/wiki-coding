import assert from "node:assert/strict";
import test from "node:test";
import {
  MINIMAX_BUILTIN_ADAPTER_ID,
  PROVIDER_FEATURE_KEYS,
  ProviderContractError,
  parseModelProfileId,
  parseProviderAdapterId,
  parseProviderAdapterManifest,
  parseProviderFeatureProfile,
  parseProviderProfileId,
  validateProviderContract
} from "../src/providers/provider-adapter.js";

function featureMatrix(value = false): Record<string, boolean> {
  return Object.fromEntries(PROVIDER_FEATURE_KEYS.map((feature) => [feature, value]));
}

function validManifest(adapterId = "adapter:example/standard"): Record<string, unknown> {
  return {
    schemaVersion: 1,
    adapterId,
    displayName: "Example Standard Adapter",
    packageVersion: "1.0.0",
    apiVersion: 1,
    protocols: ["responses", "chat_completions"]
  };
}

function isContractError(code: ProviderContractError["code"], path?: string) {
  return (error: unknown): boolean =>
    error instanceof ProviderContractError &&
    error.code === code &&
    (path === undefined || error.path === path);
}

test("qualified adapter, provider, and model identities cannot be mixed", () => {
  assert.equal(parseProviderAdapterId("adapter:example/standard"), "adapter:example/standard");
  assert.equal(parseProviderProfileId("provider:example/default"), "provider:example/default");
  assert.equal(parseModelProfileId("model:example/model-a"), "model:example/model-a");

  assert.throws(
    () => parseProviderAdapterId("provider:example/default"),
    isContractError("identifier_kind_mismatch", "adapterId")
  );
  assert.throws(
    () => parseProviderProfileId("model:example/model-a"),
    isContractError("identifier_kind_mismatch", "providerProfileId")
  );
  assert.throws(
    () => parseModelProfileId("adapter:example/standard"),
    isContractError("identifier_kind_mismatch", "modelProfileId")
  );
});

test("adapter manifests reject unknown versions and incomplete data", () => {
  assert.deepEqual(parseProviderAdapterManifest(validManifest()), validManifest());

  assert.throws(
    () => parseProviderAdapterManifest({...validManifest(), schemaVersion: 2}),
    isContractError("unsupported_schema_version", "manifest.schemaVersion")
  );
  const incomplete = validManifest();
  delete incomplete.protocols;
  assert.throws(
    () => parseProviderAdapterManifest(incomplete),
    isContractError("missing_field", "manifest.protocols")
  );
});

test("the protected MiniMax adapter identity requires a builtin origin", () => {
  const manifest = validManifest(MINIMAX_BUILTIN_ADAPTER_ID);
  assert.throws(
    () => parseProviderAdapterManifest(manifest, {origin: "managed"}),
    isContractError("protected_identifier", "manifest.adapterId")
  );
  assert.equal(
    parseProviderAdapterManifest(manifest, {origin: "builtin"}).adapterId,
    MINIMAX_BUILTIN_ADAPTER_ID
  );
});

test("feature profiles are complete and reject unknown capabilities", () => {
  const features = featureMatrix();
  assert.deepEqual(parseProviderFeatureProfile({schemaVersion: 1, features}), {
    schemaVersion: 1,
    features
  });

  const incomplete = featureMatrix();
  delete incomplete.streaming;
  assert.throws(
    () => parseProviderFeatureProfile({schemaVersion: 1, features: incomplete}),
    isContractError("missing_field", "featureProfile.features.streaming")
  );
  assert.throws(
    () =>
      parseProviderFeatureProfile({
        schemaVersion: 1,
        features: {...featureMatrix(), filesystem_access: true}
      }),
    isContractError("unknown_feature", "featureProfile.features.filesystem_access")
  );
});

test("contract failures use one structured and redacted classification", () => {
  assert.deepEqual(validateProviderContract(() => parseProviderProfileId("provider:a/b")), {
    ok: true
  });
  const result = validateProviderContract(() =>
    parseProviderProfileId("model:example/model-a")
  );
  assert.equal(result.ok, false);
  if (!result.ok) {
    assert.deepEqual(result.issues, [
      {
        code: "identifier_kind_mismatch",
        path: "providerProfileId",
        message:
          "Provider contract validation failed at providerProfileId (identifier_kind_mismatch)."
      }
    ]);
  }
});
