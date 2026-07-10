export type Role = "system" | "user" | "assistant";

export type ItemType =
  | "user_message"
  | "assistant_message"
  | "trace_event"
  | "context_summary"
  | "compaction"
  | "api_request"
  | "api_response"
  | "error";

export type ThreadStatus = "active" | "archived";
export type TurnStatus = "running" | "completed" | "failed" | "interrupted";
export type StorageDriver = "jsonl" | "sqlite";
export type ApiProtocol = "responses" | "chat_completions";
export type ApiProviderId = string;

export interface ModelProviderConfig {
  name: string;
  baseUrl: string;
  protocol: ApiProtocol;
  envKey?: string;
  defaultModel?: string;
  headers?: Record<string, string>;
  supportsThinkTags?: boolean;
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

export interface AppConfig {
  modelProvider: ApiProviderId;
  modelProviders: Record<ApiProviderId, ModelProviderConfig>;
  api: {
    provider: "minimax" | "hashsight" | "openai-compatible";
    protocol: ApiProtocol;
    baseUrl: string;
  };
  model: string;
  storage: {
    driver: StorageDriver;
  };
  context: {
    workingContextLimit: number;
    autoCompactRatio: number;
    maxCompletionTokens: number;
  };
}

export interface ModelContextMessage {
  role: Role;
  content: string;
}
