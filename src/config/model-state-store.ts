import {join, resolve} from "node:path";
import type {ModelProfileId} from "../types.js";
import {parseModelProfileId} from "../providers/provider-adapter.js";
import {readJsonFile, writeJsonFile} from "../utils/jsonl.js";
import {resolveUserConfigRoot} from "./user-config-root.js";

export const MODEL_STATE_SCHEMA_VERSION = 1 as const;

export interface ActiveModelState {
  readonly schemaVersion: 1;
  readonly lastSelectedModelProfileId: ModelProfileId;
}

export type ModelStateLoadResult =
  | {readonly status: "unselected"}
  | {readonly status: "selected"; readonly state: ActiveModelState}
  | {readonly status: "recovery_required"; readonly statePath: string};

export type ModelStateValidationErrorCode =
  | "invalid_type"
  | "missing_field"
  | "unknown_field"
  | "unsupported_schema_version"
  | "invalid_model_profile_id";

export class ModelStateValidationError extends Error {
  constructor(
    readonly code: ModelStateValidationErrorCode,
    readonly path: string
  ) {
    super(`Model state validation failed at ${path} (${code}).`);
    this.name = "ModelStateValidationError";
  }
}

export type ModelStateWriter = (
  statePath: string,
  state: ActiveModelState
) => Promise<void>;

export interface ModelStateStoreOptions {
  readonly userConfigDir?: string;
  readonly writeStateFile?: ModelStateWriter;
}

const MODEL_STATE_KEYS = ["schemaVersion", "lastSelectedModelProfileId"] as const;
const MISSING_STATE = Symbol("missing-model-state");

export class ModelStateStore {
  readonly statePath: string;
  private readonly writeStateFile: ModelStateWriter;

  constructor(options: ModelStateStoreOptions = {}) {
    const userConfigDir = resolve(options.userConfigDir ?? resolveUserConfigRoot());
    this.statePath = join(userConfigDir, "model-state.json");
    this.writeStateFile =
      options.writeStateFile ??
      ((statePath, state) => writeJsonFile(statePath, state, {mode: 0o600}));
  }

  async load(): Promise<ModelStateLoadResult> {
    try {
      const state = await readJsonFile<ActiveModelState | typeof MISSING_STATE>(
        this.statePath,
        MISSING_STATE,
        {parse: parseActiveModelState}
      );
      if (state === MISSING_STATE) {
        return {status: "unselected"};
      }
      return {status: "selected", state};
    } catch {
      return {status: "recovery_required", statePath: this.statePath};
    }
  }

  async save(lastSelectedModelProfileId: string): Promise<ActiveModelState> {
    const state = parseActiveModelState({
      schemaVersion: MODEL_STATE_SCHEMA_VERSION,
      lastSelectedModelProfileId
    });
    await this.writeStateFile(this.statePath, state);
    return state;
  }
}

export function parseActiveModelState(value: unknown): ActiveModelState {
  if (!isRecord(value)) {
    throw new ModelStateValidationError("invalid_type", "modelState");
  }

  for (const key of Object.keys(value)) {
    if (!(MODEL_STATE_KEYS as readonly string[]).includes(key)) {
      throw new ModelStateValidationError("unknown_field", `modelState.${key}`);
    }
  }
  for (const key of MODEL_STATE_KEYS) {
    if (!Object.hasOwn(value, key)) {
      throw new ModelStateValidationError("missing_field", `modelState.${key}`);
    }
  }

  if (value.schemaVersion !== MODEL_STATE_SCHEMA_VERSION) {
    throw new ModelStateValidationError(
      "unsupported_schema_version",
      "modelState.schemaVersion"
    );
  }

  let lastSelectedModelProfileId: ModelProfileId;
  try {
    lastSelectedModelProfileId = parseModelProfileId(
      value.lastSelectedModelProfileId,
      "modelState.lastSelectedModelProfileId"
    );
  } catch {
    throw new ModelStateValidationError(
      "invalid_model_profile_id",
      "modelState.lastSelectedModelProfileId"
    );
  }

  return Object.freeze({
    schemaVersion: MODEL_STATE_SCHEMA_VERSION,
    lastSelectedModelProfileId
  });
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
