import type {Command} from "../protocol.js";
import type {UiState} from "./ui-state.js";

export type ChatInputAction =
  | {type: "empty"}
  | {type: "invalid"; message: string}
  | {type: "retry_init"}
  | {type: "command"; command: Command};

export type PlaintextConfirmationAction =
  | {type: "cancel"}
  | {
      type: "command";
      command: Extract<Command, {type: "config.api_key.plaintext.confirm"}>;
    };

export function classifyChatInput(value: string, options: {agentDefaultRoute?: boolean} = {}): ChatInputAction {
  const text = value.trim();
  if (!text) {
    return {type: "empty"};
  }
  if (text === "/interrupt") {
    return {type: "command", command: {type: "turn.interrupt"}};
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
  if (text === "/continue") {
    return {type: "command", command: {type: "agent.continue"}};
  }
  if (text === "/agent") {
    return {type: "invalid", message: "用法：/agent <request>"};
  }
  if (text.startsWith("/agent ")) {
    return {type: "command", command: {type: "agent.submit", input: text.slice("/agent ".length).trim()}};
  }
  if (text === "/chat") {
    return {type: "invalid", message: "用法：/chat <request>"};
  }
  if (text.startsWith("/chat ")) {
    return {type: "command", command: {type: "turn.submit", input: text.slice("/chat ".length).trim()}};
  }
  if (text === "/models") {
    return {type: "command", command: {type: "model.list"}};
  }
  if (text === "/model") {
    return {
      type: "invalid",
      message: "用法：/model <fully-qualified-model-id>；先输入 /models 查看 ID"
    };
  }
  if (text.startsWith("/model ")) {
    return {
      type: "command",
      command: {
        type: "model.switch",
        modelProfileId: text.slice("/model ".length).trim()
      }
    };
  }
  if (text === "/capabilities") {
    return {type: "command", command: {type: "capability.list"}};
  }
  if (text === "/capabilities search") {
    return {type: "invalid", message: "用法：/capabilities search <query>"};
  }
  if (text.startsWith("/capabilities search ")) {
    return {
      type: "command",
      command: {
        type: "capability.search",
        query: text.slice("/capabilities search ".length).trim()
      }
    };
  }
  if (text === "/permissions") {
    return {type: "command", command: {type: "permission.show"}};
  }
  if (text.startsWith("/permissions ")) {
    const raw = text.slice("/permissions ".length).trim();
    const mode = raw === "workspace-read" ? "workspace_read" : raw === "full-access" ? "full_access" : raw;
    if (mode === "confirm" || mode === "workspace_read" || mode === "full_access") {
      return {type: "command", command: {type: "permission.set", mode}};
    }
    return {type: "invalid", message: "用法：/permissions confirm|workspace-read|full-access"};
  }
  if (text === "/trace") {
    return {type: "command", command: {type: "trace.toggle"}};
  }
  if (text === "/exit" || text === "/quit") {
    return {type: "command", command: {type: "app.exit"}};
  }
  if (text.startsWith("/")) {
    return {type: "invalid", message: `未知命令：${text}`};
  }
  return {type: "command", command: options.agentDefaultRoute ? {type: "agent.submit", input: text} : {type: "turn.submit", input: text}};
}

export function classifyUiInput(
  state: Pick<UiState, "phase" | "inputMode" | "recoverableAgentTurnId" | "agentDefaultRoute">,
  value: string
): ChatInputAction {
  const text = value.trim();
  if (!text) {
    return {type: "empty"};
  }
  if (state.inputMode === "init_recovery") {
    if (text === "/retry") {
      return {type: "retry_init"};
    }
    if (text === "/exit") {
      return {type: "command", command: {type: "app.exit"}};
    }
    return {
      type: "invalid",
      message: "Runtime initialization failed. Use /retry or /exit."
    };
  }
  if (state.inputMode === "api_setup_required") {
    return text.startsWith("/")
      ? gateContinue(state, classifyChatInput(text))
      : {
          type: "invalid",
          message: "当前供应商需要 API key；请输入 /api 开始设置。"
        };
  }
  if (
    state.inputMode !== "chat" ||
    state.phase === "booting" ||
    state.phase === "stopped"
  ) {
    return {type: "invalid", message: "Runtime 尚未准备好接收聊天输入。"};
  }
  return gateContinue(state, classifyChatInput(text, {agentDefaultRoute: state.agentDefaultRoute}));
}

function gateContinue(state: Pick<UiState, "recoverableAgentTurnId">, action: ChatInputAction): ChatInputAction {
  return action.type === "command" && action.command.type === "agent.continue" && !state.recoverableAgentTurnId
    ? {type: "invalid", message: "当前没有可恢复的 Agent 检查点。"}
    : action;
}

export function classifyPlaintextConfirmation(
  value: string
): PlaintextConfirmationAction {
  return value.trim() === "YES"
    ? {
        type: "command",
        command: {type: "config.api_key.plaintext.confirm"}
      }
    : {type: "cancel"};
}
