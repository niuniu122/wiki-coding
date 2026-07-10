import type {Command, RuntimeEvent} from "../protocol.js";
import type {ThreadRecord} from "../types.js";
import {AgentRuntime} from "./agent-runtime.js";

export interface RuntimePort {
  init(): Promise<RuntimeEvent[]>;
  hasApiKey(): Promise<boolean>;
  getProviderSummary(): string;
  setApiKey(apiKey: string): Promise<"keychain" | "user-file">;
  listProviderSummaries(): string[];
  switchProvider(providerId: string): Promise<string>;
  newThread(): Promise<RuntimeEvent[]>;
  listThreads(): Promise<ThreadRecord[]>;
  resumeThread(threadId: string): Promise<RuntimeEvent[]>;
  interruptCurrentTurn(): RuntimeEvent;
  compact(reason: "manual"): Promise<RuntimeEvent[]>;
  submitUserInput(input: string): AsyncGenerator<RuntimeEvent>;
}

export class CommandDispatcher {
  constructor(private readonly runtime: RuntimePort = new AgentRuntime()) {}

  async init(): Promise<RuntimeEvent[]> {
    try {
      const events = await this.runtime.init();
      events.push({
        type: "runtime.ready",
        hasApiKey: await this.runtime.hasApiKey(),
        providerSummary: this.runtime.getProviderSummary(),
        recoveredTurns: events.filter((event) => event.type === "turn.recovered").length
      });
      return events;
    } catch (error) {
      return [{type: "error", message: errorMessage(error)}];
    }
  }

  async *dispatch(command: Command): AsyncGenerator<RuntimeEvent> {
    try {
      switch (command.type) {
        case "thread.new":
          yield* emitAll(await this.runtime.newThread());
          return;
        case "thread.list":
          yield {type: "thread.listed", threads: await this.runtime.listThreads()};
          return;
        case "thread.resume":
          if (!command.threadId) {
            throw new Error("用法：/resume <threadId>；先输入 /threads 查看 ID");
          }
          yield* emitAll(await this.runtime.resumeThread(command.threadId));
          return;
        case "turn.submit":
          yield* this.runtime.submitUserInput(command.input);
          return;
        case "turn.interrupt":
          yield this.runtime.interruptCurrentTurn();
          return;
        case "compact.manual":
          yield* emitAll(await this.runtime.compact("manual"));
          return;
        case "config.api_key.request":
          yield {
            type: "config.api_key.requested",
            providerSummary: this.runtime.getProviderSummary()
          };
          return;
        case "config.api_key.set": {
          const apiKey = command.apiKey.trim();
          if (!apiKey) {
            throw new Error("API key 不能为空");
          }
          const location = await this.runtime.setApiKey(apiKey);
          yield {
            type: "config.api_key.saved",
            location,
            providerSummary: this.runtime.getProviderSummary()
          };
          return;
        }
        case "provider.list":
          yield {
            type: "provider.listed",
            current: this.runtime.getProviderSummary(),
            providers: this.runtime.listProviderSummaries()
          };
          return;
        case "provider.switch": {
          if (!command.providerId) {
            throw new Error("用法：/provider <providerId>");
          }
          const summary = await this.runtime.switchProvider(command.providerId);
          yield {
            type: "provider.changed",
            summary,
            hasApiKey: await this.runtime.hasApiKey()
          };
          return;
        }
        case "trace.toggle":
          yield {type: "trace.toggle.requested"};
          return;
        case "app.exit":
          yield {type: "app.exit.requested"};
          return;
      }
    } catch (error) {
      const secret = command.type === "config.api_key.set" ? command.apiKey.trim() : "";
      yield {type: "error", message: redactSecret(errorMessage(error), secret)};
    }
  }
}

async function* emitAll(events: RuntimeEvent[]): AsyncGenerator<RuntimeEvent> {
  for (const event of events) {
    yield event;
  }
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function redactSecret(message: string, secret: string): string {
  return secret ? message.split(secret).join("[REDACTED]") : message;
}
