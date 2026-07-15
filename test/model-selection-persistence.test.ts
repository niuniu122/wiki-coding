import assert from "node:assert/strict";
import {mkdtemp, rm} from "node:fs/promises";
import {tmpdir} from "node:os";
import {join} from "node:path";
import test from "node:test";
import {JsonlStorageProvider} from "../src/storage/jsonl-storage.js";
import {
  ModelSelectionError,
  ModelSelectionService
} from "../src/runtime/model-selection-service.js";
import {SessionService} from "../src/runtime/session-service.js";
import type {TurnModelProvenance} from "../src/types.js";

const M1: TurnModelProvenance = Object.freeze({
  schemaVersion: 1,
  adapterId: "adapter:minimax/builtin",
  providerProfileId: "provider:minimax/official",
  modelProfileId: "model:minimax/official/MiniMax-M3",
  model: "MiniMax-M3",
  protocol: "responses"
});

const M2: TurnModelProvenance = Object.freeze({
  ...M1,
  providerProfileId: "provider:minimax/hashsight",
  modelProfileId: "model:minimax/hashsight/MiniMax-M2.1",
  model: "MiniMax-M2.1",
  protocol: "chat_completions"
});

test("historical Turns retain the model that actually served them", async () => {
  const root = await mkdtemp(join(tmpdir(), "minimax-turn-provenance-"));
  try {
    const repository = new JsonlStorageProvider(root);
    const session = new SessionService(repository);
    await session.init(M1.model, root);

    const first = await session.createTurn("first", M1);
    await session.completeTurn(first, "completed");
    const second = await session.createTurn("second", M2);
    await session.completeTurn(second, "completed");
    const legacy = await session.createTurn("legacy-compatible");
    await session.completeTurn(legacy, "completed");

    const snapshot = await repository.readThread(session.activeThread.id);
    assert.deepEqual(snapshot.turns[0]?.modelProvenance, M1);
    assert.deepEqual(snapshot.turns[1]?.modelProvenance, M2);
    assert.equal(snapshot.turns[2]?.modelProvenance, undefined);
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});

test("a sticky model with a missing credential enters explicit recovery", async () => {
  const configuredDefault = {
    modelProfile: {modelProfileId: M1.modelProfileId}
  };
  const sticky = {
    ...configuredDefault,
    stickyEligible: true
  };
  const service = new ModelSelectionService(
    {
      getConfiguredDefault: () => configuredDefault,
      getModel: () => sticky
    } as never,
    {
      async load() {
        return {
          status: "selected" as const,
          state: {
            schemaVersion: 1 as const,
            lastSelectedModelProfileId: M1.modelProfileId
          }
        };
      },
      async save() {}
    } as never,
    {
      async locate() {
        return {
          hasCredential: false,
          handle: {targetId: "missing", async readSecret() { return ""; }}
        };
      }
    }
  );

  await assert.rejects(
    service.initialize(0.9),
    (error: unknown) =>
      error instanceof ModelSelectionError &&
      error.code === "recovery_required" &&
      error.configuredDefaultModelProfileId === M1.modelProfileId
  );
});
