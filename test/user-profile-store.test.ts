import assert from "node:assert/strict";
import {access, mkdtemp, readFile, rm, writeFile} from "node:fs/promises";
import {tmpdir} from "node:os";
import {join} from "node:path";
import test from "node:test";
import {UserProfileStore} from "../src/config/user-profile-store.js";
import {
  createModelProfileFixture,
  createProviderProfileFixture
} from "./support/provider-conformance-suite.js";

test("user provider and model profiles persist separately without sticky state", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-user-profiles-"));
  const store = new UserProfileStore({userConfigDir: root});
  const provider = createProviderProfileFixture("responses", "provider:user/example");
  const model = createModelProfileFixture(provider, {
    modelProfileId: "model:user/example/model-a"
  });

  try {
    await store.saveProviderProfile(provider);
    await store.saveModelProfile(model);

    const providerFile = JSON.parse(await readFile(store.providerProfilesPath, "utf8"));
    const modelFile = JSON.parse(await readFile(store.modelProfilesPath, "utf8"));
    assert.deepEqual(providerFile, {schemaVersion: 1, profiles: [provider]});
    assert.deepEqual(modelFile, {schemaVersion: 1, profiles: [model]});
    assert.equal(JSON.stringify(providerFile).includes("apiKey"), false);
    assert.equal(JSON.stringify(modelFile).includes("permissionMode"), false);
    await assert.rejects(access(join(root, "model-state.json")), {code: "ENOENT"});
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("user profile writes reject credential and permission fields", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-user-profile-authority-"));
  const store = new UserProfileStore({userConfigDir: root});
  const provider = createProviderProfileFixture("responses", "provider:user/authority");
  const model = createModelProfileFixture(provider, {
    modelProfileId: "model:user/authority/model-a"
  });

  try {
    await assert.rejects(
      () => store.saveProviderProfile({...provider, apiKey: "forbidden"} as never),
      /provider contract validation/i
    );
    await assert.rejects(
      () => store.saveModelProfile({...model, permissionMode: "full_access"} as never),
      /provider contract validation/i
    );
    await assert.rejects(access(store.providerProfilesPath), {code: "ENOENT"});
    await assert.rejects(access(store.modelProfilesPath), {code: "ENOENT"});
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("invalid and duplicate optional profiles are quarantined without hiding valid peers", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-user-profile-quarantine-"));
  const store = new UserProfileStore({userConfigDir: root});
  const valid = createProviderProfileFixture("responses", "provider:user/valid");
  const duplicate = createProviderProfileFixture("responses", "provider:user/duplicate");

  try {
    await writeFile(
      store.providerProfilesPath,
      JSON.stringify({
        schemaVersion: 1,
        profiles: [
          valid,
          {...valid, providerProfileId: "not-qualified"},
          duplicate,
          {...duplicate, displayName: "same id again"}
        ]
      }),
      "utf8"
    );

    const snapshot = await store.load();

    assert.deepEqual(snapshot.providerProfiles.map((profile) => profile.providerProfileId), [
      valid.providerProfileId
    ]);
    assert.equal(snapshot.issues.some((issue) => issue.code === "invalid_profile"), true);
    assert.equal(
      snapshot.issues.some((issue) => issue.code === "duplicate_profile_id"),
      true
    );
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("user profile files recover their last valid backup atomically", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-user-profile-backup-"));
  const store = new UserProfileStore({userConfigDir: root});
  const first = createProviderProfileFixture("responses", "provider:user/first");
  const second = createProviderProfileFixture("responses", "provider:user/second");

  try {
    await store.saveProviderProfile(first);
    await store.saveProviderProfile(second);
    await writeFile(store.providerProfilesPath, "{broken", "utf8");

    const snapshot = await store.load();

    assert.deepEqual(snapshot.providerProfiles, [first]);
    assert.deepEqual(JSON.parse(await readFile(store.providerProfilesPath, "utf8")), {
      schemaVersion: 1,
      profiles: [first]
    });
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("an unrecoverable optional profile file degrades to an isolated issue", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-user-profile-corrupt-"));
  const store = new UserProfileStore({userConfigDir: root});

  try {
    await writeFile(store.modelProfilesPath, "{broken", "utf8");
    await writeFile(`${store.modelProfilesPath}.bak`, "[]", "utf8");

    const snapshot = await store.load();

    assert.deepEqual(snapshot.modelProfiles, []);
    assert.equal(
      snapshot.issues.some((issue) => issue.code === "store_recovery_required"),
      true
    );
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});
