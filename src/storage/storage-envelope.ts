export type Validator<T> = (value: unknown) => value is T;

export interface StoredEnvelope<T> {
  schemaVersion: 1;
  sequence: number;
  kind: string;
  payload: T;
  createdAt: string;
}

export function createStoredEnvelope<T>(
  kind: string,
  payload: T,
  sequence: number,
  createdAt: string
): StoredEnvelope<T> {
  if (!kind) {
    throw new Error("Stored envelope kind must not be empty.");
  }
  if (!Number.isSafeInteger(sequence) || sequence < 1) {
    throw new Error("Stored envelope sequence must be a positive safe integer.");
  }
  if (!createdAt) {
    throw new Error("Stored envelope createdAt must not be empty.");
  }
  return {schemaVersion: 1, sequence, kind, payload, createdAt};
}

export function inspectStoredEnvelope(value: unknown): StoredEnvelope<unknown> | null {
  if (!isRecord(value) || !("schemaVersion" in value)) {
    return null;
  }
  if (value.schemaVersion !== 1) {
    throw new Error(`Unknown storage schema version ${String(value.schemaVersion)}.`);
  }
  if (
    !Number.isSafeInteger(value.sequence) ||
    (value.sequence as number) < 1 ||
    typeof value.kind !== "string" ||
    value.kind.length === 0 ||
    !("payload" in value) ||
    typeof value.createdAt !== "string" ||
    value.createdAt.length === 0
  ) {
    throw new Error("Structurally invalid version-1 storage envelope.");
  }
  return value as unknown as StoredEnvelope<unknown>;
}

export function unwrapStoredRecord<T>(
  value: unknown,
  kind: string,
  validate: Validator<T>
): T {
  const envelope = inspectStoredEnvelope(value);
  const payload = envelope?.payload ?? value;
  if (envelope && envelope.kind !== kind) {
    throw new Error(`Unexpected stored record kind ${envelope.kind}; expected ${kind}.`);
  }
  if (!validate(payload)) {
    throw new Error(`Structurally invalid ${kind} record.`);
  }
  return payload;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
