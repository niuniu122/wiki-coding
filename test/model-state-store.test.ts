import assert from "node:assert/strict";
import {access, mkdtemp, readFile, rm, writeFile} from "node:fs/promises";
import {tmpdir} from "node:os";
import {join} from "node:path";
import test from "node:test";
import {
  ModelStateStore,
  ModelStateValidationError,
  parseActiveModelState
} from "../src/config/model-state-store.js";

const FIRST_MODEL_ID = "model:minimax/minimax-text-01";
const SECOND_MODEL_ID = "model:openai/gpt-5";

test("missing model state is unselected and does not create a file", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-model-state-missing-"));
  const userConfigDir = join(root, "user-config");
  const store = new ModelStateStore({userConfigDir});

  try {
    assert.deepEqual(await store.load(), {status: "unselected"});
    await assert.rejects(access(store.statePath), {code: "ENOENT"});
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("model state persists only its schema and fully-qualified model profile id", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-model-state-schema-"));
  const store = new ModelStateStore({userConfigDir: root});

  try {
    await assert.rejects(() => store.save("minimax-text-01"), ModelStateValidationError);
    await store.save(FIRST_MODEL_ID);

    const persisted = JSON.parse(await readFile(store.statePath, "utf8")) as unknown;
    assert.deepEqual(persisted, {
      schemaVersion: 1,
      lastSelectedModelProfileId: FIRST_MODEL_ID
    });
    await assert.rejects(access(join(root, "credentials.json")), {code: "ENOENT"});
    await assert.rejects(access(join(root, "config.json")), {code: "ENOENT"});
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("model state rejects authority, secret, workspace, prompt, and endpoint fields", () => {
  const base = {
    schemaVersion: 1,
    lastSelectedModelProfileId: FIRST_MODEL_ID
  };

  for (const field of [
    "apiKey",
    "credential",
    "permissionMode",
    "workspacePath",
    "prompt",
    "systemPrompt",
    "endpoint",
    "headers",
    "baseUrl"
  ]) {
    assert.throws(
      () => parseActiveModelState({...base, [field]: "forbidden"}),
      (error: unknown) =>
        error instanceof ModelStateValidationError &&
        error.code === "unknown_field" &&
        error.path === `modelState.${field}`
    );
  }
});

test("model state rejects unknown schema versions and incomplete records", () => {
  assert.throws(
    () =>
      parseActiveModelState({
        schemaVersion: 2,
        lastSelectedModelProfileId: FIRST_MODEL_ID
      }),
    (error: unknown) =>
      error instanceof ModelStateValidationError &&
      error.code === "unsupported_schema_version" &&
      error.path === "modelState.schemaVersion"
  );
  assert.throws(
    () => parseActiveModelState({schemaVersion: 1}),
    (error: unknown) =>
      error instanceof ModelStateValidationError &&
      error.code === "missing_field" &&
      error.path === "modelState.lastSelectedModelProfileId"
  );
});

test("invalid primary and backup return explicit recovery instead of a default", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-model-state-corrupt-"));
  const store = new ModelStateStore({userConfigDir: root});

  try {
    await writeFile(store.statePath, "{broken", "utf8");
    await writeFile(`${store.statePath}.bak`, "{}", "utf8");

    assert.deepEqual(await store.load(), {
      status: "recovery_required",
      statePath: store.statePath
    });
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("a failed model-state write preserves the previous pointer", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-model-state-failed-write-"));
  const durableStore = new ModelStateStore({userConfigDir: root});

  try {
    await durableStore.save(FIRST_MODEL_ID);
    const failingStore = new ModelStateStore({
      userConfigDir: root,
      writeStateFile: async () => {
        throw new Error("simulated atomic replacement failure");
      }
    });

    await assert.rejects(() => failingStore.save(SECOND_MODEL_ID), /replacement failure/i);
    assert.deepEqual(await durableStore.load(), {
      status: "selected",
      state: {
        schemaVersion: 1,
        lastSelectedModelProfileId: FIRST_MODEL_ID
      }
    });
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});
