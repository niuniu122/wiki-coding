# MiniMax CLI Core Runtime Rewrite Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the monolithic CLI Runtime with a contract-first kernel while preserving current commands, Provider connectivity, and legacy workspace data.

**Architecture:** Keep `Command` and `RuntimeEvent` as the stable outer protocol. Build a new `ApplicationKernel` from a workspace lease, command arbiter, Provider service/gateway, session service/repository, Turn engine, credential store, and context engine; switch the UI only after offline end-to-end contracts pass.

**Tech Stack:** Node.js 20+, TypeScript 5.8, Ink 5, React 18, Node test runner, JSON/JSONL, Fetch/SSE, optional `@napi-rs/keyring` 1.3.

## Global Constraints

- Do not add `tool_call`, `tool_result`, file/shell/Git/MCP tools, permissions, sandboxing, hooks, plugins, or sub-agents.
- Preserve `/new`, `/threads`, `/resume`, `/api`, `/provider`, `/trace`, `/compact`, `/interrupt`, and `/exit` behavior.
- Preserve existing legacy sessions and configuration until a validated atomic migration commits.
- Permit only one live CLI process per `.mini-codex` workspace.
- Never treat stream EOF as Provider success; require a protocol terminal event.
- Never persist plaintext credentials without a visible warning and single-use user consent.
- Never run a real Provider request automatically.
- Every production behavior change starts with a focused failing test.
- Keep each task green and commit it before starting the next task.

---

### Task 1: Workspace Lease and Core Command Ownership

**Files:**
- Create: `src/runtime/workspace-lease.ts`
- Create: `src/runtime/command-arbiter.ts`
- Create: `test/workspace-lease.test.ts`
- Create: `test/command-arbiter.test.ts`
- Modify: `test/run-tests.ts`

**Interfaces:**
- Consumes: workspace state root path and current process identity.
- Produces: `WorkspaceLease.acquire()`, `WorkspaceLease.release()`, `CommandArbiter.begin()`, and `CommandArbiter.finish()`.

- [ ] **Step 1: Write failing lease and arbiter tests**

```ts
test("a second live owner cannot acquire the same workspace", async () => {
  const first = new WorkspaceLease(root, {pid: 100, isProcessAlive: () => true});
  const second = new WorkspaceLease(root, {pid: 200, isProcessAlive: () => true});
  await first.acquire();
  await assert.rejects(() => second.acquire(), /already open.*PID 100/i);
});

test("a dead owner is replaced without deleting the new owner", async () => {
  const stale = new WorkspaceLease(root, {pid: 100, isProcessAlive: () => false});
  await stale.acquire();
  const recovered = new WorkspaceLease(root, {pid: 200, isProcessAlive: () => false});
  await recovered.acquire();
  await stale.release();
  await assert.rejects(
    () => new WorkspaceLease(root, {pid: 300, isProcessAlive: () => true}).acquire(),
    /PID 200/
  );
});

test("only interrupt, shutdown, and read-only commands pass while a Turn runs", () => {
  const arbiter = new CommandArbiter();
  arbiter.markReady();
  const ownership = arbiter.begin({type: "turn.submit", input: "hello"});
  assert.equal(arbiter.canDispatch({type: "turn.interrupt"}), true);
  assert.equal(arbiter.canDispatch({type: "thread.list"}), true);
  assert.equal(arbiter.canDispatch({type: "thread.new"}), false);
  assert.equal(arbiter.canDispatch({type: "provider.switch", providerId: "hashsight"}), false);
  ownership.finish();
  assert.equal(arbiter.canDispatch({type: "thread.new"}), true);
});
```

- [ ] **Step 2: Run focused tests and verify RED**

Run: `npx tsx --test test/workspace-lease.test.ts test/command-arbiter.test.ts`
Expected: module-not-found failures for `workspace-lease.ts` and `command-arbiter.ts`.

- [ ] **Step 3: Implement nonce-safe lease ownership**

```ts
export interface WorkspaceLeaseOptions {
  pid?: number;
  isProcessAlive?: (pid: number) => boolean;
  now?: () => string;
}

export class WorkspaceLease {
  private nonce: string | null = null;
  constructor(private readonly stateRoot: string, private readonly options: WorkspaceLeaseOptions = {}) {}
  async acquire(): Promise<void>;
  async release(): Promise<void>;
}
```

Implementation requirements:

```ts
const lockDir = join(stateRoot, "locks", "runtime.lock");
await mkdir(lockDir); // atomic ownership attempt
await writeJsonFile(join(lockDir, "owner.json"), {pid, startedAt, workspace, nonce}, {backup: false});
```

On `EEXIST`, validate `owner.json`. Reject a live PID. For stale ownership, rename `runtime.lock` to `runtime.lock.stale.<uuid>`, retry `mkdir`, and remove only the renamed directory. `release()` removes the lock only after the stored nonce equals this instance's nonce.

- [ ] **Step 4: Implement kernel-level command policy**

```ts
export type KernelPhase = "booting" | "idle" | "running_turn" | "shutting_down" | "stopped";

export class CommandArbiter {
  private phase: KernelPhase = "booting";
  markReady(): void;
  canDispatch(command: Command): boolean;
  begin(command: Command): {finish(): void};
  beginShutdown(): void;
}
```

Only `turn.interrupt`, `thread.list`, `provider.list`, `trace.toggle`, and `app.exit` are concurrent with `running_turn`. `begin()` throws a typed busy error instead of queueing a mutating command.

- [ ] **Step 5: Run focused and full tests**

Run: `npx tsx --test test/workspace-lease.test.ts test/command-arbiter.test.ts`
Expected: both suites pass.
Run: `npm test`
Expected: existing 60 tests plus new lease/arbiter tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/runtime/workspace-lease.ts src/runtime/command-arbiter.ts test/workspace-lease.test.ts test/command-arbiter.test.ts test/run-tests.ts
git commit -m "feat: enforce runtime workspace ownership"
```

### Task 2: Fail-closed Provider Stream State Machines

**Files:**
- Create: `src/providers/provider-gateway.ts`
- Modify: `src/providers/provider-protocol.ts`
- Modify: `src/providers/provider-model-adapter.ts`
- Modify: `src/providers/provider-error.ts`
- Modify: `src/runtime/model-adapter.ts`
- Modify: `test/provider-protocol.test.ts`
- Modify: `test/provider-model-adapter.test.ts`

**Interfaces:**
- Consumes: SSE data frames and existing Provider configuration.
- Produces: `ProviderGateway`, normalized `text.delta`, `usage`, `diagnostic`, and `completed` events; throws `ProviderError(kind="protocol")` for invalid lifecycle state. `ProviderModelAdapter` remains as a temporary compatibility wrapper for the old Runtime.

- [ ] **Step 1: Add failing protocol lifecycle tests**

```ts
test("responses requires response.completed", async () => {
  const adapter = adapterForSse([
    {type: "response.output_text.delta", delta: "partial"}
  ]);
  await assert.rejects(() => collect(adapter), (error: unknown) =>
    error instanceof ProviderError && error.kind === "protocol"
  );
});

test("malformed SSE JSON is a protocol failure", async () => {
  const adapter = adapterForRawSse("data: {not-json}\n\n");
  await assert.rejects(() => collect(adapter), /malformed provider event/i);
});

test("chat completions succeeds only after DONE", async () => {
  const events = await collect(adapterForRawSse(
    'data: {"choices":[{"delta":{"content":"ok"}}]}\n\ndata: [DONE]\n\n'
  ));
  assert.equal(events.at(-1)?.type, "completed");
});
```

- [ ] **Step 2: Run Provider tests and verify RED**

Run: `npx tsx --test test/provider-protocol.test.ts test/provider-model-adapter.test.ts`
Expected: truncated and malformed streams incorrectly complete or are ignored.

- [ ] **Step 3: Make protocol parsing explicit**

```ts
export type ProviderStreamEvent =
  | {type: "ignored"}
  | {type: "delta"; delta: string}
  | {type: "reasoning"; content: string}
  | {type: "usage"; inputTokens?: number; outputTokens?: number; totalTokens?: number}
  | {type: "completed"};

export class ProviderProtocolError extends Error {}
```

`parseEvent("[DONE]")` returns `completed`. Invalid JSON throws `ProviderProtocolError`. Responses `response.completed` returns `completed`; unknown valid objects return `ignored`.

- [ ] **Step 4: Enforce exactly one terminal event in the adapter**

```ts
export type ProviderGatewayEvent =
  | {type: "text.delta"; delta: string}
  | {type: "usage"; inputTokens?: number; outputTokens?: number; totalTokens?: number}
  | {type: "diagnostic"; code: ModelDiagnosticCode; facts: ModelDiagnosticFacts}
  | {type: "completed"};

export interface ProviderGateway {
  stream(request: ProviderRequest): AsyncGenerator<ProviderGatewayEvent>;
}

let completed = false;
for await (const raw of readSseData(response.body)) {
  const event = protocol.parseEvent(raw);
  if (completed && event.type !== "ignored") {
    throw new ProviderProtocolError("Provider emitted data after completion.");
  }
  if (event.type === "completed") {
    completed = true;
    yield {type: "completed"};
    continue;
  }
  // normalize visible events
}
if (!completed) {
  throw new ProviderProtocolError("Provider stream ended before a terminal event.");
}
```

Implement the state machine in `StrictProviderGateway`. Map `ProviderProtocolError` to `ProviderError` with `kind: "protocol"` and `retryable: true`. Do not include raw frames in the user message or trace. Keep `ProviderModelAdapter` as a compatibility wrapper that maps `text.delta` to legacy `delta` events until Task 7 removes the old Runtime connection.

- [ ] **Step 5: Run Provider, Runtime, and full tests**

Run: `npx tsx --test test/provider-protocol.test.ts test/provider-model-adapter.test.ts test/agent-runtime-provider-trace.test.ts`
Expected: all pass.
Run: `npm test && npm run check`
Expected: all tests and TypeScript checking pass.

- [ ] **Step 6: Commit**

```bash
git add src/providers src/runtime/model-adapter.ts test/provider-protocol.test.ts test/provider-model-adapter.test.ts
git commit -m "fix: require provider terminal events"
```

### Task 3: Versioned JSONL Repository, Delta Batching, and Checkpoints

**Files:**
- Create: `src/storage/storage-envelope.ts`
- Create: `src/storage/session-repository.ts`
- Create: `src/storage/turn-delta-buffer.ts`
- Modify: `src/storage/storage-provider.ts`
- Modify: `src/storage/jsonl-storage.ts`
- Modify: `src/utils/jsonl.ts`
- Create: `test/storage-versioning.test.ts`
- Create: `test/turn-delta-buffer.test.ts`
- Modify: `test/storage-turns.test.ts`
- Modify: `test/run-tests.ts`

**Interfaces:**
- Consumes: legacy bare JSONL records and new version-1 envelopes.
- Produces: `StoredEnvelope<T>`, `TurnDeltaBatch`, `checkpointTurns()`, and `TurnDeltaBuffer`.

- [ ] **Step 1: Add failing versioning and checkpoint tests**

```ts
test("new records are versioned and legacy records remain readable", async () => {
  await writeFile(sessionPath, `${JSON.stringify(legacyItem)}\n`, "utf8");
  await storage.appendItem(newItem);
  const raw = await readFile(sessionPath, "utf8");
  assert.match(raw, /"schemaVersion":1/);
  assert.deepEqual(await storage.readThreadItems(thread.id), [legacyItem, newItem]);
});

test("terminal checkpoint keeps one snapshot per Turn", async () => {
  await storage.appendTurn(running);
  await storage.appendTurnDelta({threadId: thread.id, turnId: running.id, delta: "partial ", createdAt: NOW});
  await storage.appendTurnDelta({threadId: thread.id, turnId: running.id, delta: "answer", createdAt: NOW});
  await storage.appendTurn({...running, status: "completed", completedAt: NOW});
  await storage.checkpointTurns(thread.id);
  const rawLines = (await readFile(turnPath, "utf8")).trim().split("\n");
  assert.equal(rawLines.length, 1);
  assert.equal((await storage.readTurns(thread.id))[0]?.assistantDraft, "partial answer");
});
```

- [ ] **Step 2: Add failing delta-buffer tests**

```ts
test("delta buffer flushes at 1024 characters and on completion", async () => {
  const batches: string[] = [];
  const buffer = new TurnDeltaBuffer(async (delta) => { batches.push(delta); }, {delayMs: 250, maxCharacters: 1024});
  await buffer.push("x".repeat(1024));
  await buffer.push("tail");
  await buffer.flush();
  assert.deepEqual(batches, ["x".repeat(1024), "tail"]);
});
```

- [ ] **Step 3: Run storage tests and verify RED**

Run: `npx tsx --test test/storage-versioning.test.ts test/turn-delta-buffer.test.ts test/storage-turns.test.ts`
Expected: missing envelope, buffer, and checkpoint APIs.

- [ ] **Step 4: Implement envelope readers and writers**

```ts
export interface StoredEnvelope<T> {
  schemaVersion: 1;
  sequence: number;
  kind: string;
  payload: T;
  createdAt: string;
}

export function unwrapStoredRecord<T>(value: unknown, kind: string, validate: Validator<T>): T;
export function createStoredEnvelope<T>(kind: string, payload: T, sequence: number, createdAt: string): StoredEnvelope<T>;
```

Add `writeJsonlFile(filePath, values)` to `src/utils/jsonl.ts` using the existing same-directory `atomicReplace`. `JsonlStorageProvider` writes version-1 envelopes, reads bare version-0 records, validates domain payloads, and rejects unknown schema versions or non-monotonic version-1 sequences.

- [ ] **Step 5: Implement manifest and safe migration**

```ts
interface StorageManifest {
  schemaVersion: 1;
  storage: "jsonl";
  createdAt: string;
}
```

On legacy startup, validate affected files, write and re-read migrated temporary replacements, retain `.v0.bak` originals, and write `manifest.json` last. An unknown manifest version throws before mutation.

- [ ] **Step 6: Implement batching and checkpointing**

```ts
export interface TurnDeltaBatch {
  threadId: string;
  turnId: string;
  delta: string;
  createdAt: string;
}

export class TurnDeltaBuffer {
  constructor(
    private readonly persist: (delta: string, createdAt: string) => Promise<void>,
    options?: {delayMs?: number; maxCharacters?: number}
  );
  push(delta: string): Promise<void>;
  flush(): Promise<void>;
  close(): Promise<void>;
}
```

Create `SessionRepository` in `src/storage/session-repository.ts` with the contract from the design, including `appendTurnDelta(batch: TurnDeltaBatch)` and `checkpointTurns(threadId)`. `JsonlStorageProvider` implements it and exports a temporary `JsonlSessionRepository` alias. `TurnEngine` constructs `TurnDeltaBuffer` with a closure that adds `threadId` and `turnId` before calling the repository. `checkpointTurns(threadId)` replays current Turn state and atomically rewrites one snapshot envelope per Turn, retaining `assistantDraft`.

- [ ] **Step 7: Run focused and full verification**

Run: `npx tsx --test test/storage-versioning.test.ts test/turn-delta-buffer.test.ts test/storage-turns.test.ts test/file-reliability.test.ts`
Expected: all pass.
Run: `npm test && npm run check`
Expected: all pass.

- [ ] **Step 8: Commit**

```bash
git add src/storage src/utils/jsonl.ts test/storage-versioning.test.ts test/turn-delta-buffer.test.ts test/storage-turns.test.ts test/run-tests.ts
git commit -m "feat: version and checkpoint runtime storage"
```

### Task 4: Canonical Configuration and Consent-based Credentials

**Files:**
- Create: `src/config/credential-store.ts`
- Create: `src/runtime/provider-service.ts`
- Modify: `src/config/config-manager.ts`
- Modify: `src/config/secret-store.ts`
- Modify: `src/config/provider-config.ts`
- Modify: `src/types.ts`
- Modify: `src/protocol.ts`
- Modify: `package.json`
- Modify: `package-lock.json`
- Create: `test/credential-consent.test.ts`
- Modify: `test/config-manager.test.ts`
- Modify: `test/secret-store.test.ts`
- Modify: `test/run-tests.ts`

**Interfaces:**
- Consumes: legacy `api`, JSONL-only storage configuration, environment variables, optional OS keyring, and explicit plaintext consent.
- Produces: canonical `AppConfig`, `CredentialStore`, `PlaintextConsent`, and `ProviderService`.

- [ ] **Step 1: Write failing configuration tests**

```ts
test("legacy api configuration migrates to one provider source", async () => {
  await writeFile(configPath, JSON.stringify({api: {provider: "hashsight", protocol: "chat_completions", baseUrl: "https://example.test/v1"}}));
  const config = await manager.load();
  assert.equal(config.modelProvider, "hashsight");
  assert.equal("api" in config, false);
});

test("sqlite is rejected during configuration migration", async () => {
  await writeFile(configPath, JSON.stringify({storage: {driver: "sqlite"}}));
  await assert.rejects(() => manager.load(), /SQLite is not supported.*jsonl/i);
});
```

- [ ] **Step 2: Write failing credential-consent tests**

```ts
test("plaintext storage requires a single-use consent token", async () => {
  const store = new CredentialStore({keyring: null, userConfigDir});
  await assert.rejects(() => store.saveToUserFile("minimax-official", "secret", undefined), /consent/i);
  const consent = store.createPlaintextConsent();
  await store.saveToUserFile("minimax-official", "secret", consent);
  await assert.rejects(() => store.saveToUserFile("hashsight", "another", consent), /already used/i);
});

test("environment credentials win over every persisted backend", async () => {
  const store = new CredentialStore({keyring, userConfigDir, env: {MINIMAX_API_KEY: "environment"}});
  assert.equal(await store.get("minimax-official", "MINIMAX_API_KEY"), "environment");
});
```

- [ ] **Step 3: Run focused tests and verify RED**

Run: `npx tsx --test test/config-manager.test.ts test/credential-consent.test.ts test/secret-store.test.ts`
Expected: canonical config and consent APIs are missing.

- [ ] **Step 4: Canonicalize AppConfig**

```ts
export interface AppConfig {
  schemaVersion: 1;
  modelProvider: ApiProviderId;
  modelProviders: Record<ApiProviderId, ModelProviderConfig>;
  model: string;
  context: ContextConfig;
}
```

Remove `StorageDriver`, `storage.driver`, and the `api` object from the canonical type. Parse legacy values in `ConfigManager`, reject legacy SQLite clearly, merge built-in/custom providers, validate the result, and save only canonical fields.

- [ ] **Step 5: Add the maintained optional keyring backend**

```json
{
  "optionalDependencies": {
    "@napi-rs/keyring": "^1.3.0"
  }
}
```

Implement an adapter around `new Entry(service, account)`, `getPassword()`, and `setPassword(value)`. Native import/load failure returns backend status `unavailable`; it never triggers a plaintext write.

- [ ] **Step 6: Implement single-use consent and ProviderService**

```ts
export class PlaintextConsent {
  #used = false;
  consume(): void {
    if (this.#used) throw new Error("Plaintext credential consent was already used.");
    this.#used = true;
  }
}

export class ProviderService {
  init(): Promise<void>;
  inspectCredential(): Promise<CredentialStatus>;
  getApiKey(): Promise<string | null>;
  saveApiKey(value: string, consent?: PlaintextConsent): Promise<CredentialBackend>;
  list(): string[];
  switch(providerId: string): Promise<string>;
  get config(): AppConfig;
}
```

Add protocol events for `config.api_key.plaintext_confirmation_required` and `config.api_key.plaintext_confirmed`. The key-entry mode begins only after confirmation when keyring storage is unavailable.

- [ ] **Step 7: Run focused and full verification**

Run: `npm install`
Expected: lockfile contains `@napi-rs/keyring` as optional.
Run: `npx tsx --test test/config-manager.test.ts test/credential-consent.test.ts test/secret-store.test.ts`
Expected: all pass.
Run: `npm test && npm run check`
Expected: all pass.

- [ ] **Step 8: Commit**

```bash
git add package.json package-lock.json src/config src/runtime/provider-service.ts src/types.ts src/protocol.ts test
git commit -m "feat: canonicalize providers and credential consent"
```

### Task 5: Structured Context Engine and Replaceable Token Estimation

**Files:**
- Create: `src/runtime/token-estimator.ts`
- Create: `src/runtime/context-engine.ts`
- Modify: `src/runtime/context-manager.ts`
- Modify: `src/runtime/summary-generator.ts`
- Modify: `test/context-manager.test.ts`
- Modify: `test/summary-generator.test.ts`
- Create: `test/token-estimator.test.ts`
- Modify: `test/run-tests.ts`

**Interfaces:**
- Consumes: canonical AppConfig, visible ThreadItems, ContextSummaries, and current user input.
- Produces: `ContextEngine.build()`, `ContextEngine.compactionBoundary()`, `StructuredLocalSummaryGenerator`, and `TokenEstimator`.

- [ ] **Step 1: Write failing estimator tests**

```ts
test("Chinese and emoji are not divided by four", () => {
  const estimator = new ConservativeTokenEstimator();
  assert.equal(estimator.estimateText("你好世界"), 5); // four CJK tokens plus safety margin
  assert.equal(estimator.estimateText("😀😀"), 3);
});

test("code receives a tighter estimate than plain Latin prose", () => {
  const estimator = new ConservativeTokenEstimator();
  assert.ok(estimator.estimateText("const value = foo.bar(baz);") > estimator.estimateText("a calm ordinary sentence here"));
});
```

- [ ] **Step 2: Write failing structured-summary tests**

```ts
test("summary preserves original goal, constraints, decisions, open items, and recent exchanges", async () => {
  const content = await generator.generate(items, "manual");
  assert.match(content, /Original goal:/);
  assert.match(content, /Constraints:/);
  assert.match(content, /Decisions:/);
  assert.match(content, /Open items:/);
  assert.match(content, /Recent exchanges:/);
  assert.ok(content.length <= 4096);
});
```

- [ ] **Step 3: Run focused tests and verify RED**

Run: `npx tsx --test test/token-estimator.test.ts test/summary-generator.test.ts test/context-manager.test.ts`
Expected: missing estimator/engine APIs and old summary layout.

- [ ] **Step 4: Implement TokenEstimator**

```ts
export interface TokenEstimator {
  estimateText(text: string): number;
  estimateMessages(messages: ModelContextMessage[]): number;
}

export class ConservativeTokenEstimator implements TokenEstimator {
  estimateText(text: string): number;
  estimateMessages(messages: ModelContextMessage[]): number;
}
```

Count CJK and emoji code points individually, estimate Latin prose at four characters per token, estimate code-heavy text at three characters per token, add four tokens per message, then apply `Math.ceil(raw * 1.15)`.

- [ ] **Step 5: Implement structured bounded summaries**

Build named sections from model-visible completed items. Preserve the earliest user goal, requirement/prohibition statements, explicit choice/decision statements, unresolved user/error signals, and the latest three completed exchanges. Cap each entry at 480 characters and the complete summary at 4,096 characters. Exclude trace, error payload bodies, secrets, raw reasoning, and partial assistant replies.

- [ ] **Step 6: Implement ContextEngine and compatibility export**

```ts
export class ContextEngine {
  constructor(private readonly estimator: TokenEstimator = new ConservativeTokenEstimator()) {}
  build(params: BuildContextParams): BuiltContext;
  compactionBoundary(items: ThreadItem[], preserveTurnId?: string): number;
  createSummary(threadId: string, content: string, coveredThroughItemId: string): ContextSummary;
}

export {ContextEngine as ContextManager};
```

Use `estimateMessages`, reserve `maxCompletionTokens`, and keep the existing summary coverage boundary behavior.

- [ ] **Step 7: Run focused and full verification**

Run: `npx tsx --test test/token-estimator.test.ts test/summary-generator.test.ts test/context-manager.test.ts test/agent-runtime-compaction.test.ts`
Expected: all pass.
Run: `npm test && npm run check`
Expected: all pass.

- [ ] **Step 8: Commit**

```bash
git add src/runtime/token-estimator.ts src/runtime/context-engine.ts src/runtime/context-manager.ts src/runtime/summary-generator.ts test
git commit -m "refactor: introduce structured context engine"
```

### Task 6: SessionService and TurnEngine Rewrite

**Files:**
- Create: `src/runtime/session-service.ts`
- Create: `src/runtime/turn-engine.ts`
- Modify: `src/runtime/agent-runtime.ts`
- Modify: `src/storage/storage-provider.ts`
- Create: `test/session-service.test.ts`
- Create: `test/turn-engine.test.ts`
- Modify: `test/run-tests.ts`

**Interfaces:**
- Consumes: `SessionRepository`, `ProviderService`, `ProviderGateway`, `ContextEngine`, `SummaryGenerator`, and `SafeTraceRecorder`.
- Produces: thread/session lifecycle from `SessionService` and Turn streaming from `TurnEngine`.

- [ ] **Step 1: Write failing SessionService tests**

```ts
test("session service restores one active thread and recovers running Turns", async () => {
  const events = await service.init();
  assert.equal(events[0]?.type, "thread.loaded");
  assert.equal(events.some((event) => event.type === "turn.recovered"), true);
});

test("session service creates and resumes threads without Provider dependencies", async () => {
  const created = await service.newThread("MiniMax-M3", cwd);
  const resumed = await service.resumeThread(previous.id);
  assert.equal(created.thread.id === resumed.thread.id, false);
});
```

- [ ] **Step 2: Write failing TurnEngine tests**

```ts
test("TurnEngine flushes and checkpoints after formal Provider completion", async () => {
  const events = await collect(engine.submit("hello"));
  assert.equal(events.some((event) => event.type === "assistant.completed"), true);
  assert.equal(repository.flushCount, 1);
  assert.equal(repository.checkpointCount, 1);
});

test("TurnEngine persists premature EOF as failed partial output", async () => {
  await collect(engine.submit("hello"));
  assert.equal(repository.latestTurn.status, "failed");
  assert.equal(repository.latestAssistant.metadata?.partial, true);
});
```

- [ ] **Step 3: Run focused tests and verify RED**

Run: `npx tsx --test test/session-service.test.ts test/turn-engine.test.ts`
Expected: missing service and engine modules.

- [ ] **Step 4: Move thread lifecycle into SessionService**

```ts
export class SessionService {
  init(model: string, cwd: string): Promise<RuntimeEvent[]>;
  get activeThread(): ThreadRecord;
  newThread(model: string, cwd: string): Promise<{thread: ThreadRecord; events: RuntimeEvent[]}>;
  listThreads(): Promise<ThreadRecord[]>;
  resumeThread(threadId: string): Promise<{thread: ThreadRecord; events: RuntimeEvent[]}>;
  createTurn(input: string): Promise<TurnRecord>;
  completeTurn(turn: TurnRecord, status: TurnStatus): Promise<void>;
  createItem(params: CreateItemParams): ThreadItem;
}
```

Move ensure-thread, activation, stale-Turn recovery, Turn snapshots, item creation, and thread timestamps from `AgentRuntime` without importing Provider modules.

- [ ] **Step 5: Move Turn execution into TurnEngine**

```ts
export class TurnEngine {
  submit(input: string): AsyncGenerator<RuntimeEvent>;
  interrupt(): RuntimeEvent;
  compact(reason: CompactReason, options?: CompactOptions): Promise<RuntimeEvent[]>;
  shutdown(): Promise<void>;
}
```

`TurnEngine` owns active AbortController, ContextEngine, summary generation, `TurnDeltaBuffer`, Provider Gateway consumption, safe trace, terminal persistence, forced flush, and checkpoints. It does not load configuration files or select threads.

- [ ] **Step 6: Reduce AgentRuntime to a temporary facade**

```ts
export class AgentRuntime {
  constructor(private readonly kernel = createLegacyCompatibleKernel()) {}
  init(): Promise<RuntimeEvent[]> { return this.kernel.init(); }
  // public legacy methods delegate without workflow logic
}
```

Keep old public methods only until Task 7 switches `CommandDispatcher` to `ApplicationKernel`.

- [ ] **Step 7: Run Runtime and full verification**

Run: `npx tsx --test test/session-service.test.ts test/turn-engine.test.ts test/agent-runtime-compaction.test.ts test/agent-runtime-recovery.test.ts test/agent-runtime-interrupt.test.ts test/thread-navigation.test.ts`
Expected: all pass.
Run: `npm test && npm run check`
Expected: all pass.

- [ ] **Step 8: Commit**

```bash
git add src/runtime/session-service.ts src/runtime/turn-engine.ts src/runtime/agent-runtime.ts src/storage/storage-provider.ts test
git commit -m "refactor: split session and turn runtime services"
```

### Task 7: ApplicationKernel and End-to-End Connection Contract

**Files:**
- Create: `src/runtime/runtime-application.ts`
- Create: `src/runtime/application-kernel.ts`
- Modify: `src/runtime/command-dispatcher.ts`
- Modify: `src/runtime/agent-runtime.ts`
- Modify: `src/protocol.ts`
- Create: `test/application-kernel.test.ts`
- Create: `test/runtime-connection-contract.test.ts`
- Modify: `test/command-dispatcher.test.ts`
- Modify: `test/run-tests.ts`

**Interfaces:**
- Consumes: all services built in Tasks 1-6.
- Produces: the single `RuntimeApplication` connection consumed by `CommandDispatcher` and Ink.

- [ ] **Step 1: Write failing kernel lifecycle tests**

```ts
test("kernel acquires the lease before initializing services and releases it on shutdown", async () => {
  const events = await kernel.init();
  assert.equal(events.at(-1)?.type, "runtime.ready");
  assert.deepEqual(calls.slice(0, 3), ["lease.acquire", "provider.init", "session.init"]);
  await kernel.shutdown("user");
  assert.deepEqual(calls.slice(-2), ["turn.shutdown", "lease.release"]);
});

test("kernel rejects a second mutating command while a Turn runs", async () => {
  const running = collect(kernel.dispatch({type: "turn.submit", input: "hello"}));
  await turnStarted;
  const rejected = await collect(kernel.dispatch({type: "thread.new"}));
  assert.equal(rejected[0]?.type, "command.rejected");
  await collect(kernel.dispatch({type: "turn.interrupt"}));
  await running;
});
```

- [ ] **Step 2: Write failing full connection contract**

```ts
test("Command connects through the new kernel to Provider, storage, and RuntimeEvents", async () => {
  const app = createTestKernel({providerEvents: [
    {type: "text.delta", delta: "connected"},
    {type: "completed"}
  ]});
  await app.init();
  const events = await collect(app.dispatch({type: "turn.submit", input: "hello"}));
  assert.equal(events.some((event) => event.type === "assistant.delta"), true);
  assert.equal(events.some((event) => event.type === "assistant.completed"), true);
  assert.equal((await repository.readThread(activeId)).items.at(-1)?.content, "connected");
});
```

- [ ] **Step 3: Run kernel tests and verify RED**

Run: `npx tsx --test test/application-kernel.test.ts test/runtime-connection-contract.test.ts`
Expected: missing `ApplicationKernel` and `RuntimeApplication`.

- [ ] **Step 4: Implement the RuntimeApplication contract**

```ts
export interface RuntimeApplication {
  init(): Promise<RuntimeEvent[]>;
  dispatch(command: Command): AsyncGenerator<RuntimeEvent>;
  shutdown(reason: "user" | "signal" | "fatal"): Promise<void>;
}
```

- [ ] **Step 5: Implement ApplicationKernel composition and routing**

```ts
export class ApplicationKernel implements RuntimeApplication {
  async init(): Promise<RuntimeEvent[]>;
  async *dispatch(command: Command): AsyncGenerator<RuntimeEvent>;
  async shutdown(reason: ShutdownReason): Promise<void>;
}
```

Acquire the lease first. Initialize ProviderService and SessionService. Mark the arbiter ready only after both succeed. Route Commands to focused services, use arbiter ownership around mutations, allow interrupt/shutdown bypasses, and redact credential-command failures.

- [ ] **Step 6: Switch CommandDispatcher and remove workflow logic from AgentRuntime**

`CommandDispatcher` constructor accepts `RuntimeApplication = new ApplicationKernel()`. `AgentRuntime` becomes a deprecated re-export/facade with delegation only; no configuration, storage, context, Provider, or Turn implementation remains in that file.

- [ ] **Step 7: Run connection and full verification**

Run: `npx tsx --test test/application-kernel.test.ts test/runtime-connection-contract.test.ts test/command-dispatcher.test.ts`
Expected: all pass.
Run: `npm test && npm run check && npm run build`
Expected: all pass.

- [ ] **Step 8: Commit**

```bash
git add src/runtime/runtime-application.ts src/runtime/application-kernel.ts src/runtime/command-dispatcher.ts src/runtime/agent-runtime.ts src/protocol.ts test
git commit -m "refactor: connect cli through application kernel"
```

### Task 8: Pure UI Reducer, Plaintext Confirmation Flow, and Cleanup

**Files:**
- Create: `src/ui/ui-state.ts`
- Modify: `src/ui/App.tsx`
- Modify: `src/ui/chat-input-policy.ts`
- Modify: `src/ui/format-runtime-event.ts`
- Delete: `src/storage/sqlite-storage.ts`
- Modify: `src/types.ts`
- Create: `test/ui-state.test.ts`
- Modify: `test/ui-command-boundary.test.ts`
- Modify: `test/chat-input-policy.test.ts`
- Modify: `test/run-tests.ts`

**Interfaces:**
- Consumes: stable RuntimeEvents including busy rejection and plaintext confirmation.
- Produces: pure `reduceRuntimeEvent`, UI-only rendering, and confirmed `/api` fallback flow.

- [ ] **Step 1: Write failing reducer tests**

```ts
test("runtime.ready is the only event that leaves booting", () => {
  const failed = reduceRuntimeEvent(initialUiState(), {type: "error", message: "init failed"});
  assert.equal(failed.phase, "booting");
  const ready = reduceRuntimeEvent(initialUiState(), readyEvent);
  assert.equal(ready.phase, "idle");
});

test("plaintext warning requires confirmation before API input", () => {
  const warning = reduceRuntimeEvent(initialUiState(), {
    type: "config.api_key.plaintext_confirmation_required",
    path: "C:/Users/test/credentials.json"
  });
  assert.equal(warning.phase, "confirming_plaintext");
  assert.match(warning.status, /plaintext|明文/i);
});
```

- [ ] **Step 2: Write failing static UI boundary tests**

```ts
test("App contains no RuntimeEvent branch table or command concurrency policy", async () => {
  const source = await readFile(appPath, "utf8");
  assert.equal(source.includes("function applyRuntimeEvent"), false);
  assert.equal(source.includes("function isBlockingCommand"), false);
  assert.equal(source.includes("AgentRuntime"), false);
});
```

- [ ] **Step 3: Run UI tests and verify RED**

Run: `npx tsx --test test/ui-state.test.ts test/ui-command-boundary.test.ts test/chat-input-policy.test.ts`
Expected: reducer module is missing and App still owns event/concurrency branching.

- [ ] **Step 4: Implement pure UI state reduction**

```ts
export type UiPhase = "booting" | "idle" | "running" | "confirming_plaintext" | "entering_api_key" | "stopped";

export interface UiState {
  phase: UiPhase;
  messages: DisplayMessage[];
  traces: TraceEvent[];
  status: string;
  tokenLine: string;
  traceOpen: boolean;
}

export function initialUiState(): UiState;
export function reduceRuntimeEvent(state: UiState, event: RuntimeEvent): UiState;
```

Use an exhaustive switch with a `never` check. Preserve stable `user-<turnId>` and `assistant-<turnId>` identities and the existing visible copy.

- [ ] **Step 5: Simplify App and implement confirmation input**

Use one `useReducer(reduceRuntimeEvent, initialUiState())`. `App.tsx` dispatches Commands and renders UiState. In `confirming_plaintext`, accept `YES` to send `config.api_key.plaintext.confirm`; any other input cancels and returns to idle. Core rejection events, not UI command lists, control busy behavior.

- [ ] **Step 6: Remove dead architecture**

Delete SQLite storage, remove `StorageDriver`, unused item kinds, unused Provider capability flags, and compatibility code that no longer has a consumer. Keep only the deprecated `AgentRuntime` export if an external package entry still requires it; otherwise remove it after the static import scan is clean.

- [ ] **Step 7: Run UI and full verification**

Run: `npx tsx --test test/ui-state.test.ts test/ui-command-boundary.test.ts test/chat-input-policy.test.ts test/ui-status.test.ts`
Expected: all pass.
Run: `npm test && npm run check && npm run build`
Expected: all pass.

- [ ] **Step 8: Commit**

```bash
git add src/ui src/types.ts src/storage test
git commit -m "refactor: reduce runtime events outside ink"
```

### Task 9: Migration Fixtures, Optional Live Smoke Command, and Final Verification

**Files:**
- Create: `test/fixtures/legacy-v0/config.json`
- Create: `test/fixtures/legacy-v0/indexes/threads.json`
- Create: `test/fixtures/legacy-v0/sessions/2026/07/thread_legacy.jsonl`
- Create: `test/fixtures/legacy-v0/turns/thread_legacy.turns.jsonl`
- Create: `test/legacy-workspace-migration.test.ts`
- Create: `src/smoke/provider-smoke.ts`
- Modify: `package.json`
- Modify: `README.md`
- Modify: `test/run-tests.ts`

**Interfaces:**
- Consumes: a byte-stable version-0 workspace fixture and real Provider environment variables only when explicitly invoked.
- Produces: migration proof, offline end-to-end verification, and `npm run smoke:provider`.

- [ ] **Step 1: Add the legacy fixture and failing migration test**

```ts
test("a version-0 workspace migrates and remains fully usable", async () => {
  await copyFixture("legacy-v0", root);
  const app = createKernelForRoot(root, fakeProvider);
  const initEvents = await app.init();
  assert.equal(initEvents.some((event) => event.type === "history.loaded"), true);
  assert.equal((await readJson(join(root, "manifest.json"))).schemaVersion, 1);
  assert.equal(await exists(join(root, "config.json.v0.bak")), true);
  const turnEvents = await collect(app.dispatch({type: "turn.submit", input: "after migration"}));
  assert.equal(turnEvents.some((event) => event.type === "assistant.completed"), true);
});
```

- [ ] **Step 2: Run the migration test and verify RED**

Run: `npx tsx --test test/legacy-workspace-migration.test.ts`
Expected: at least one fixture migration, backup, or post-migration connection assertion fails.

- [ ] **Step 3: Complete migration compatibility gaps**

Adjust only the migration adapters needed for the fixture to pass. Do not weaken validation. Preserve original fixture bytes in `.v0.bak` files and verify the new Runtime can resume and append after migration.

- [ ] **Step 4: Add an opt-in Provider smoke entrypoint**

```ts
const app = new ApplicationKernel(process.cwd());
const init = await app.init();
if (!init.some((event) => event.type === "runtime.ready")) process.exitCode = 1;
for await (const event of app.dispatch({type: "turn.submit", input: "Reply with exactly: connected"})) {
  if (event.type === "assistant.completed" && event.item.content.trim()) process.stdout.write("provider connection passed\n");
  if (event.type === "error") throw new Error(event.message);
}
await app.shutdown("user");
```

Add `"smoke:provider": "tsx src/smoke/provider-smoke.ts"`. The script reads credentials through `CredentialStore`, never accepts a key argument, never prints prompts/keys/raw frames, and is not called by `test`, `check`, or `build`.

- [ ] **Step 5: Update README**

Document the rewritten architecture, one-process lease, stale recovery, strict terminal events, versioned migration/backups, plaintext confirmation flow, JSONL-only storage, structured summaries, and the explicit authorization requirement before `npm run smoke:provider`.

- [ ] **Step 6: Run complete offline verification**

Run: `npm test`
Expected: all legacy and rewrite tests pass.
Run: `npm run check`
Expected: no TypeScript errors.
Run: `npm run build`
Expected: production build succeeds.
Run: `git diff --check`
Expected: no whitespace errors.
Run: `git status --short`
Expected: only Task 9 files are pending.

- [ ] **Step 7: Do not run the live smoke test automatically**

Record `npm run smoke:provider` as pending explicit user approval. Offline fake-Provider connection tests are the completion gate for this task.

- [ ] **Step 8: Commit**

```bash
git add test/fixtures test/legacy-workspace-migration.test.ts test/run-tests.ts src/smoke/provider-smoke.ts package.json README.md
git commit -m "test: verify runtime rewrite connectivity"
```

## Final Plan Self-review

- Every design goal is covered by one or more tasks.
- No task adds the excluded model-tool-result loop.
- The stable connection path is tested before and after switching to `ApplicationKernel`.
- Lease, Provider terminal state, migration, batching, credential consent, context estimation, and UI reduction each have a failing-test-first step.
- Interface names are consistent across producer and consumer tasks.
- Every task ends with focused verification, full regression verification, and an atomic commit.
- The only real Provider command is opt-in and is explicitly excluded from automated verification.
