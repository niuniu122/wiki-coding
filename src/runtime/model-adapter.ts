import type {ApiProtocol, AppConfig, ModelContextMessage} from "../types.js";
import type {ModelToolCall} from "../agent/model-action.js";

export type ModelDiagnosticCode =
  | "provider.request.started"
  | "provider.stream.started"
  | "provider.reasoning.filtered";

export type ModelDiagnosticFacts = Record<
  string,
  string | number | boolean | null | undefined
> & {
  providerId?: string;
  protocol?: ApiProtocol;
  model?: string;
  hiddenCharacters?: number;
};

export type ModelAdapterEvent =
  | {type: "delta"; delta: string}
  | {type: "tool_call"; call: ModelToolCall}
  | {type: "usage"; inputTokens?: number; outputTokens?: number; totalTokens?: number}
  | {type: "diagnostic"; code: ModelDiagnosticCode; facts: ModelDiagnosticFacts}
  | {type: "completed"};

export interface ModelAdapter {
  streamResponse(params: {
    config: AppConfig;
    apiKey: string;
    messages: ModelContextMessage[];
    signal?: AbortSignal;
  }): AsyncGenerator<ModelAdapterEvent>;
}

export type {
  ProviderGateway,
  ProviderGatewayEvent,
  ProviderRequest
} from "../providers/provider-gateway.js";

export {
  ProviderModelAdapter,
  ProviderModelAdapter as MiniMaxModelAdapter
} from "../providers/provider-model-adapter.js";
