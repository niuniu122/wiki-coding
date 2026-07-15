import {homedir} from "node:os";
import {join, resolve} from "node:path";
import type {ResolvedAgentFeatureFlags} from "../config/feature-flags.js";
import type {PermissionMode} from "../runtime/permission-service.js";
import type {AgentCapabilityRetriever} from "../runtime/agent-run-engine.js";
import {builtinCapabilityDescriptors} from "./builtin-capabilities.js";
import {CapabilityCatalog} from "./capability-catalog.js";
import {CapabilityDispatcher, type CapabilityInvocationRecorder} from "./capability-dispatcher.js";
import {CapabilityReportService} from "./capability-report-service.js";
import {createCapabilitySnapshot, type CapabilitySnapshot} from "./capability-snapshot.js";
import {CapabilityRefreshCoordinator} from "./refresh-coordinator.js";
import {HybridCapabilityRetriever} from "./search/hybrid-retriever.js";
import {CapabilitySnapshotStore} from "./snapshot-store.js";
import type {CapabilitySourceAdapter} from "./source-adapter.js";
import {ClawCodeCapabilitySource} from "./sources/claw-code-source.js";
import {CodexCapabilitySource, CodexPluginCapabilitySource} from "./sources/codex-source.js";
import {MiniMaxCapabilitySource} from "./sources/minimax-source.js";

export interface LocalCapabilityRuntimeOptions {
  readonly workspaceRoot: string;
  readonly stateRoot: string;
  readonly userConfigRoot: string;
  readonly getPermissionMode: () => PermissionMode;
  readonly env?: Readonly<Record<string, string | undefined>>;
  readonly homeDir?: string;
}

export class LocalCapabilityRuntime implements AgentCapabilityRetriever {
  private flags: ResolvedAgentFeatureFlags | undefined;
  private snapshot: CapabilitySnapshot | undefined;
  private report: CapabilityReportService | undefined;
  private retriever: HybridCapabilityRetriever | undefined;
  private readonly coordinator: CapabilityRefreshCoordinator;

  constructor(private readonly options: LocalCapabilityRuntimeOptions) {
    this.coordinator = new CapabilityRefreshCoordinator(
      new CapabilitySnapshotStore(options.stateRoot),
      () => this.buildSnapshot()
    );
  }

  async initialize(flags: ResolvedAgentFeatureFlags): Promise<void> {
    this.flags = flags;
    if (!flags.capabilityCatalog) return;
    await this.coordinator.initialize();
    this.snapshot = await this.coordinator.refresh();
    this.report = new CapabilityReportService(this.snapshot);
    // Embedding is an optional resource package. Without an explicitly installed runtime,
    // the hybrid retriever intentionally remains exact + BM25 and reports its fallback.
    this.retriever = new HybridCapabilityRetriever(this.snapshot.entries.map((entry) => entry.descriptor).filter((item) => item.availability === "available"));
  }

  list() {
    this.assertCatalog();
    return this.report!.list();
  }

  search(query: string) {
    this.assertCatalog();
    return this.report!.search(query);
  }

  async retrieve(query: string, inputBudgetTokens?: number) {
    this.assertCatalog();
    const result = await this.retriever!.retrieve(query, inputBudgetTokens);
    return Object.freeze({...result, snapshotVersion: this.snapshot!.version});
  }

  createDispatcher(recorder: CapabilityInvocationRecorder): CapabilityDispatcher {
    this.assertCatalog();
    return new CapabilityDispatcher({
      workspaceRoot: this.options.workspaceRoot,
      getSnapshot: () => this.snapshot!,
      getPermissionMode: this.options.getPermissionMode,
      recorder
    });
  }

  private assertCatalog(): void {
    if (!this.flags?.capabilityCatalog || !this.snapshot || !this.report || !this.retriever) throw new Error("Local capability catalog is disabled.");
  }

  private async buildSnapshot(): Promise<CapabilitySnapshot> {
    const descriptors = [...builtinCapabilityDescriptors()];
    const results = await Promise.all(this.sources().map((source) => source.scan()));
    for (const result of results) descriptors.push(...result.descriptors);
    return createCapabilitySnapshot(CapabilityCatalog.build(descriptors).entries());
  }

  private sources(): CapabilitySourceAdapter[] {
    const env = this.options.env ?? process.env;
    const home = this.options.homeDir ?? homedir();
    const sources: CapabilitySourceAdapter[] = [
      new MiniMaxCapabilitySource(join(this.options.stateRoot, "capabilities"), "project_native"),
      new MiniMaxCapabilitySource(join(this.options.userConfigRoot, "capabilities"), "user_native"),
      new CodexCapabilitySource(join(this.options.workspaceRoot, ".agents", "skills"), "project_compat"),
      new CodexCapabilitySource(join(this.options.workspaceRoot, ".codex", "skills"), "project_compat"),
      new CodexPluginCapabilitySource(join(this.options.workspaceRoot, ".codex", "plugins"), "project_compat"),
      new CodexCapabilitySource(join(home, ".agents", "skills"), "user_compat")
    ];
    if (env.CODEX_HOME?.trim()) {
      sources.push(new CodexCapabilitySource(join(resolve(env.CODEX_HOME), "skills"), "user_compat"));
      sources.push(new CodexPluginCapabilitySource(join(resolve(env.CODEX_HOME), "plugins"), "user_compat"));
    }
    if (env.CLAW_CODE_HOME?.trim()) sources.push(new ClawCodeCapabilitySource(join(resolve(env.CLAW_CODE_HOME), "commands"), "user_compat"));
    return sources;
  }
}
