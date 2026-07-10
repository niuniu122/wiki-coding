import type {Command} from "../protocol.js";

export type ChatInputAction =
  | {type: "empty"}
  | {type: "busy"}
  | {type: "invalid"; message: string}
  | {type: "command"; command: Command};

export function classifyChatInput(value: string, busy: boolean): ChatInputAction {
  const text = value.trim();
  if (!text) {
    return {type: "empty"};
  }
  if (text === "/interrupt") {
    return {type: "command", command: {type: "turn.interrupt"}};
  }
  if (busy) {
    return {type: "busy"};
  }

  if (text === "/new") {
    return {type: "command", command: {type: "thread.new"}};
  }
  if (text === "/threads") {
    return {type: "command", command: {type: "thread.list"}};
  }
  if (text === "/resume") {
    return {
      type: "invalid",
      message: "用法：/resume <threadId>；先输入 /threads 查看 ID"
    };
  }
  if (text.startsWith("/resume ")) {
    return {
      type: "command",
      command: {type: "thread.resume", threadId: text.slice("/resume ".length).trim()}
    };
  }
  if (text === "/compact") {
    return {type: "command", command: {type: "compact.manual"}};
  }
  if (text === "/api") {
    return {type: "command", command: {type: "config.api_key.request"}};
  }
  if (text === "/provider") {
    return {type: "command", command: {type: "provider.list"}};
  }
  if (text.startsWith("/provider ")) {
    return {
      type: "command",
      command: {type: "provider.switch", providerId: text.slice("/provider ".length).trim()}
    };
  }
  if (text === "/trace") {
    return {type: "command", command: {type: "trace.toggle"}};
  }
  if (text === "/exit" || text === "/quit") {
    return {type: "command", command: {type: "app.exit"}};
  }
  return {type: "command", command: {type: "turn.submit", input: text}};
}
