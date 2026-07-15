import assert from "node:assert/strict";
import {mkdtemp, rm} from "node:fs/promises";
import {tmpdir} from "node:os";
import {join} from "node:path";
import test from "node:test";
import {ConfigManager, DEFAULT_CONFIG} from "../src/config/config-manager.js";
import {CredentialStore} from "../src/config/credential-store.js";
import {createCredentialTarget} from "../src/config/provider-security.js";
import {getActiveProvider} from "../src/config/provider-config.js";
import type {
  ProviderGateway,
  ProviderGatewayEvent,
  ProviderRequest
} from "../src/providers/provider-gateway.js";
import {ProviderService} from "../src/runtime/provider-service.js";
import type {ModelAdapterEvent} from "../src/runtime/model-adapter.js";
import type {
  ModelRuntime,
  ModelRuntimeRequest
} from "../src/runtime/model-runtime.js";
import type {
  ActiveModelSelection,
  ModelRuntimeSnapshotPort
} from "../src/runtime/model-selection-service.js";
import {SessionService} from "../src/runtime/session-service.js";
import {TurnEngine} from "../src/runtime/turn-engine.js";
import {JsonlSessionRepository} from "../src/storage/jsonl-storage.js";
import type {
  RepositoryInitResult,
  SessionRepository,
  ThreadSnapshot,
  TurnDeltaBatch
} from "../src/storage/session-repository.js";
import type {
  ContextSummary,
  ThreadItem,
  ThreadRecord,
  TraceEvent,
  TurnRecord
} from "../src/types.js";
import type {RuntimeEvent} from "../src/protocol.js";

class RecordingRepository implements SessionRepository {
  deltaBatchCount = 0;
  checkpointCount = 0;

  constructor(
    private readonly inner: SessionRepository,
    private readonly failures: {
      appendDelta?: string;
      checkpoint?: string;
    } = {}
  ) {}

  init(): Promise<RepositoryInitResult> {
    return this.inner.init();
  }

  createThread(thread: ThreadRecord): Promise<void> {
    return this.inner.createThread(thread);
  }

  updateThread(thread: ThreadRecord): Promise<void> {
    return this.inner.updateThread(thread);
  }

  activateThread(threadId: string, activatedAt: string): Promise<ThreadRecord | null> {
    return this.inner.activateThread(threadId, activatedAt);
  }

  appendTurnSnapshot(turn: TurnRecord): Promise<void> {
    return this.inner.appendTurnSnapshot(turn);
  }

  async appendTurnDelta(batch: TurnDeltaBatch): Promise<void> {
    this.deltaBatchCount += 1;
    if (this.failures.appendDelta) {
      throw new Error(this.failures.appendDelta);
    }
    await this.inner.appendTurnDelta(batch);
  }

  async checkpointTurns(threadId: string): Promise<void> {
    this.checkpointCount += 1;
    if (this.failures.checkpoint) {
      throw new Error(this.failures.checkpoint);
    }
    await this.inner.checkpointTurns(threadId);
  }

  appendItem(item: ThreadItem): Promise<void> {
    return this.inner.appendItem(item);
  }

  appendTrace(event: TraceEvent): Promise<void> {
    return this.inner.appendTrace(event);
  }

  appendSummary(summary: ContextSummary): Promise<void> {
    return this.inner.appendSummary(summary);
  }

  readThread(threadId: string): Promise<ThreadSnapshot> {
    return this.inner.readThread(threadId);
  }

  listThreads(): Promise<ThreadRecord[]> {
    return this.inner.listThreads();
  }
}

class CompletedGateway implements ProviderGateway {
  async *stream(_request: ProviderRequest): AsyncGenerator<ProviderGatewayEvent> {
    yield {type: "text.delta", delta: "formal completion"};
    yield {type: "completed"};
  }
}

class PrematureEofGateway implements ProviderGateway {
  async *stream(_request: ProviderRequest): AsyncGenerator<ProviderGatewayEvent> {
    yield {type: "text.delta", delta: "truncated partial"};
  }
}

class BlockingGateway implements ProviderGateway {
  private resolveStarted!: () => void;
  readonly started = new Promise<void>((resolve) => {
    this.resolveStarted = resolve;
  });

  async *stream(request: ProviderRequest): AsyncGenerator<ProviderGatewayEvent> {
    yield {type: "text.delta", delta: "partial before shutdown"};
    this.resolveStarted();
    await new Promise<never>((_resolve, reject) => {
      const rejectAborted = (): void => {
        const error = new Error("request aborted");
        error.name = "AbortError";
        reject(error);
      };
      if (request.signal?.aborted) {
        rejectAborted();
      } else {
        request.signal?.addEventListener("abort", rejectAborted, {once: true});
      }
    });
  }
}

interface Harness {
  cwd: string;
  repository: RecordingRepository;
  session: SessionService;
  providerService: ProviderService;
}

async function createHarness(
  failures: {appendDelta?: string; checkpoint?: string} = {}
): Promise<Harness> {
  const cwd = await mkdtemp(join(tmpdir(), "minimax-turn-engine-"));
  const stateRoot = join(cwd, ".mini-codex");
  const configManager = new ConfigManager(stateRoot);
  const credentialStore = new CredentialStore({
    userConfigDir: join(stateRoot, "user-config"),
    keyring: null,
    env: {}
  });
  await configManager.save(DEFAULT_CONFIG);
  await credentialStore.saveToUserFile(
    createCredentialTarget(
      "minimax-official",
      DEFAULT_CONFIG.modelProviders["minimax-official"]!
    ),
    "fake-test-key",
    credentialStore.createPlaintextConsent()
  );
  const providerService = new ProviderService(configManager, credentialStore);
  await providerService.init();
  const repository = new RecordingRepository(
    new JsonlSessionRepository(stateRoot),
    failures
  );
  const session = new SessionService(repository);
  await session.init(DEFAULT_CONFIG.model, cwd);
  return {cwd, repository, session, providerService};
}

function createEngine(harness: Harness, gateway: ProviderGateway): TurnEngine {
  return new TurnEngine({
    sessionService: harness.session,
    modelRuntime: gatewayRuntime(harness.providerService, gateway),
    repository: harness.repository,
    deltaBufferOptions: {delayMs: 60_000}
  });
}

function gatewayRuntime(
  providerService: ProviderService,
  gateway: ProviderGateway
): ModelRuntimeSnapshotPort {
  const config = providerService.config;
  const provider = getActiveProvider(config);
  const selection: ActiveModelSelection = Object.freeze({
    adapterId: "adapter:minimax/builtin" as never,
    providerProfileId: "provider:minimax/official" as never,
    modelProfileId: "model:minimax/official/MiniMax-M3" as never,
    providerDisplayName: provider.name,
    modelDisplayName: config.model,
    model: config.model,
    protocol: provider.protocol,
    source: "builtin",
    contextWindow: config.context.workingContextLimit,
    maxOutputTokens: config.context.maxCompletionTokens,
    autoCompactRatio: config.context.autoCompactRatio,
    supportsNativeToolCalls: false
  });
  const runtime: ModelRuntime = {
    adapterId: selection.adapterId,
    providerProfileId: selection.providerProfileId,
    modelProfileId: selection.modelProfileId,
    async *stream(request: ModelRuntimeRequest): AsyncGenerator<ModelAdapterEvent> {
      const apiKey = await providerService.getApiKey();
      if (!apiKey) {
        throw new Error("fixture credential is unavailable");
      }
      for await (const event of gateway.stream({
        config,
        apiKey,
        messages: [...request.messages],
        ...(request.signal ? {signal: request.signal} : {})
      })) {
        if (event.type === "text.delta") {
          yield {type: "delta", delta: event.delta};
        } else {
          yield event;
        }
      }
    },
    async dispose(): Promise<void> {}
  };
  return {
    getRuntimeSnapshot: () => ({selection, runtime}),
    assertAgentCompatible: () => {
      throw new Error("fixture is chat-only");
    }
  };
}

async function collect(stream: AsyncGenerator<RuntimeEvent>): Promise<RuntimeEvent[]> {
  const events: RuntimeEvent[] = [];
  for await (const event of stream) {
    events.push(event);
  }
  return events;
}

async function pullThrough(
  stream: AsyncGenerator<RuntimeEvent>,
  type: RuntimeEvent["type"]
): Promise<void> {
  while (true) {
    const next = await stream.next();
    if (next.done) {
      throw new Error(`Turn stream ended before ${type}.`);
    }
    if (next.value.type === type) {
      return;
    }
  }
}

async function settlesWithin(
  promise: Promise<void>,
  milliseconds: number
): Promise<{settled: boolean; error?: unknown}> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const result = await Promise.race([
    promise.then(
      () => ({settled: true}),
      (error: unknown) => ({settled: true, error})
    ),
    new Promise<{settled: false}>((resolve) => {
      timer = setTimeout(() => resolve({settled: false}), milliseconds);
    })
  ]);
  if (timer) {
    clearTimeout(timer);
  }
  return result;
}

test("TurnEngine flushes and checkpoints after formal Provider completion", async () => {
  const harness = await createHarness();

  try {
    const engine = createEngine(harness, new CompletedGateway());
    const events = await collect(engine.submit("hello"));
    const snapshot = await harness.repository.readThread(harness.session.activeThread.id);

    assert.equal(events.some((event) => event.type === "assistant.completed"), true);
    assert.equal(harness.repository.deltaBatchCount, 1);
    assert.equal(harness.repository.checkpointCount, 1);
    assert.equal(snapshot.turns.at(-1)?.status, "completed");
    assert.equal(
      snapshot.items.some(
        (item) => item.type === "assistant_message" && item.content === "formal completion"
      ),
      true
    );
  } finally {
    await rm(harness.cwd, {recursive: true, force: true});
  }
});

test("TurnEngine persists premature EOF as failed partial output", async () => {
  const harness = await createHarness();

  try {
    const engine = createEngine(harness, new PrematureEofGateway());
    const events = await collect(engine.submit("hello"));
    const snapshot = await harness.repository.readThread(harness.session.activeThread.id);
    const latestAssistant = snapshot.items.filter(
      (item) => item.type === "assistant_message"
    ).at(-1);

    assert.equal(events.some((event) => event.type === "assistant.completed"), false);
    assert.equal(events.some((event) => event.type === "error"), true);
    assert.equal(snapshot.turns.at(-1)?.status, "failed");
    assert.equal(latestAssistant?.metadata?.partial, true);
    assert.equal(latestAssistant?.metadata?.failed, true);
    assert.equal(harness.repository.deltaBatchCount, 1);
    assert.equal(harness.repository.checkpointCount, 1);
  } finally {
    await rm(harness.cwd, {recursive: true, force: true});
  }
});

test("TurnEngine shutdown interrupts, flushes, and checkpoints an active Turn", async () => {
  const harness = await createHarness();

  try {
    const gateway = new BlockingGateway();
    const engine = createEngine(harness, gateway);
    const collected = collect(engine.submit("stop during shutdown"));
    await gateway.started;

    await engine.shutdown();
    const events = await collected;
    const snapshot = await harness.repository.readThread(harness.session.activeThread.id);

    assert.equal(events.some((event) => event.type === "turn.interrupted"), true);
    assert.equal(snapshot.turns.at(-1)?.status, "interrupted");
    assert.equal(harness.repository.deltaBatchCount, 1);
    assert.equal(harness.repository.checkpointCount, 1);
  } finally {
    await rm(harness.cwd, {recursive: true, force: true});
  }
});

test("TurnEngine interrupt is harmless with no active Turn", async () => {
  const harness = await createHarness();

  try {
    const engine = createEngine(harness, new CompletedGateway());
    assert.deepEqual(engine.interrupt(), {
      type: "turn.interrupt.ignored",
      reason: "no_active_request"
    });
  } finally {
    await rm(harness.cwd, {recursive: true, force: true});
  }
});

test("TurnEngine exposes whether a Turn currently owns lifecycle state", async () => {
  const harness = await createHarness();

  try {
    const engine = createEngine(harness, new BlockingGateway());
    const stream = engine.submit("inspect active state");
    const before = engine.hasActiveTurn;
    await stream.next();
    const during = engine.hasActiveTurn;
    engine.interrupt();
    await collect(stream);
    const after = engine.hasActiveTurn;

    assert.deepEqual([before, during, after], [false, true, false]);
  } finally {
    await rm(harness.cwd, {recursive: true, force: true});
  }
});

test("shutdown finalizes a paused Turn without waiting for another generator pull", async () => {
  const harness = await createHarness();

  try {
    const engine = createEngine(harness, new BlockingGateway());
    const stream = engine.submit("pause after one delta");
    await pullThrough(stream, "assistant.delta");

    const shutdown = engine.shutdown();
    const settlement = await settlesWithin(shutdown, 250);
    if (!settlement.settled) {
      await collect(stream);
      await shutdown;
    }

    const afterShutdown = await harness.repository.readThread(
      harness.session.activeThread.id
    );
    const checkpointCountAfterShutdown = harness.repository.checkpointCount;
    if (settlement.settled) {
      assert.equal(settlement.error, undefined);
      await collect(stream);
    }
    const afterLaterPull = await harness.repository.readThread(
      harness.session.activeThread.id
    );

    assert.equal(settlement.settled, true);
    assert.equal(afterShutdown.turns.at(-1)?.status, "interrupted");
    assert.equal(
      afterShutdown.items.filter(
        (item) =>
          item.type === "assistant_message" && item.metadata?.interrupted === true
      ).length,
      1
    );
    assert.equal(checkpointCountAfterShutdown, 1);
    assert.equal(harness.repository.checkpointCount, 1);
    assert.deepEqual(afterLaterPull.turns, afterShutdown.turns);
    assert.deepEqual(afterLaterPull.items, afterShutdown.items);
    assert.equal(engine.hasActiveTurn, false);
  } finally {
    await rm(harness.cwd, {recursive: true, force: true});
  }
});

test("shutdown surfaces a forced delta flush failure once", async () => {
  const harness = await createHarness({appendDelta: "forced flush failure"});

  try {
    const engine = createEngine(harness, new BlockingGateway());
    const stream = engine.submit("flush must fail visibly");
    await pullThrough(stream, "assistant.delta");

    const shutdown = engine.shutdown();
    const laterPull = stream.next();
    const [shutdownResult, pullResult] = await Promise.allSettled([
      shutdown,
      laterPull
    ]);

    assert.equal(shutdownResult.status, "rejected");
    assert.match(String(shutdownResult.status === "rejected" && shutdownResult.reason), /forced flush failure/i);
    assert.equal(pullResult.status, "rejected");
    assert.match(String(pullResult.status === "rejected" && pullResult.reason), /forced flush failure/i);
    assert.equal(harness.repository.deltaBatchCount, 1);
    assert.equal(harness.repository.checkpointCount, 0);
  } finally {
    await rm(harness.cwd, {recursive: true, force: true});
  }
});

test("shutdown surfaces a Turn checkpoint failure without duplicate terminal writes", async () => {
  const harness = await createHarness({checkpoint: "checkpoint failure"});

  try {
    const engine = createEngine(harness, new BlockingGateway());
    const stream = engine.submit("checkpoint must fail visibly");
    await pullThrough(stream, "assistant.delta");

    const shutdown = engine.shutdown();
    const laterPull = stream.next();
    const [shutdownResult, pullResult] = await Promise.allSettled([
      shutdown,
      laterPull
    ]);
    const snapshot = await harness.repository.readThread(
      harness.session.activeThread.id
    );

    assert.equal(shutdownResult.status, "rejected");
    assert.match(String(shutdownResult.status === "rejected" && shutdownResult.reason), /checkpoint failure/i);
    assert.equal(pullResult.status, "rejected");
    assert.match(String(pullResult.status === "rejected" && pullResult.reason), /checkpoint failure/i);
    assert.equal(harness.repository.deltaBatchCount, 1);
    assert.equal(harness.repository.checkpointCount, 1);
    assert.equal(
      snapshot.items.filter(
        (item) =>
          item.type === "assistant_message" && item.metadata?.interrupted === true
      ).length,
      1
    );
  } finally {
    await rm(harness.cwd, {recursive: true, force: true});
  }
});
