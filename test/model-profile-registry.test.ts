import assert from "node:assert/strict";
import {mkdtemp, readFile, rm, writeFile} from "node:fs/promises";
import {tmpdir} from "node:os";
import {join} from "node:path";
import test from "node:test";
import {UserProfileStore} from "../src/config/user-profile-store.js";
import {ConfigManager, DEFAULT_CONFIG} from "../src/config/config-manager.js";
import {BuiltinProviderAdapter} from "../src/providers/builtin-provider-adapter.js";
import {
  ProviderAdapterRegistry,
  createBuiltinConformanceEvidence
} from "../src/providers/provider-adapter-registry.js";
import {ModelProfileRegistry} from "../src/runtime/model-profile-registry.js";
import {ProfileSetupService} from "../src/runtime/profile-setup-service.js";
import {
  CONFORMANCE_FEATURES,
  createModelProfileFixture,
  createProviderProfileFixture
} from "./support/provider-conformance-suite.js";

function adapterRegistry(): ProviderAdapterRegistry {
  const adapter = new BuiltinProviderAdapter();
  const registry = new ProviderAdapterRegistry();
  registry.registerBuiltin(adapter, createBuiltinConformanceEvidence(adapter));
  return registry;
}

test("legacy Responses and Chat Completions config projects into builtin profiles", async () => {
  const registry = new ModelProfileRegistry(adapterRegistry());

  await registry.initialize(DEFAULT_CONFIG);

  const models = registry.listModels();
  assert.equal(models.some((entry) => entry.providerProfile.transport.protocol === "responses"), true);
  assert.equal(
    models.some((entry) => entry.providerProfile.transport.protocol === "chat_completions"),
    true
  );
  assert.equal(models.every((entry) => entry.adapter.manifest.adapterId === "adapter:minimax/builtin"), true);
  assert.equal(models.some((entry) => entry.source === "builtin" && entry.stickyEligible), true);
});

test("workspace-only legacy profiles remain usable but cannot become global sticky models", async () => {
  const config = {
    ...DEFAULT_CONFIG,
    modelProvider: "workspace-provider",
    model: "workspace-model",
    modelProviders: {
      ...DEFAULT_CONFIG.modelProviders,
      "workspace-provider": {
        name: "Workspace Provider",
        baseUrl: "https://workspace-provider.test/v1",
        protocol: "responses" as const,
        defaultModel: "workspace-model"
      }
    },
    context: {...DEFAULT_CONFIG.context}
  };
  const registry = new ModelProfileRegistry(adapterRegistry());

  await registry.initialize(config);

  const configured = registry.getConfiguredDefault();
  assert.equal(configured?.source, "legacy_workspace");
  assert.equal(configured?.stickyEligible, false);
  assert.equal(configured?.modelProfile.model, "workspace-model");
  assert.equal(registry.listModels().some((entry) => entry.source === "builtin"), true);
});

test("registry initialization can project legacy config without rewriting workspace bytes", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-registry-read-only-"));
  const configPath = join(root, "config.json");
  const raw = JSON.stringify({
    api: {
      provider: "hashsight",
      protocol: "chat_completions",
      baseUrl: "https://example.test/v1"
    }
  });

  try {
    await writeFile(configPath, raw, "utf8");
    const registry = new ModelProfileRegistry(adapterRegistry());

    await registry.initializeReadOnly(new ConfigManager(root));

    assert.equal(registry.getConfiguredDefault()?.modelProfile.model, "MiniMax-M3");
    assert.equal(await readFile(configPath, "utf8"), raw);
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("explicit setup publishes a global profile without changing model state", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-profile-setup-"));
  const store = new UserProfileStore({userConfigDir: root});
  const adapters = adapterRegistry();
  const setup = new ProfileSetupService(store, adapters);
  const provider = createProviderProfileFixture("responses", "provider:user/setup");
  const model = createModelProfileFixture(provider, {
    modelProfileId: "model:user/setup/model-a"
  });

  try {
    await setup.setupProvider(provider);
    await setup.setupModel(model);
    const registry = new ModelProfileRegistry(adapters, store);
    await registry.initialize(DEFAULT_CONFIG);

    const published = registry.getModel(model.modelProfileId);
    assert.equal(published?.source, "user");
    assert.equal(published?.stickyEligible, true);
    assert.deepEqual((await store.load()).modelProfiles, [model]);
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("feature-mismatched optional models are isolated while builtin models remain", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-profile-features-"));
  const store = new UserProfileStore({userConfigDir: root});
  const provider = createProviderProfileFixture("responses", "provider:user/features");
  const model = createModelProfileFixture(provider, {
    modelProfileId: "model:user/features/tool-model",
    features: {...CONFORMANCE_FEATURES, structured_output: true}
  });

  try {
    await store.saveProviderProfile(provider);
    await store.saveModelProfile(model);
    const registry = new ModelProfileRegistry(adapterRegistry(), store);
    await registry.initialize(DEFAULT_CONFIG);

    assert.equal(registry.getModel(model.modelProfileId), undefined);
    assert.equal(registry.listModels().some((entry) => entry.source === "builtin"), true);
    assert.equal(
      registry.issues.some((issue) => issue.code === "unsupported_feature"),
      true
    );
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("profiles without conformance fixtures or valid transport stay inactive", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-profile-conformance-"));
  const store = new UserProfileStore({userConfigDir: root});
  const adapter = new BuiltinProviderAdapter();
  const adapters = new ProviderAdapterRegistry();
  adapters.registerBuiltin(adapter, {
    schemaVersion: 1,
    adapterId: adapter.manifest.adapterId,
    fixtureVersion: "1",
    protocols: ["responses"]
  });
  const invalidProvider = createProviderProfileFixture(
    "responses",
    "provider:user/invalid-transport"
  );

  try {
    await store.saveProviderProfile({
      ...invalidProvider,
      transport: {...invalidProvider.transport, baseUrl: "http://example.test/v1"}
    });
    const registry = new ModelProfileRegistry(adapters, store);
    await registry.initialize(DEFAULT_CONFIG);

    assert.equal(
      registry.listModels().some(
        (entry) => entry.providerProfile.transport.protocol === "chat_completions"
      ),
      false
    );
    assert.equal(
      registry.issues.some((issue) => issue.code === "missing_conformance_fixture"),
      true
    );
    assert.equal(
      registry.issues.some((issue) => issue.code === "invalid_profile"),
      true
    );
    assert.equal(registry.listModels().some((entry) => entry.source === "builtin"), true);
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});
