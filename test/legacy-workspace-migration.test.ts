import assert from "node:assert/strict";
import {access, cp, mkdtemp, readFile, rm} from "node:fs/promises";
import {tmpdir} from "node:os";
import {dirname, join, resolve} from "node:path";
import {fileURLToPath} from "node:url";
import test from "node:test";
import {CredentialStore} from "../src/config/credential-store.js";
import type {
  ProviderGateway,
  ProviderGatewayEvent,
  ProviderRequest
} from "../src/providers/provider-gateway.js";
import {ApplicationKernel} from "../src/runtime/application-kernel.js";
import {JsonlStorageProvider} from "../src/storage/jsonl-storage.js";

const FIXTURE_ROOT = join(dirname(fileURLToPath(import.meta.url)), "fixtures", "legacy-v0");

class CompletingProvider implements ProviderGateway {
  readonly requests: ProviderRequest[] = [];

  async *stream(request: ProviderRequest): AsyncGenerator<ProviderGatewayEvent> {
    this.requests.push(request);
    yield {type: "text.delta", delta: "after migration connected"};
    yield {type: "completed"};
  }
}

async function exists(filePath: string): Promise<boolean> {
  try {
    await access(filePath);
    return true;
  } catch {
    return false;
  }
}

test("a byte-stable version-0 workspace migrates and remains fully usable", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-legacy-workspace-"));
  const fixtureFiles = [
    "config.json",
    join("sessions", "2026", "07", "thread_legacy.jsonl"),
    join("turns", "thread_legacy.turns.jsonl")
  ];
  const originals = new Map(
    await Promise.all(
      fixtureFiles.map(async (relativePath) => [
        relativePath,
        await readFile(join(FIXTURE_ROOT, relativePath))
      ] as const)
    )
  );
  const provider = new CompletingProvider();
  const app = new ApplicationKernel({
    cwd: root,
    stateRoot: root,
    credentialStore: new CredentialStore({
      keyring: null,
      userConfigDir: join(root, "user-config"),
      env: {MINIMAX_API_KEY: "offline-fixture-key"}
    }),
    providerGateway: provider
  });

  try {
    await cp(FIXTURE_ROOT, root, {recursive: true});

    const initEvents = await app.init();
    const history = initEvents.find((event) => event.type === "history.loaded");
    assert.equal(history?.type, "history.loaded");
    assert.deepEqual(
      history?.type === "history.loaded" ? history.items.map((item) => item.content) : [],
      ["legacy question", "legacy answer"]
    );

    const manifest = JSON.parse(await readFile(join(root, "manifest.json"), "utf8")) as {
      schemaVersion: number;
      storage: string;
    };
    assert.equal(manifest.schemaVersion, 1);
    assert.equal(manifest.storage, "jsonl");
    const migratedConfig = JSON.parse(await readFile(join(root, "config.json"), "utf8")) as {
      schemaVersion?: number;
      api?: unknown;
      storage?: unknown;
    };
    assert.equal(migratedConfig.schemaVersion, 1);
    assert.equal(migratedConfig.api, undefined);
    assert.equal(migratedConfig.storage, undefined);
    for (const [relativePath, original] of originals) {
      assert.equal(
        await exists(join(root, `${relativePath}.v0.bak`)),
        true,
        `${relativePath} should retain a v0 backup`
      );
      assert.deepEqual(await readFile(join(root, `${relativePath}.v0.bak`)), original);
    }

    const turnEvents = [];
    for await (const event of app.dispatch({type: "turn.submit", input: "after migration"})) {
      turnEvents.push(event);
    }
    assert.equal(
      turnEvents.some((event) => event.type === "assistant.completed"),
      true
    );
    assert.equal(provider.requests.length, 1);

    const snapshot = await new JsonlStorageProvider(root).readThread("thread_legacy");
    assert.deepEqual(
      snapshot.items.map((item) => item.content),
      ["legacy question", "legacy answer", "after migration", "after migration connected"]
    );
    assert.equal(snapshot.turns.length, 2);
    assert.equal(snapshot.turns.at(-1)?.status, "completed");
  } finally {
    await app.shutdown("user");
    await rm(root, {recursive: true, force: true});
  }
});

test("automated scripts never invoke the live Provider smoke entrypoint", async () => {
  const projectRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
  const packageJson = JSON.parse(await readFile(join(projectRoot, "package.json"), "utf8")) as {
    scripts: Record<string, string>;
  };
  const smokeSource = await readFile(
    join(projectRoot, "src", "smoke", "provider-smoke.ts"),
    "utf8"
  );
  const readme = await readFile(join(projectRoot, "README.md"), "utf8");

  assert.equal(packageJson.scripts["smoke:provider"], "tsx src/smoke/provider-smoke.ts");
  for (const automatedScript of ["test", "check", "build"]) {
    assert.equal(packageJson.scripts[automatedScript]?.includes("smoke:provider"), false);
    assert.equal(packageJson.scripts[automatedScript]?.includes("provider-smoke"), false);
  }
  assert.match(smokeSource, /process\.argv\.length\s*>\s*2/u);
  assert.equal(smokeSource.includes("apiKey:"), false);
  assert.equal(smokeSource.includes("raw"), false);
  assert.equal(smokeSource.includes("console.log"), false);
  assert.match(readme, /check.*build.*compile.*smoke.*source/is);
  assert.match(readme, /offline test.*static.*smoke/is);
  assert.match(readme, /never invoke.*npm run smoke:provider/is);
});
