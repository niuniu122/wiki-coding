import type {ThreadItem, ThreadRecord, TraceEvent, TurnRecord} from "./types.js";

export type Command =
  | {type: "thread.new"}
  | {type: "thread.list"}
  | {type: "thread.resume"; threadId: string}
  | {type: "turn.submit"; input: string}
  | {type: "turn.interrupt"}
  | {type: "compact.manual"}
  | {type: "config.api_key.request"}
  | {type: "config.api_key.set"; apiKey: string}
  | {type: "provider.list"}
  | {type: "provider.switch"; providerId: string}
  | {type: "trace.toggle"}
  | {type: "app.exit"};

export type RuntimeEvent =
  | {
      type: "runtime.ready";
      hasApiKey: boolean;
      providerSummary: string;
      recoveredTurns: number;
    }
  | {type: "thread.loaded"; thread: ThreadRecord}
  | {type: "thread.listed"; threads: ThreadRecord[]}
  | {type: "history.loaded"; items: ThreadItem[]}
  | {type: "turn.started"; turnId: string; input: string}
  | {type: "turn.recovered"; turn: TurnRecord}
  | {type: "turn.interrupt.requested"; turnId: string}
  | {type: "turn.interrupt.ignored"; reason: "no_active_request"}
  | {type: "turn.interrupted"; turnId: string}
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
      type: "config.api_key.saved";
      location: "keychain" | "user-file";
      providerSummary: string;
    }
  | {type: "provider.listed"; current: string; providers: string[]}
  | {type: "provider.changed"; summary: string; hasApiKey: boolean}
  | {type: "trace.toggle.requested"}
  | {type: "app.exit.requested"}
  | {type: "error"; message: string; turnId?: string};
