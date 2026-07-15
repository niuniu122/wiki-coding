export type Role = "system" | "user" | "assistant";

export type ItemType =
  | "user_message"
  | "assistant_message"
  | "trace_event"
  | "error"
  | "agent_item";

export type ThreadStatus = "active" | "archived";
export type TurnStatus = "running" | "completed" | "failed" | "interrupted";
export type ApiProtocol = "responses" | "chat_completions";
export type ApiProviderId = string;

declare const providerAdapterIdBrand: unique symbol;
declare const providerProfileIdBrand: unique symbol;
declare const modelProfileIdBrand: unique symbol;

export type ProviderAdapterId = string & {
  readonly [providerAdapterIdBrand]: "ProviderAdapterId";
};
export type ProviderProfileId = string & {
  readonly [providerProfileIdBrand]: "ProviderProfileId";
};
export type ModelProfileId = string & {
  readonly [modelProfileIdBrand]: "ModelProfileId";
};

export interface ModelProviderConfig {
  name: string;
  baseUrl: string;
  protocol: ApiProtocol;
  envKey?: string;
  defaultModel?: string;
  headers?: Record<string, string>;
  allowInsecureLoopback?: boolean;
}

export interface ThreadRecord {
  id: string;
  title: string;
  createdAt: string;
  updatedAt: string;
  model: string;
  cwd: string;
  status: ThreadStatus;
}

export interface TurnRecord {
  id: string;
  threadId: string;
  userInput: string;
  status: TurnStatus;
  startedAt: string;
  completedAt?: string;
  assistantDraft?: string;
  modelProvenance?: TurnModelProvenance;
}

export interface TurnModelProvenance {
  readonly schemaVersion: 1;
  readonly adapterId: string;
  readonly providerProfileId: string;
  readonly modelProfileId: string;
  readonly model: string;
  readonly protocol: ApiProtocol;
}

export interface ThreadItem {
  id: string;
  threadId: string;
  turnId?: string;
  type: ItemType;
  role?: Role;
  content: string;
  createdAt: string;
  metadata?: Record<string, unknown>;
  agent?: import("./agent/agent-item.js").AgentItemEnvelope;
}

export type TraceCategory = "lifecycle" | "provider" | "context" | "error";
export type TraceCode =
  | "turn.start"
  | "turn.recovered"
  | "turn.interrupted"
  | "compact.completed"
  | "compact.limit"
  | "provider.request.started"
  | "provider.stream.started"
  | "provider.reasoning.filtered"
  | "provider.request.failed";
export type TraceFact = string | number | boolean | null;

export interface TraceEvent {
  id: string;
  threadId: string;
  turnId?: string;
  category: TraceCategory;
  code: TraceCode;
  message: string;
  createdAt: string;
  facts?: Record<string, TraceFact>;
}

export interface ContextSummary {
  id: string;
  threadId: string;
  createdAt: string;
  content: string;
  tokenEstimate: number;
  coveredThroughItemId?: string;
}

export interface ContextConfig {
  workingContextLimit: number;
  autoCompactRatio: number;
  maxCompletionTokens: number;
}

export interface AppConfig {
  schemaVersion: 1;
  modelProvider: ApiProviderId;
  modelProviders: Record<ApiProviderId, ModelProviderConfig>;
  model: string;
  context: ContextConfig;
  features?: import("./config/feature-flags.js").AgentFeatureFlagConfig;
}

export interface ModelContextMessage {
  role: Role;
  content: string;
}
