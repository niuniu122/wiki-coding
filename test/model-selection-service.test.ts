import assert from "node:assert/strict";
import {mkdtemp, readFile, rm, writeFile} from "node:fs/promises";
import {tmpdir} from "node:os";
import {join} from "node:path";
import test from "node:test";
import {ConfigManager, DEFAULT_CONFIG} from "../src/config/config-manager.js";
import {CredentialStore} from "../src/config/credential-store.js";
import {ModelStateStore} from "../src/config/model-state-store.js";
import {UserProfileStore} from "../src/config/user-profile-store.js";
import {BuiltinProviderAdapter} from "../src/providers/builtin-provider-adapter.js";
import {createDefaultProviderAdapterRegistry} from "../src/providers/provider-adapter-registry.js";
import type {ModelStateLoadResult} from "../src/config/model-state-store.js";
import type {ProviderAdapter} from "../src/providers/provider-adapter.js";
import {parseProviderAdapterManifest} from "../src/providers/provider-adapter.js";
import type {ModelProfile} from "../src/providers/model-profile.js";
import type {ProviderProfile} from "../src/providers/provider-profile.js";
import type {ModelProfileRegistryEntry} from "../src/runtime/model-profile-registry.js";
import {ModelProfileRegistry} from "../src/runtime/model-profile-registry.js";
import type {
  ModelCredentialHandle,
  ModelRuntime,
  ModelRuntimeFactoryInput,
  ModelRuntimeRequest
} from "../src/runtime/model-runtime.js";
import {
  ModelSelectionError,
  ModelSelectionService,
  type ModelCredentialBinding,
  type ModelCredentialLocator
} from "../src/runtime/model-selection-service.js";
import type {ModelAdapterEvent} from "../src/runtime/model-adapter.js";
import {ProviderService} from "../src/runtime/provider-service.js";
import {
  CONFORMANCE_FEATURES,
  createModelProfileFixture,
  createProviderProfileFixture
} from "./support/provider-conformance-suite.js";

class FakeRuntime implements ModelRuntime {
  readonly adapterId;
  readonly providerProfileId;
  readonly modelProfileId;
  disposed = false;

  constructor(
    private readonly input: ModelRuntimeFactoryInput,
    private readonly log: string[]
  ) {
    this.adapterId = input.providerProfile.adapterId;
    this.providerProfileId = input.providerProfile.providerProfileId;
    this.modelProfileId = input.modelProfile.modelProfileId;
  }

  async *stream(_request: ModelRuntimeRequest): AsyncGenerator<ModelAdapterEvent> {
    yield {type: "completed"};
  }

  async dispose(): Promise<void> {
    this.disposed = true;
    this.log.push(`disposed:${this.modelProfileId}`);
  }
}

class FakeAdapter implements ProviderAdapter {
  readonly manifest = parseProviderAdapterManifest(
    {
      schemaVersion: 1,
      adapterId: "adapter:test/selection",
      displayName: "Selection test adapter",
      packageVersion: "1.0.0",
      apiVersion: 1,
      protocols: ["responses"]
    },
    {origin: "builtin"}
  );
  failModelId: string | undefined;
  readonly runtimes = new Map<string, FakeRuntime>();

  constructor(private readonly log: string[]) {}

  validateProfile(_profile: ProviderProfile) {
    return {ok: true} as const;
  }

  describeFeatures(model: ModelProfile) {
    return model.featureProfile;
  }

  async createRuntime(input: ModelRuntimeFactoryInput): Promise<ModelRuntime> {
    this.log.push(`runtime:${input.modelProfile.modelProfileId}`);
    if (input.modelProfile.modelProfileId === this.failModelId) {
      throw new Error("runtime construction failed");
    }
    const runtime = new FakeRuntime(input, this.log);
    this.runtimes.set(input.modelProfile.modelProfileId, runtime);
    return runtime;
  }
}

class FakeCatalog {
  constructor(
    private readonly entries: readonly ModelProfileRegistryEntry[],
    private readonly defaultId: string,
    private readonly log: string[]
  ) {}

  getModel(modelProfileId: string): ModelProfileRegistryEntry | undefined {
    this.log.push(`profile:${modelProfileId}`);
    return this.entries.find(
      (entry) => entry.modelProfile.modelProfileId === modelProfileId
    );
  }

  getConfiguredDefault(): ModelProfileRegistryEntry | undefined {
    this.log.push("profile:configured-default");
    return this.entries.find(
      (entry) => entry.modelProfile.modelProfileId === this.defaultId
    );
  }
}

class FakeStateStore {
  saved: string[] = [];
  failSave = false;
  beforeSave?: () => void;

  constructor(public loadResult: ModelStateLoadResult) {}

  async load(): Promise<ModelStateLoadResult> {
    return this.loadResult;
  }

  async save(modelProfileId: string) {
    this.beforeSave?.();
    if (this.failSave) {
      throw new Error("state write failed");
    }
    this.saved.push(modelProfileId);
    this.loadResult = {
      status: "selected",
      state: {
        schemaVersion: 1,
        lastSelectedModelProfileId: modelProfileId as never
      }
    };
    return this.loadResult.state;
  }
}

class FakeCredentialLocator implements ModelCredentialLocator {
  missingModelId: string | undefined;

  constructor(private readonly log: string[]) {}

  async locate(entry: ModelProfileRegistryEntry): Promise<ModelCredentialBinding> {
    const id = entry.modelProfile.modelProfileId;
    this.log.push(`credential:${id}`);
    const handle: ModelCredentialHandle = {
      targetId: `target:${id}`,
      readSecret: async () => "fixture-secret"
    };
    return {handle, hasCredential: id !== this.missingModelId};
  }
}

function entry(
  adapter: ProviderAdapter,
  name: string,
  options: {
    readonly source?: ModelProfileRegistryEntry["source"];
    readonly stickyEligible?: boolean;
    readonly nativeToolCalls?: boolean;
  } = {}
): ModelProfileRegistryEntry {
  const provider = createProviderProfileFixture(
    "responses",
    `provider:test/${name}`
  );
  const model = createModelProfileFixture(provider, {
    modelProfileId: `model:test/${name}/model`,
    features: {
      ...CONFORMANCE_FEATURES,
      native_tool_calls: options.nativeToolCalls ?? false
    }
  });
  return {
    providerProfile: {...provider, adapterId: adapter.manifest.adapterId},
    modelProfile: model,
    adapter,
    source: options.source ?? "user",
    stickyEligible: options.stickyEligible ?? true
  };
}

function selected(modelProfileId: string): ModelStateLoadResult {
  return {
    status: "selected",
    state: {
      schemaVersion: 1,
      lastSelectedModelProfileId: modelProfileId as never
    }
  };
}

test("startup prefers a valid sticky pointer and otherwise uses the configured default read-only", async () => {
  const log: string[] = [];
  const adapter = new FakeAdapter(log);
  const first = entry(adapter, "first");
  const sticky = entry(adapter, "sticky");
  const catalog = new FakeCatalog(
    [first, sticky],
    first.modelProfile.modelProfileId,
    log
  );
  const locator = new FakeCredentialLocator(log);
  const stickyState = new FakeStateStore(selected(sticky.modelProfile.modelProfileId));
  const stickyService = new ModelSelectionService(catalog, stickyState, locator);

  const restored = await stickyService.initialize(0.85);

  assert.equal(restored.modelProfileId, sticky.modelProfile.modelProfileId);
  assert.deepEqual(stickyState.saved, []);

  const fallbackState = new FakeStateStore({status: "unselected"});
  const fallbackService = new ModelSelectionService(catalog, fallbackState, locator);
  const fallback = await fallbackService.initialize(0.85);

  assert.equal(fallback.modelProfileId, first.modelProfile.modelProfileId);
  assert.deepEqual(fallbackState.saved, []);
});

test("switch validates, locates credentials, builds, persists, then publishes", async () => {
  const log: string[] = [];
  const adapter = new FakeAdapter(log);
  const first = entry(adapter, "first");
  const second = entry(adapter, "second");
  const catalog = new FakeCatalog(
    [first, second],
    first.modelProfile.modelProfileId,
    log
  );
  const state = new FakeStateStore({status: "unselected"});
  const service = new ModelSelectionService(
    catalog,
    state,
    new FakeCredentialLocator(log)
  );
  await service.initialize(0.9);
  log.length = 0;
  state.beforeSave = () => {
    log.push(`state:${second.modelProfile.modelProfileId}`);
    assert.equal(
      service.getRuntimeSnapshot().selection.modelProfileId,
      first.modelProfile.modelProfileId
    );
  };

  const changed = await service.switchModel(second.modelProfile.modelProfileId);

  assert.equal(changed.modelProfileId, second.modelProfile.modelProfileId);
  assert.equal(
    service.getRuntimeSnapshot().selection.modelProfileId,
    second.modelProfile.modelProfileId
  );
  assert.deepEqual(log, [
    `profile:${second.modelProfile.modelProfileId}`,
    `credential:${second.modelProfile.modelProfileId}`,
    `runtime:${second.modelProfile.modelProfileId}`,
    `state:${second.modelProfile.modelProfileId}`,
    `disposed:${first.modelProfile.modelProfileId}`
  ]);
});

test("ProviderService switches a real registered Runtime without rewriting workspace config", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-provider-model-selection-"));
  const stateRoot = join(root, "workspace-state");
  const userRoot = join(root, "user-state");
  const configManager = new ConfigManager(stateRoot);
  const credentialStore = new CredentialStore({
    userConfigDir: userRoot,
    keyring: null,
    env: {
      MINIMAX_API_KEY: "official-fixture-key",
      HASHSIGHT_API_KEY: "hashsight-fixture-key"
    }
  });
  const registry = new ModelProfileRegistry(
    createDefaultProviderAdapterRegistry(new BuiltinProviderAdapter()),
    new UserProfileStore({userConfigDir: userRoot})
  );
  const service = new ProviderService(
    configManager,
    credentialStore,
    undefined,
    registry,
    {stateStore: new ModelStateStore({userConfigDir: userRoot})}
  );

  try {
    await configManager.save(DEFAULT_CONFIG);
    const before = await readFile(join(stateRoot, "config.json"), "utf8");
    await service.init();
    const target = registry
      .listModels()
      .find(
        (candidate) =>
          candidate.providerProfile.providerProfileId ===
          "provider:minimax/hashsight"
      );
    assert.ok(target);

    const changed = await service.switchModel(
      target.modelProfile.modelProfileId
    );

    assert.equal(changed.providerProfileId, "provider:minimax/hashsight");
    assert.equal(
      service.getRuntimeSnapshot().selection.modelProfileId,
      target.modelProfile.modelProfileId
    );
    assert.equal(await readFile(join(stateRoot, "config.json"), "utf8"), before);
    assert.deepEqual(
      JSON.parse(await readFile(join(userRoot, "model-state.json"), "utf8")),
      {
        schemaVersion: 1,
        lastSelectedModelProfileId: target.modelProfile.modelProfileId
      }
    );
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("credential, runtime, and state failures preserve the previous pointer and Runtime", async () => {
  for (const failure of ["credential", "runtime", "state"] as const) {
    const log: string[] = [];
    const adapter = new FakeAdapter(log);
    const first = entry(adapter, `first-${failure}`);
    const second = entry(adapter, `second-${failure}`);
    const catalog = new FakeCatalog(
      [first, second],
      first.modelProfile.modelProfileId,
      log
    );
    const state = new FakeStateStore(selected(first.modelProfile.modelProfileId));
    const locator = new FakeCredentialLocator(log);
    const service = new ModelSelectionService(catalog, state, locator);
    await service.initialize(0.9);
    const oldRuntime = service.getRuntimeSnapshot().runtime as FakeRuntime;
    if (failure === "credential") {
      locator.missingModelId = second.modelProfile.modelProfileId;
    } else if (failure === "runtime") {
      adapter.failModelId = second.modelProfile.modelProfileId;
    } else {
      state.failSave = true;
    }

    await assert.rejects(() => service.switchModel(second.modelProfile.modelProfileId));

    assert.equal(
      service.getRuntimeSnapshot().selection.modelProfileId,
      first.modelProfile.modelProfileId,
      failure
    );
    assert.equal(oldRuntime.disposed, false, failure);
    assert.equal(
      state.loadResult.status === "selected"
        ? state.loadResult.state.lastSelectedModelProfileId
        : undefined,
      first.modelProfile.modelProfileId,
      failure
    );
    if (failure === "state") {
      assert.equal(
        adapter.runtimes.get(second.modelProfile.modelProfileId)?.disposed,
        true
      );
    }
  }
});

test("active Turns and workspace-only legacy profiles cannot change the sticky model", async () => {
  const log: string[] = [];
  const adapter = new FakeAdapter(log);
  const first = entry(adapter, "first");
  const legacy = entry(adapter, "legacy", {
    source: "legacy_workspace",
    stickyEligible: false
  });
  const catalog = new FakeCatalog(
    [first, legacy],
    first.modelProfile.modelProfileId,
    log
  );
  const state = new FakeStateStore({status: "unselected"});
  let active = false;
  const service = new ModelSelectionService(
    catalog,
    state,
    new FakeCredentialLocator(log),
    {isTurnActive: () => active}
  );
  await service.initialize(0.9);

  active = true;
  await assert.rejects(
    () => service.switchModel(first.modelProfile.modelProfileId),
    (error: unknown) =>
      error instanceof ModelSelectionError && error.code === "turn_active"
  );
  active = false;
  await assert.rejects(
    () => service.switchModel(legacy.modelProfile.modelProfileId),
    (error: unknown) =>
      error instanceof ModelSelectionError &&
      error.code === "sticky_selection_forbidden"
  );
  assert.deepEqual(state.saved, []);
});

test("switching changes only user model state and leaves workspace files byte-identical", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-model-switch-workspace-"));
  const workspace = join(root, "workspace.json");
  const history = join(root, "thread.jsonl");
  const workspaceBytes = "{\"model\":\"legacy\"}";
  const historyBytes = "{\"type\":\"user_message\"}\n";
  const log: string[] = [];
  const adapter = new FakeAdapter(log);
  const first = entry(adapter, "first");
  const second = entry(adapter, "second");
  const service = new ModelSelectionService(
    new FakeCatalog(
      [first, second],
      first.modelProfile.modelProfileId,
      log
    ),
    new FakeStateStore({status: "unselected"}),
    new FakeCredentialLocator(log)
  );

  try {
    await writeFile(workspace, workspaceBytes, "utf8");
    await writeFile(history, historyBytes, "utf8");
    await service.initialize(0.9);
    await service.switchModel(second.modelProfile.modelProfileId);

    assert.equal(await readFile(workspace, "utf8"), workspaceBytes);
    assert.equal(await readFile(history, "utf8"), historyBytes);
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("a chat-only model remains selectable but fails Agent compatibility before a request", async () => {
  const log: string[] = [];
  const adapter = new FakeAdapter(log);
  const chatOnly = entry(adapter, "chat-only", {nativeToolCalls: false});
  const service = new ModelSelectionService(
    new FakeCatalog(
      [chatOnly],
      chatOnly.modelProfile.modelProfileId,
      log
    ),
    new FakeStateStore({status: "unselected"}),
    new FakeCredentialLocator(log)
  );

  const selection = await service.initialize(0.9);

  assert.equal(selection.supportsNativeToolCalls, false);
  assert.throws(
    () => service.assertAgentCompatible(),
    (error: unknown) =>
      error instanceof ModelSelectionError &&
      error.code === "agent_feature_unsupported"
  );
});

test("dangling or corrupt sticky state enters explicit recovery", async () => {
  const log: string[] = [];
  const adapter = new FakeAdapter(log);
  const fallback = entry(adapter, "fallback");
  const catalog = new FakeCatalog(
    [fallback],
    fallback.modelProfile.modelProfileId,
    log
  );

  for (const stateResult of [
    selected("model:test/missing/model"),
    {status: "recovery_required", statePath: "model-state.json"} as const
  ]) {
    const service = new ModelSelectionService(
      catalog,
      new FakeStateStore(stateResult),
      new FakeCredentialLocator(log)
    );
    await assert.rejects(
      () => service.initialize(0.9),
      (error: unknown) =>
        error instanceof ModelSelectionError &&
        error.code === "recovery_required" &&
        error.configuredDefaultModelProfileId ===
          fallback.modelProfile.modelProfileId
    );
  }
});
