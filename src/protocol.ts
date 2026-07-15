import type {ThreadItem, ThreadRecord, TraceEvent, TurnRecord} from "./types.js";

export interface ModelSelectionView {
  readonly adapterId: string;
  readonly providerProfileId: string;
  readonly modelProfileId: string;
  readonly providerDisplayName: string;
  readonly modelDisplayName: string;
  readonly model: string;
  readonly protocol: string;
  readonly source: "builtin" | "legacy_workspace" | "user";
  readonly supportsNativeToolCalls: boolean;
}

export interface ModelCatalogView {
  readonly modelProfileId: string;
  readonly providerProfileId: string;
  readonly modelDisplayName: string;
  readonly providerDisplayName: string;
  readonly model: string;
  readonly source: "builtin" | "legacy_workspace" | "user";
  readonly availability: "active" | "available" | "unavailable";
  readonly reason?:
    | "credential_unavailable"
    | "workspace_profile_requires_promotion";
}

export interface FeatureFlagsView {
  readonly capabilityCatalog: boolean;
  readonly capabilityEmbedding: boolean;
  readonly agentExecution: boolean;
  readonly agentDefaultRoute: boolean;
  readonly diagnostics: readonly string[];
}

export type Command =
  | {type: "thread.new"}
  | {type: "thread.list"}
  | {type: "thread.resume"; threadId: string}
  | {type: "turn.submit"; input: string}
  | {type: "agent.submit"; input: string}
  | {type: "agent.continue"}
  | {type: "turn.interrupt"}
  | {type: "compact.manual"}
  | {type: "config.api_key.request"}
  | {type: "config.api_key.plaintext.confirm"}
  | {type: "config.api_key.set"; apiKey: string}
  | {type: "provider.list"}
  | {type: "provider.switch"; providerId: string}
  | {type: "model.list"}
  | {type: "model.switch"; modelProfileId: string}
  | {type: "capability.list"}
  | {type: "capability.search"; query: string}
  | {type: "permission.show"}
  | {type: "permission.set"; mode: "confirm" | "workspace_read" | "full_access"}
  | {type: "trace.toggle"}
  | {type: "app.exit"};

export type RuntimeEvent =
  | {
      type: "runtime.ready";
      hasApiKey: boolean;
      providerSummary: string;
      activeModel?: ModelSelectionView;
      features?: FeatureFlagsView;
      recoveredTurns: number;
    }
  | {type: "runtime.init_failed"; message: string}
  | {type: "thread.loaded"; thread: ThreadRecord}
  | {type: "thread.listed"; threads: ThreadRecord[]}
  | {type: "history.loaded"; items: ThreadItem[]}
  | {type: "turn.started"; turnId: string; input: string}
  | {type: "turn.recovered"; turn: TurnRecord}
  | {type: "turn.interrupt.requested"; turnId: string}
  | {type: "turn.interrupt.ignored"; reason: "no_active_request"}
  | {type: "turn.interrupted"; turnId: string}
  | {type: "agent.started"; turnId: string; input: string}
  | {type: "agent.retrieval.started"; turnId: string; query: string}
  | {type: "agent.retrieval.completed"; turnId: string; snapshotVersion: string; candidates: readonly string[]; path: string}
  | {type: "agent.model.started"; turnId: string; step: number}
  | {type: "agent.assistant.delta"; turnId: string; delta: string}
  | {type: "agent.tool.requested"; turnId: string; invocationId: string; capabilityId: string}
  | {type: "agent.tool.completed"; turnId: string; invocationId: string; status: string}
  | {type: "agent.permission.required"; turnId: string; invocationId: string; capabilityId: string}
  | {type: "agent.completed"; turnId: string; item: ThreadItem}
  | {type: "agent.stopped"; turnId: string; reason: string}
  | {type: "agent.recovery.available"; turnId: string; checkpointId: string}
  | {type: "agent.recovery.blocked"; turnId: string; reason: "indeterminate_invocation" | "invalid_checkpoint"}
  | {type: "agent.continued"; turnId: string; checkpointId: string}
  | {type: "assistant.delta"; turnId: string; delta: string}
  | {type: "assistant.completed"; item: ThreadItem}
  | {type: "trace.event"; event: TraceEvent}
  | {type: "token.usage"; used: number; limit: number; autoCompactAt: number}
  | {type: "compact.started"; reason: "manual" | "auto" | "resume"}
  | {
      type: "compact.completed";
      summary: string;
      compacted: boolean;
      coveredThroughItemId?: string;
      beforeTokens: number;
      afterTokens: number;
    }
  | {type: "api.status"; status: "idle" | "requesting" | "streaming" | "completed"}
  | {type: "config.api_key.requested"; providerSummary: string}
  | {
      type: "config.legacy_credential.reentry_required";
      path: string;
      hasUsableCredential: boolean;
    }
  | {
      type: "config.api_key.plaintext_confirmation_required";
      path: string;
    }
  | {type: "config.api_key.plaintext_confirmed"; providerSummary: string}
  | {
      type: "config.api_key.saved";
      location: "keychain" | "os-keyring" | "user-file";
      providerSummary: string;
    }
  | {type: "provider.listed"; current: string; providers: string[]}
  | {type: "provider.changed"; summary: string; hasApiKey: boolean}
  | {
      type: "model.listed";
      current: ModelSelectionView;
      models: readonly ModelCatalogView[];
    }
  | {type: "model.changed"; selection: ModelSelectionView}
  | {
      type: "model.change_failed";
      code:
        | "not_initialized"
        | "model_unavailable"
        | "credential_unavailable"
        | "turn_active"
        | "sticky_selection_forbidden"
        | "agent_feature_unsupported"
        | "recovery_required"
        | "selection_failed";
      configuredDefaultModelProfileId?: string;
    }
  | {
      type: "capability.listed";
      snapshotVersion: string;
      health: string;
      mode: string;
      capabilities: readonly {
        id: string;
        name: string;
        source: string;
        status: string;
        safetyClass: string;
        shadowedBy?: string;
      }[];
    }
  | {
      type: "capability.searched";
      query: string;
      snapshotVersion: string;
      health: string;
      mode: string;
      fallbackReason?: string;
      candidates: readonly {
        id: string;
        name: string;
        source: string;
        status: string;
        safetyClass: string;
        matchPath: string;
      }[];
    }
  | {type: "capability.unavailable"; reason: "disabled" | "not_initialized"}
  | {type: "permission.current"; mode: "confirm" | "workspace_read" | "full_access"}
  | {type: "permission.changed"; mode: "confirm" | "workspace_read" | "full_access"}
  | {type: "trace.toggle.requested"}
  | {type: "app.exit.requested"}
  | {
      type: "command.rejected";
      commandType: Command["type"];
      phase:
        | "booting"
        | "idle"
        | "running_mutation"
        | "running_turn"
        | "shutting_down"
        | "stopped";
      message: string;
    }
  | {type: "error"; message: string; turnId?: string};
