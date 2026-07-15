import assert from "node:assert/strict";
import test from "node:test";
import {BuiltinProviderAdapter} from "../src/providers/builtin-provider-adapter.js";
import {
  ProviderAdapterRegistry,
  ProviderAdapterRegistryError,
  createBuiltinConformanceEvidence
} from "../src/providers/provider-adapter-registry.js";

test("the protected builtin adapter registers with explicit conformance evidence", () => {
  const adapter = new BuiltinProviderAdapter();
  const registry = new ProviderAdapterRegistry();

  registry.registerBuiltin(adapter, createBuiltinConformanceEvidence(adapter));

  assert.equal(registry.get(adapter.manifest.adapterId), adapter);
  assert.equal(registry.hasConformanceFixture(adapter.manifest.adapterId, "responses"), true);
  assert.equal(
    registry.hasConformanceFixture(adapter.manifest.adapterId, "chat_completions"),
    true
  );
});

test("adapter registration rejects conflicts and missing protocol fixtures", () => {
  const adapter = new BuiltinProviderAdapter();
  const registry = new ProviderAdapterRegistry();
  registry.registerBuiltin(adapter, {
    schemaVersion: 1,
    adapterId: adapter.manifest.adapterId,
    fixtureVersion: "1",
    protocols: ["responses"]
  });

  assert.equal(
    registry.hasConformanceFixture(adapter.manifest.adapterId, "chat_completions"),
    false
  );
  assert.throws(
    () => registry.registerBuiltin(adapter, createBuiltinConformanceEvidence(adapter)),
    (error: unknown) =>
      error instanceof ProviderAdapterRegistryError && error.code === "duplicate_adapter"
  );
});

test("the first registry never dynamically loads arbitrary Tier-2 JavaScript", async () => {
  const registry = new ProviderAdapterRegistry();

  await assert.rejects(
    () => registry.loadDynamicPackage("C:/untrusted/provider-adapter.js"),
    (error: unknown) =>
      error instanceof ProviderAdapterRegistryError &&
      error.code === "dynamic_packages_disabled"
  );
});
