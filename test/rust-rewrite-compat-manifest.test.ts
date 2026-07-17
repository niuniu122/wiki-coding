import assert from "node:assert/strict";
import {readFile} from "node:fs/promises";
import test from "node:test";

const LOCKED_COMMANDS = [
  "/interrupt",
  "/new",
  "/threads",
  "/resume",
  "/compact",
  "/api",
  "/provider",
  "/continue",
  "/agent",
  "/chat",
  "/models",
  "/model",
  "/capabilities",
  "/permissions",
  "/trace",
  "/retry",
  "/exit",
  "/quit"
] as const;

const PROFILE_CLASSES = [
  "custom_openai_compatible",
  "minimax_hashsight",
  "minimax_official"
] as const;

const PROTOCOLS = ["chat_completions", "responses"] as const;
const STATUSES = new Set(["matched", "pending", "approved_difference"]);
const ARGUMENT_SHAPES = new Set(["none", "optional", "required"]);

interface CompatFixtures {
  commands: unknown;
  providers: unknown;
  publicContract: unknown;
  invalidCases: unknown;
  validStreams: readonly unknown[];
}

test("Rust compatibility fixtures preserve the immutable public contract", async () => {
  const fixtures = await loadCompatFixtures();
  assert.doesNotThrow(() => validateCompatFixtures(fixtures));
});

test("compatibility validation rejects duplicate commands and aliases", async () => {
  const fixtures = await loadCompatFixtures();
  const commands = structuredClone(asRecord(fixtures.commands));
  const entries = asArray(commands.commands);
  entries.push(structuredClone(entries[0]));

  assert.throws(
    () => validateCompatFixtures({...fixtures, commands}),
    /duplicate command or alias/i
  );
});

test("compatibility validation rejects secret values", async () => {
  const fixtures = await loadCompatFixtures();
  const providers = structuredClone(asRecord(fixtures.providers));
  const firstProfile = asRecord(asArray(providers.profileClasses)[0]);
  firstProfile.credentialBindings = ["sk-fixture-value-that-must-be-rejected"];

  assert.throws(
    () => validateCompatFixtures({...fixtures, providers}),
    /secret-like value/i
  );
});

test("compatibility validation rejects contract status without Rust evidence", async () => {
  const fixtures = await loadCompatFixtures();
  const publicContract = structuredClone(asRecord(fixtures.publicContract));
  const matched = asRecord(asArray(publicContract.items).find((item) => {
    return asRecord(item).status === "matched";
  }));
  matched.evidence = [];

  assert.throws(
    () => validateCompatFixtures({...fixtures, publicContract}),
    /public contract item requires Rust evidence/i
  );
});

export function validateCompatFixtures(fixtures: CompatFixtures): void {
  assertSchemaVersion(fixtures.commands, "commands");
  assertSchemaVersion(fixtures.providers, "providers");
  assertSchemaVersion(fixtures.publicContract, "public contract");
  assertSchemaVersion(fixtures.invalidCases, "invalid provider cases");

  const commandManifest = asRecord(fixtures.commands);
  const commandEntries = asArray(commandManifest.commands).map(asRecord);
  const allNames = commandEntries.flatMap((entry) => [
    asString(entry.name),
    ...asArray(entry.aliases).map(asString)
  ]);
  assert.equal(
    new Set(allNames).size,
    allNames.length,
    "duplicate command or alias"
  );
  assert.deepEqual([...allNames].sort(), [...LOCKED_COMMANDS].sort());
  for (const entry of commandEntries) {
    assert.equal(ARGUMENT_SHAPES.has(asString(entry.argument)), true);
    assert.notEqual(asString(entry.outcome).trim(), "");
  }
  assert.deepEqual(commandManifest.targetPermissionModes, [
    "confirm",
    "full-access"
  ]);

  const providerManifest = asRecord(fixtures.providers);
  assert.deepEqual(
    asArray(providerManifest.profileClasses)
      .map((profile) => asString(asRecord(profile).id))
      .sort(),
    [...PROFILE_CLASSES]
  );
  assert.deepEqual(
    asArray(providerManifest.protocols).map(asString).sort(),
    [...PROTOCOLS]
  );

  const contractManifest = asRecord(fixtures.publicContract);
  assert.equal(contractManifest.contractVersion, "v1");
  assert.equal(contractManifest.provenanceCommit, "84784f5");
  assert.match(asString(contractManifest.contentFingerprint), /^sha256:[a-f0-9]{64}$/u);
  assert.equal(contractManifest.productEntry, "bin/minimax-codex.cjs");
  const requiredItemIds = asArray(contractManifest.requiredItemIds).map(asString);
  const contractItems = asArray(contractManifest.items).map(asRecord);
  assert.deepEqual(
    contractItems.map((item) => asString(item.id)).sort(),
    [...requiredItemIds].sort()
  );
  assert.equal(new Set(requiredItemIds).size, requiredItemIds.length);
  for (const item of contractItems) {
    const id = asString(item.id);
    const status = asString(item.status);
    assert.match(id, /^contract\./u);
    assert.doesNotMatch(id, /^(?:rust|typescript)\./u);
    assert.equal(STATUSES.has(status), true, `unsupported status: ${status}`);
    const evidence = asArray(item.evidence).map(asString);
    assert.notEqual(evidence.length, 0, "public contract item requires Rust evidence");
    assert.notEqual(status, "pending", `public contract item is pending: ${id}`);
    if (status === "approved_difference") {
      assert.match(asString(item.approvedDifference), /^difference\.command\./u);
    }
  }

  const invalidCases = asArray(asRecord(fixtures.invalidCases).cases).map(asRecord);
  assert.deepEqual(
    invalidCases
      .map((item) => asString(asRecord(item.expected_error).code))
      .sort(),
    [
      "duplicate_terminal",
      "duplicate_tool_call_id",
      "event_after_terminal",
      "invalid_tool_arguments",
      "malformed_json",
      "missing_call_id",
      "premature_eof"
    ]
  );
  for (const value of fixtures.validStreams) {
    const stream = asRecord(value);
    assert.notEqual(asString(stream.case_id), "");
    assert.equal(PROTOCOLS.includes(asString(stream.protocol) as never), true);
    assert.notEqual(asArray(stream.raw).length, 0);
    assert.notEqual(asArray(stream.expected_events).length, 0);
  }
  for (const protocol of PROTOCOLS) {
    const events = fixtures.validStreams
      .map(asRecord)
      .filter((stream) => stream.protocol === protocol)
      .flatMap((stream) => asArray(stream.expected_events).map(asRecord));
    for (const type of [
      "text_delta",
      "reasoning_filtered",
      "usage",
      "tool_call",
      "completed"
    ]) {
      assert.equal(
        events.some((event) => event.type === type),
        true,
        `${protocol} fixture is missing ${type}`
      );
    }
    const toolCall = events.find((event) => event.type === "tool_call");
    assert.equal(toolCall?.call_id, "call-1");
    assert.equal(toolCall?.name, "invoke_local_capability");
  }

  assertNoSecretMaterial(fixtures);
}

async function loadCompatFixtures(): Promise<CompatFixtures> {
  const [commands, providers, publicContract, invalidCases, responses, chatCompletions] =
    await Promise.all([
      readJson("../fixtures/compat/commands.v1.json"),
      readJson("../fixtures/compat/providers.v1.json"),
      readJson("../fixtures/compat/public-contract.v1.json"),
      readJson("../fixtures/compat/provider-streams/invalid-cases.v1.json"),
      readJsonLines("../fixtures/compat/provider-streams/responses.valid.jsonl"),
      readJsonLines(
        "../fixtures/compat/provider-streams/chat-completions.valid.jsonl"
      )
    ]);
  return {
    commands,
    providers,
    publicContract,
    invalidCases,
    validStreams: [...responses, ...chatCompletions]
  };
}

async function readJson(relativePath: string): Promise<unknown> {
  return JSON.parse(await readFile(new URL(relativePath, import.meta.url), "utf8")) as unknown;
}

async function readJsonLines(relativePath: string): Promise<unknown[]> {
  const content = await readFile(new URL(relativePath, import.meta.url), "utf8");
  return content
    .split(/\r?\n/u)
    .filter((line) => line.trim() !== "")
    .map((line) => JSON.parse(line) as unknown);
}

function assertSchemaVersion(value: unknown, label: string): void {
  assert.equal(asRecord(value).schemaVersion, 1, `${label} schemaVersion`);
}

function assertNoSecretMaterial(value: unknown, key = "root"): void {
  if (typeof value === "string") {
    assert.doesNotMatch(
      value,
      /(?:^sk-[a-z0-9_-]{16,}$|^bearer\s+\S{12,}$)/iu,
      `secret-like value at ${key}`
    );
    return;
  }
  if (Array.isArray(value)) {
    value.forEach((entry, index) => assertNoSecretMaterial(entry, `${key}[${index}]`));
    return;
  }
  if (value === null || typeof value !== "object") {
    return;
  }
  for (const [childKey, child] of Object.entries(value)) {
    assert.doesNotMatch(
      childKey,
      /^(?:api[-_]?key|authorization|secret|token)$/iu,
      `secret field name at ${key}.${childKey}`
    );
    assertNoSecretMaterial(child, `${key}.${childKey}`);
  }
}

function asRecord(value: unknown): Record<string, unknown> {
  assert.equal(typeof value, "object");
  assert.notEqual(value, null);
  assert.equal(Array.isArray(value), false);
  return value as Record<string, unknown>;
}

function asArray(value: unknown): unknown[] {
  assert.equal(Array.isArray(value), true);
  return value as unknown[];
}

function asString(value: unknown): string {
  assert.equal(typeof value, "string");
  return value as string;
}
