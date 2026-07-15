import {join} from "node:path";
import {ConfigManager} from "../config/config-manager.js";
import type {CapabilityReportService} from "../capabilities/capability-report-service.js";
import {ModelStateStore} from "../config/model-state-store.js";
import {UserProfileStore} from "../config/user-profile-store.js";
import {
  CredentialStore,
  type CredentialBackend,
  type CredentialStatus,
  type PlaintextConsent,
  normalizeApiKey
} from "../config/credential-store.js";
import {formatProviderSummary, getActiveProvider} from "../config/provider-config.js";
import {
  StrictProviderGateway,
  type ProfileProviderGateway,
  type ProfileProviderRequest,
  type ProviderGateway,
  type ProviderGatewayEvent
} from "../providers/provider-gateway.js";
import {BuiltinProviderAdapter} from "../providers/builtin-provider-adapter.js";
import {createDefaultProviderAdapterRegistry} from "../providers/provider-adapter-registry.js";
import type {
  Command,
  ModelSelectionView,
  RuntimeEvent
} from "../protocol.js";
import {JsonlStorageProvider} from "../storage/jsonl-storage.js";
import type {AppConfig, ThreadRecord} from "../types.js";
import {
  CommandArbiter,
  CommandBusyError,
  type KernelPhase
} from "./command-arbiter.js";
import {ContextEngine} from "./context-engine.js";
import {ModelProfileRegistry} from "./model-profile-registry.js";
import {
  ModelSelectionError,
  type ActiveModelSelection
} from "./model-selection-service.js";
import {ProviderService} from "./provider-service.js";
import {SessionService, type SessionTransition} from "./session-service.js";
import type {RuntimeApplication, ShutdownReason} from "./runtime-application.js";
import {StructuredLocalSummaryGenerator} from "./summary-generator.js";
import {SafeTraceRecorder} from "./trace-recorder.js";
import {TurnEngine} from "./turn-engine.js";
import {WorkspaceLease} from "./workspace-lease.js";
import {PermissionService} from "./permission-service.js";
import {RuntimeFeatureFlagService} from "../config/feature-flags.js";
import {LocalCapabilityRuntime} from "../capabilities/local-capability-runtime.js";
import {AgentRunEngine} from "./agent-run-engine.js";

export interface WorkspaceLeasePort {
  acquire(): Promise<void>;
  release(): Promise<void>;
}

export interface ProviderServicePort {
  init(): Promise<void>;
  inspectCredential(): Promise<CredentialStatus>;
  saveApiKey(value: string, consent?: PlaintextConsent): Promise<CredentialBackend>;
  list(): string[];
  switch(providerId: string): Promise<string>;
  listModels?(): Promise<ReadonlyArray<import("../protocol.js").ModelCatalogView>>;
  switchModel?(modelProfileId: string): Promise<ActiveModelSelection>;
  switchProvider?(providerId: string): Promise<ActiveModelSelection>;
  getActiveModelSelection?(): ActiveModelSelection;
  getLegacyCredentialNotice?(): {
    status: "reentry_required";
    path: string;
    hasUsableCredential: boolean;
  } | undefined;
  readonly config: AppConfig;
}

export interface SessionServicePort {
  init(model: string, cwd: string): Promise<RuntimeEvent[]>;
  newThread(model: string, cwd: string): Promise<SessionTransition>;
  listThreads(): Promise<ThreadRecord[]>;
  resumeThread(threadId: string): Promise<SessionTransition>;
}

export interface TurnEnginePort {
  submit(input: string): AsyncGenerator<RuntimeEvent>;
  interrupt(): RuntimeEvent;
  compact(reason: "manual"): Promise<RuntimeEvent[]>;
  shutdown(): Promise<void>;
}

export interface AgentRunEnginePort {
  readonly hasActiveRun: boolean;
  submit(input: string): AsyncGenerator<RuntimeEvent>;
  continue(): AsyncGenerator<RuntimeEvent>;
  interrupt(): RuntimeEvent;
  shutdown(): Promise<void>;
}

export interface CredentialConsentIssuer {
  createPlaintextConsent(): PlaintextConsent;
}

export interface ApplicationKernelServices {
  cwd: string;
  lease: WorkspaceLeasePort;
  arbiter: CommandArbiter;
  providerService: ProviderServicePort;
  sessionService: SessionServicePort;
  turnEngine: TurnEnginePort;
  agentRunEngine?: AgentRunEnginePort;
  credentialStore: CredentialConsentIssuer;
  capabilityService?: Pick<CapabilityReportService, "list" | "search">;
  permissionService?: PermissionService;
  featureFlags?: RuntimeFeatureFlagService;
  initializeOptionalRuntime?(): Promise<void>;
}

export interface ApplicationKernelOptions {
  cwd?: string;
  stateRoot?: string;
  credentialStore?: CredentialStore;
  providerGateway?: ProviderGateway;
  services?: ApplicationKernelServices;
}

class InitializationCancelledError extends Error {
  constructor() {
    super("Runtime initialization was cancelled by shutdown.");
    this.name = "InitializationCancelledError";
  }
}

export class ApplicationKernel implements RuntimeApplication {
  private readonly services: ApplicationKernelServices;
  private initialization: Promise<RuntimeEvent[]> | null = null;
  private shutdownOperation: Promise<void> | null = null;
  private turnShutdownOperation: Promise<void> | null = null;
  private leaseHeld = false;
  private shutdownRequested = false;
  private pendingPlaintextConfirmation = false;
  private plaintextConsent: PlaintextConsent | undefined;

  constructor(options: string | ApplicationKernelOptions = {}) {
    this.services = createServices(
      typeof options === "string" ? {cwd: options} : options
    );
  }

  init(): Promise<RuntimeEvent[]> {
    if (this.shutdownRequested) {
      return Promise.reject(new InitializationCancelledError());
    }
    if (!this.initialization) {
      const operation = this.initialize();
      const tracked = operation.catch((error: unknown) => {
        if (this.initialization === tracked) {
          this.initialization = null;
        }
        throw error;
      });
      this.initialization = tracked;
    }
    return this.initialization;
  }

  async *dispatch(command: Command): AsyncGenerator<RuntimeEvent> {
    let ownership: {finish(): void};
    try {
      ownership = this.services.arbiter.begin(command);
    } catch (error) {
      if (error instanceof CommandBusyError) {
        yield commandRejected(error);
      } else {
        yield {type: "error", message: errorMessage(error)};
      }
      return;
    }

    try {
      yield* this.route(command);
    } catch (error) {
      const secrets =
        command.type === "config.api_key.set"
          ? [command.apiKey.trim(), normalizeApiKey(command.apiKey)]
          : [];
      yield {
        type: "error",
        message: redactSecrets(errorMessage(error), secrets)
      };
    } finally {
      ownership.finish();
    }
  }

  shutdown(reason: ShutdownReason): Promise<void> {
    this.shutdownRequested = true;
    if (!this.shutdownOperation) {
      this.services.arbiter.beginShutdown();
      const operation = this.performShutdown(reason);
      const tracked = operation.catch((error: unknown) => {
        if (this.shutdownOperation === tracked && this.leaseHeld) {
          this.shutdownOperation = null;
        }
        throw error;
      });
      this.shutdownOperation = tracked;
    }
    return this.shutdownOperation;
  }

  private async initialize(): Promise<RuntimeEvent[]> {
    await this.services.lease.acquire();
    this.leaseHeld = true;
    try {
      this.assertInitializationAllowed();
      await this.services.providerService.init();
      this.assertInitializationAllowed();
      await this.services.initializeOptionalRuntime?.();
      this.assertInitializationAllowed();
      const events = await this.services.sessionService.init(
        this.activeModelName(),
        this.services.cwd
      );
      this.assertInitializationAllowed();
      const credential = await this.services.providerService.inspectCredential();
      this.assertInitializationAllowed();
      const legacyNotice = this.services.providerService.getLegacyCredentialNotice?.();
      const legacyEvents: RuntimeEvent[] = legacyNotice
        ? [{
            type: "config.legacy_credential.reentry_required",
            path: legacyNotice.path,
            hasUsableCredential: legacyNotice.hasUsableCredential
          }]
        : [];
      const ready: RuntimeEvent = {
        type: "runtime.ready",
        hasApiKey: credential.hasCredential,
        providerSummary: this.activeProviderSummary(),
        ...(this.services.providerService.getActiveModelSelection
          ? {
              activeModel: modelSelectionToView(
                this.services.providerService.getActiveModelSelection()
              )
            }
          : {}),
        ...(this.services.featureFlags ? {features: this.services.featureFlags.current} : {}),
        recoveredTurns: events.filter((event) => event.type === "turn.recovered" || event.type === "agent.recovery.available" || event.type === "agent.recovery.blocked").length
      };
      this.services.arbiter.markReady();
      return [...events, ...legacyEvents, ready];
    } catch (error) {
      if (this.shutdownRequested) {
        throw error;
      }
      try {
        await this.releaseLease();
      } catch (releaseError) {
        throw new AggregateError(
          [error, releaseError],
          "Runtime initialization failed and its workspace lease could not be released."
        );
      }
      throw error;
    }
  }

  private async *route(command: Command): AsyncGenerator<RuntimeEvent> {
    switch (command.type) {
      case "thread.new":
        this.services.permissionService?.resetSession();
        yield* emitAll(
          (
            await this.services.sessionService.newThread(
              this.activeModelName(),
              this.services.cwd
            )
          ).events
        );
        return;
      case "thread.list":
        yield {
          type: "thread.listed",
          threads: await this.services.sessionService.listThreads()
        };
        return;
      case "thread.resume":
        if (!command.threadId.trim()) {
          throw new Error("Usage: /resume <threadId>; use /threads to list IDs first.");
        }
        this.services.permissionService?.resetSession();
        yield* emitAll(
          (await this.services.sessionService.resumeThread(command.threadId)).events
        );
        return;
      case "turn.submit":
        yield* this.services.turnEngine.submit(command.input);
        return;
      case "agent.submit":
        if (!this.services.agentRunEngine || (this.services.featureFlags && !this.services.featureFlags.current.agentExecution)) {
          yield {type: "agent.stopped", turnId: "unavailable", reason: "agent_disabled"};
          return;
        }
        yield* this.services.agentRunEngine.submit(command.input);
        return;
      case "agent.continue":
        if (!this.services.agentRunEngine || (this.services.featureFlags && !this.services.featureFlags.current.agentExecution)) {
          yield {type: "agent.stopped", turnId: "unavailable", reason: "agent_disabled"};
          return;
        }
        yield* this.services.agentRunEngine.continue();
        return;
      case "turn.interrupt":
        yield this.services.agentRunEngine?.hasActiveRun
          ? this.services.agentRunEngine.interrupt()
          : this.services.turnEngine.interrupt();
        return;
      case "compact.manual":
        yield* emitAll(await this.services.turnEngine.compact("manual"));
        return;
      case "config.api_key.request":
        yield* this.requestApiKeyEntry();
        return;
      case "config.api_key.plaintext.confirm":
        if (!this.pendingPlaintextConfirmation) {
          throw new Error("No plaintext credential confirmation is pending.");
        }
        this.plaintextConsent =
          this.services.credentialStore.createPlaintextConsent();
        this.pendingPlaintextConfirmation = false;
        yield {
          type: "config.api_key.plaintext_confirmed",
          providerSummary: this.activeProviderSummary()
        };
        return;
      case "config.api_key.set":
        yield* this.saveApiKey(command.apiKey);
        return;
      case "provider.list":
        yield {
          type: "provider.listed",
          current: this.activeProviderSummary(),
          providers: this.services.providerService.list()
        };
        return;
      case "provider.switch": {
        if (!command.providerId.trim()) {
          throw new Error("Usage: /provider <providerId>.");
        }
        this.clearPlaintextFlow();
        const switchProvider = this.services.providerService.switchProvider;
        if (!switchProvider) {
          throw new Error("Transactional model selection is not configured.");
        }
        yield* this.switchSelection(() => switchProvider.call(
          this.services.providerService,
          command.providerId
        ));
        return;
      }
      case "model.list": {
        const listModels = this.services.providerService.listModels;
        const getActive = this.services.providerService.getActiveModelSelection;
        if (!listModels || !getActive) {
          throw new Error("Model catalog is not configured.");
        }
        yield {
          type: "model.listed",
          current: modelSelectionToView(
            getActive.call(this.services.providerService)
          ),
          models: await listModels.call(this.services.providerService)
        };
        return;
      }
      case "model.switch": {
        if (!command.modelProfileId.trim()) {
          throw new Error("Usage: /model <fully-qualified-model-id>.");
        }
        this.clearPlaintextFlow();
        const switchModel = this.services.providerService.switchModel;
        if (!switchModel) {
          throw new Error("Transactional model selection is not configured.");
        }
        yield* this.switchSelection(() => switchModel.call(
          this.services.providerService,
          command.modelProfileId
        ));
        return;
      }
      case "capability.list": {
        if (!this.services.capabilityService || (this.services.featureFlags && !this.services.featureFlags.current.capabilityCatalog)) {
          yield {type: "capability.unavailable", reason: "disabled"};
          return;
        }
        const report = this.services.capabilityService.list();
        yield {type: "capability.listed", ...report};
        return;
      }
      case "capability.search": {
        if (!command.query.trim()) {
          throw new Error("Usage: /capabilities search <query>.");
        }
        if (!this.services.capabilityService || (this.services.featureFlags && !this.services.featureFlags.current.capabilityCatalog)) {
          yield {type: "capability.unavailable", reason: "disabled"};
          return;
        }
        const report = await this.services.capabilityService.search(command.query);
        yield {type: "capability.searched", query: command.query, ...report};
        return;
      }
      case "permission.show":
        yield {type: "permission.current", mode: this.services.permissionService?.current ?? "confirm"};
        return;
      case "permission.set":
        if (!this.services.permissionService) throw new Error("Session permissions are not configured.");
        yield {type: "permission.changed", mode: this.services.permissionService.set(command.mode)};
        return;
      case "trace.toggle":
        yield {type: "trace.toggle.requested"};
        return;
      case "app.exit":
        yield {type: "app.exit.requested"};
        return;
      default:
        return assertNever(command);
    }
  }

  private async *requestApiKeyEntry(): AsyncGenerator<RuntimeEvent> {
    const status = await this.services.providerService.inspectCredential();
    this.plaintextConsent = undefined;
    if (status.backend === "unavailable" || status.backend === "user-file") {
      this.pendingPlaintextConfirmation = true;
      yield {
        type: "config.api_key.plaintext_confirmation_required",
        path: status.userFilePath
      };
      return;
    }
    this.pendingPlaintextConfirmation = false;
    yield {
      type: "config.api_key.requested",
      providerSummary: this.activeProviderSummary()
    };
  }

  private async *switchSelection(
    operation: () => Promise<ActiveModelSelection>
  ): AsyncGenerator<RuntimeEvent> {
    try {
      yield {
        type: "model.changed",
        selection: modelSelectionToView(await operation())
      };
    } catch (error) {
      if (error instanceof ModelSelectionError) {
        yield {
          type: "model.change_failed",
          code: error.code,
          ...(error.configuredDefaultModelProfileId
            ? {
                configuredDefaultModelProfileId:
                  error.configuredDefaultModelProfileId
              }
            : {})
        };
        return;
      }
      yield {type: "model.change_failed", code: "selection_failed"};
    }
  }

  private async *saveApiKey(apiKey: string): AsyncGenerator<RuntimeEvent> {
    const normalized = apiKey.trim();
    if (!normalized) {
      throw new Error("API key cannot be empty.");
    }

    const consent = this.plaintextConsent;
    this.plaintextConsent = undefined;
    const backend = await this.services.providerService.saveApiKey(
      normalized,
      consent
    );
    if (backend === "unavailable") {
      const status = await this.services.providerService.inspectCredential();
      this.pendingPlaintextConfirmation = true;
      yield {
        type: "config.api_key.plaintext_confirmation_required",
        path: status.userFilePath
      };
      return;
    }
    if (backend === "environment") {
      throw new Error("An environment credential cannot be written by the Runtime.");
    }

    this.pendingPlaintextConfirmation = false;
    yield {
      type: "config.api_key.saved",
      location: backend,
      providerSummary: this.activeProviderSummary()
    };
  }

  private async performShutdown(reason: ShutdownReason): Promise<void> {
    void reason;
    this.clearPlaintextFlow();
    const initialization = this.initialization;
    if (initialization) {
      try {
        await initialization;
      } catch {
        // The init caller retains its own failure; shutdown owns final cleanup.
      }
    }

    let turnError: unknown;
    try {
      this.turnShutdownOperation ??= Promise.all([
        this.services.turnEngine.shutdown(),
        this.services.agentRunEngine?.shutdown() ?? Promise.resolve()
      ]).then(() => undefined);
      await this.turnShutdownOperation;
    } catch (error) {
      turnError = error;
    }
    await this.services.arbiter.waitForMutation();

    let releaseError: unknown;
    try {
      await this.releaseLease();
    } catch (error) {
      releaseError = error;
    }

    if (turnError !== undefined && releaseError !== undefined) {
      throw new AggregateError(
        [turnError, releaseError],
        "Runtime shutdown and workspace lease release both failed."
      );
    }
    if (turnError !== undefined) {
      throw turnError;
    }
    if (releaseError !== undefined) {
      throw releaseError;
    }
    this.services.arbiter.markStopped();
  }

  private async releaseLease(): Promise<void> {
    if (!this.leaseHeld) {
      return;
    }
    await this.services.lease.release();
    this.leaseHeld = false;
  }

  private activeProviderSummary(): string {
    const selection = this.services.providerService.getActiveModelSelection?.();
    if (selection) {
      return `${selection.providerDisplayName} | ${selection.protocol} | model=${selection.model}`;
    }
    const config = this.services.providerService.config;
    return formatProviderSummary(config, getActiveProvider(config));
  }

  private activeModelName(): string {
    return (
      this.services.providerService.getActiveModelSelection?.().model ??
      this.services.providerService.config.model
    );
  }

  private assertInitializationAllowed(): void {
    if (this.shutdownRequested) {
      throw new InitializationCancelledError();
    }
  }

  private clearPlaintextFlow(): void {
    this.pendingPlaintextConfirmation = false;
    this.plaintextConsent = undefined;
  }
}

function modelSelectionToView(
  selection: ActiveModelSelection
): ModelSelectionView {
  return Object.freeze({
    adapterId: selection.adapterId,
    providerProfileId: selection.providerProfileId,
    modelProfileId: selection.modelProfileId,
    providerDisplayName: selection.providerDisplayName,
    modelDisplayName: selection.modelDisplayName,
    model: selection.model,
    protocol: selection.protocol,
    source: selection.source,
    supportsNativeToolCalls: selection.supportsNativeToolCalls
  });
}

function createServices(options: ApplicationKernelOptions): ApplicationKernelServices {
  if (options.services) {
    return options.services;
  }

  const cwd = options.cwd ?? process.cwd();
  const stateRoot = options.stateRoot ?? join(cwd, ".mini-codex");
  const credentialStore = options.credentialStore ?? new CredentialStore();
  const providerGateway = options.providerGateway ?? new StrictProviderGateway();
  const profileProviderGateway = supportsProfileGateway(providerGateway)
    ? providerGateway
    : new LegacyProfileProviderGateway(providerGateway);
  const builtinAdapter = new BuiltinProviderAdapter(
    undefined,
    profileProviderGateway
  );
  const modelProfileRegistry = new ModelProfileRegistry(
    createDefaultProviderAdapterRegistry(builtinAdapter),
    new UserProfileStore({userConfigDir: credentialStore.configRoot})
  );
  let turnEngine: TurnEngine | undefined;
  let agentRunEngine: AgentRunEngine | undefined;
  const providerService = new ProviderService(
    new ConfigManager(stateRoot),
    credentialStore,
    {
      migrate: (target) =>
        credentialStore.migrateLegacyWorkspaceCredential(
          target,
          join(stateRoot, "secrets.local.json")
        )
    },
    modelProfileRegistry,
    {
      stateStore: new ModelStateStore({userConfigDir: credentialStore.configRoot}),
      activity: {isTurnActive: () => (turnEngine?.hasActiveTurn ?? false) || (agentRunEngine?.hasActiveRun ?? false)}
    }
  );
  const repository = new JsonlStorageProvider(stateRoot);
  const traceRecorder = new SafeTraceRecorder();
  const sessionService = new SessionService(repository, traceRecorder);
  const permissionService = new PermissionService();
  // Default-route gate remains closed until Task 22's complete offline release report passes.
  const featureFlags = new RuntimeFeatureFlagService(false);
  const capabilityRuntime = new LocalCapabilityRuntime({
    workspaceRoot: cwd,
    stateRoot,
    userConfigRoot: credentialStore.configRoot,
    getPermissionMode: () => permissionService.current
  });
  turnEngine = new TurnEngine({
    sessionService,
    modelRuntime: providerService,
    repository,
    contextEngine: new ContextEngine(),
    summaryGenerator: new StructuredLocalSummaryGenerator(),
    traceRecorder
  });
  agentRunEngine = new AgentRunEngine({
    sessionService,
    repository,
    modelRuntime: providerService,
    retriever: capabilityRuntime,
    createDispatcher: (recorder) => capabilityRuntime.createDispatcher(recorder)
  });

  return {
    cwd,
    lease: new WorkspaceLease(stateRoot),
    arbiter: new CommandArbiter(),
    providerService,
    sessionService,
    turnEngine,
    agentRunEngine,
    credentialStore,
    permissionService,
    featureFlags,
    capabilityService: capabilityRuntime,
    initializeOptionalRuntime: async () => {
      const flags = featureFlags.initialize(providerService.config.features);
      try {
        await capabilityRuntime.initialize(flags);
      } catch {
        featureFlags.disableCapabilityRuntime();
      }
    }
  };
}

class LegacyProfileProviderGateway implements ProfileProviderGateway {
  constructor(private readonly gateway: ProviderGateway) {}

  async *streamProfile(
    request: ProfileProviderRequest
  ): AsyncGenerator<ProviderGatewayEvent> {
    const providerId = request.providerProfile.providerProfileId;
    yield* this.gateway.stream({
      config: {
        schemaVersion: 1,
        modelProvider: providerId,
        modelProviders: {
          [providerId]: {
            name: request.providerProfile.displayName,
            baseUrl: request.providerProfile.transport.baseUrl,
            protocol: request.providerProfile.transport.protocol,
            headers: {...request.providerProfile.transport.publicHeaders},
            allowInsecureLoopback:
              request.providerProfile.transport.allowInsecureLoopback,
            ...(request.providerProfile.authentication.envBinding
              ? {envKey: request.providerProfile.authentication.envBinding}
              : {}),
            defaultModel: request.modelProfile.model
          }
        },
        model: request.modelProfile.model,
        context: {
          workingContextLimit:
            request.modelProfile.featureProfile.contextWindow,
          autoCompactRatio: 0.9,
          maxCompletionTokens: request.maxOutputTokens
        }
      },
      apiKey: request.apiKey,
      messages: [...request.messages],
      ...(request.signal ? {signal: request.signal} : {})
    });
  }
}

function supportsProfileGateway(
  gateway: ProviderGateway
): gateway is ProviderGateway & ProfileProviderGateway {
  return (
    "streamProfile" in gateway &&
    typeof (gateway as Partial<ProfileProviderGateway>).streamProfile === "function"
  );
}

async function* emitAll(events: RuntimeEvent[]): AsyncGenerator<RuntimeEvent> {
  for (const event of events) {
    yield event;
  }
}

function commandRejected(error: CommandBusyError): RuntimeEvent {
  return {
    type: "command.rejected",
    commandType: error.commandType,
    phase: error.phase,
    message: error.message
  };
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function redactSecrets(message: string, secrets: readonly string[]): string {
  return secrets
    .filter((secret, index) => secret && secrets.indexOf(secret) === index)
    .reduce(
      (redacted, secret) => redacted.split(secret).join("[REDACTED]"),
      message
    );
}

function assertNever(value: never): never {
  throw new Error(`Unhandled command: ${JSON.stringify(value)}`);
}

export type {KernelPhase};
