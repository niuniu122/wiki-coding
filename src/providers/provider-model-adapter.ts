import type {ModelAdapter, ModelAdapterEvent} from "../runtime/model-adapter.js";
import type {AppConfig, ModelContextMessage} from "../types.js";
import {
  StrictProviderGateway,
  type ProviderGateway
} from "./provider-gateway.js";
import {
  FetchHttpStreamTransport,
  type HttpStreamTransport
} from "./http-transport.js";

export class ProviderModelAdapter implements ModelAdapter {
  private readonly gateway: ProviderGateway;

  constructor(
    transport: HttpStreamTransport = new FetchHttpStreamTransport()
  ) {
    this.gateway = new StrictProviderGateway(transport);
  }

  async *streamResponse(params: {
    config: AppConfig;
    apiKey: string;
    messages: ModelContextMessage[];
    signal?: AbortSignal;
  }): AsyncGenerator<ModelAdapterEvent> {
    for await (const event of this.gateway.stream(params)) {
      if (event.type === "text.delta") {
        yield {type: "delta", delta: event.delta};
        continue;
      }
      if (event.type === "tool.call") {
        yield {type: "tool_call", call: event.call};
        continue;
      }
      yield event;
    }
  }
}
