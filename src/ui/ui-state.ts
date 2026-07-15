import type {RuntimeEvent} from "../protocol.js";
import type {TraceEvent} from "../types.js";
import {
  formatCompactionStatus,
  formatHistoryMessages,
  formatModelCatalog,
  formatThreadList,
  type DisplayMessage
} from "./format-runtime-event.js";

export type UiPhase = "booting" | "init_failed" | "idle" | "running" | "stopped";

export type UiInputMode =
  | "disabled"
  | "init_recovery"
  | "chat"
  | "api_setup_required"
  | "confirming_plaintext"
  | "entering_api_key";

export interface UiState {
  phase: UiPhase;
  inputMode: UiInputMode;
  messages: DisplayMessage[];
  traces: TraceEvent[];
  status: string;
  tokenLine: string;
  traceOpen: boolean;
  recoverableAgentTurnId: string | null;
  agentDefaultRoute: boolean;
}

export type UiEvent =
  | RuntimeEvent
  | {type: "ui.input.invalid"; message: string}
  | {type: "ui.init.retrying"}
  | {type: "ui.plaintext.cancelled"};

export const WELCOME_MESSAGE: DisplayMessage = {
  id: "welcome",
  role: "system",
  content: "MiniMax Codex 已启动。输入问题开始对话，或输入 /api 更换 API。"
};

export function initialUiState(): UiState {
  return {
    phase: "booting",
    inputMode: "disabled",
    messages: [WELCOME_MESSAGE],
    traces: [],
    status: "启动中",
    tokenLine: "token: waiting",
    traceOpen: false,
    recoverableAgentTurnId: null,
    agentDefaultRoute: false
  };
}

export function reduceRuntimeEvent(state: UiState, event: UiEvent): UiState {
  if (
    state.phase === "stopped" &&
    (event.type === "runtime.ready" || event.type === "runtime.init_failed")
  ) {
    return state;
  }
  switch (event.type) {
    case "runtime.ready": {
      const recoverySuffix =
        event.recoveredTurns > 0
          ? `；已恢复 ${event.recoveredTurns} 个异常中断的 Turn`
          : "";
      return {
        ...state,
        phase: "idle",
        agentDefaultRoute: event.features?.agentDefaultRoute ?? false,
        inputMode: event.hasApiKey ? "chat" : "api_setup_required",
        messages: state.messages.filter(
          (message) => message.id !== "runtime-init-failed"
        ),
        status: event.hasApiKey
          ? `就绪：${event.providerSummary}${recoverySuffix}`
          : `需要 API key：${event.providerSummary}${recoverySuffix}。输入 /api 开始设置。`
      };
    }
    case "runtime.init_failed":
      return {
        ...state,
        phase: "init_failed",
        inputMode: "init_recovery",
        status: `Runtime initialization failed: ${event.message}`,
        messages: [
          ...state.messages.filter((message) => message.id !== "runtime-init-failed"),
          {id: "runtime-init-failed", role: "system", content: event.message}
        ]
      };
    case "thread.loaded":
      return {
        ...state,
        traces: [],
        recoverableAgentTurnId: null,
        status: `会话已加载: ${event.thread.id}`
      };
    case "thread.listed":
      return {
        ...state,
        messages: [
          ...state.messages,
          {
            id: nextMessageId(state.messages, "threads"),
            role: "system",
            content: formatThreadList(event.threads)
          }
        ],
        status: `已列出 ${event.threads.length} 个历史会话`
      };
    case "history.loaded":
      return {
        ...state,
        messages: [WELCOME_MESSAGE, ...formatHistoryMessages(event.items)]
      };
    case "turn.started":
      return {
        ...state,
        phase: phaseAfterBoot(state, "running"),
        messages: startTurnMessages(state.messages, event.turnId, event.input)
      };
    case "turn.recovered":
      return {
        ...state,
        status: `已恢复异常中断的 Turn: ${event.turn.id}`
      };
    case "turn.interrupt.requested":
      return {...state, status: `正在取消 Turn: ${event.turnId}`};
    case "turn.interrupt.ignored":
      return {...state, status: "当前没有正在进行的模型请求"};
    case "turn.interrupted":
      return {
        ...state,
        phase: phaseAfterBoot(state, "idle"),
        status: `已取消 Turn: ${event.turnId}`,
        messages: state.messages.map((message) =>
          message.id === `assistant-${event.turnId}`
            ? {
                ...message,
                content: message.content
                  ? `${message.content}\n[已取消]`
                  : "[已取消，模型尚未返回内容]"
              }
            : message
        )
      };
    case "agent.started":
      return {
        ...state,
        phase: phaseAfterBoot(state, "running"),
        messages: startTurnMessages(state.messages, event.turnId, event.input),
        status: "Agent 已开始本机能力检索"
      };
    case "agent.retrieval.started":
      return appendAgentStatus(state, `正在检索本机能力：${event.query}`, "agent-retrieval");
    case "agent.retrieval.completed":
      return appendAgentStatus(state, `已召回 ${event.candidates.length} 个本机能力候选（${event.path}）：${event.candidates.join(", ")}`, "agent-candidates");
    case "agent.model.started":
      return {...state, status: `Agent 正在规划第 ${event.step} 步`};
    case "agent.assistant.delta":
      return {
        ...state,
        messages: state.messages.map((message) =>
          message.id === `assistant-${event.turnId}`
            ? {...message, content: `${message.content}${event.delta}`}
            : message
        )
      };
    case "agent.tool.requested":
      return appendAgentStatus(state, `请求本机能力：${event.capabilityId}`, "agent-tool-request");
    case "agent.tool.completed":
      return appendAgentStatus(state, `本机能力结果：${event.status}`, "agent-tool-result");
    case "agent.permission.required":
      return {...appendAgentStatus(state, `需要确认后才能执行：${event.capabilityId}。调整当前 Session 权限后可使用 /continue。`, "agent-permission"), phase: phaseAfterBoot(state, "idle"), recoverableAgentTurnId: event.turnId};
    case "agent.completed":
      return {
        ...state,
        phase: phaseAfterBoot(state, "idle"),
        recoverableAgentTurnId: null,
        messages: state.messages.map((message) =>
          message.id === `assistant-${event.turnId}` ? {...message, content: event.item.content} : message
        ),
        status: "Agent 已完成"
      };
    case "agent.stopped":
      return {...state, phase: phaseAfterBoot(state, "idle"), status: `Agent 已停止：${event.reason}`};
    case "agent.recovery.available":
      return {...appendAgentStatus(state, `发现可恢复的 Agent 检查点：${event.checkpointId}；可使用 /continue`, "agent-checkpoint"), recoverableAgentTurnId: event.turnId};
    case "agent.recovery.blocked":
      return {...appendAgentStatus(state, `Agent 恢复已阻止：${event.reason}`, "agent-recovery-blocked"), recoverableAgentTurnId: null};
    case "agent.continued": {
      const continued = appendAgentStatus(state, `从检查点继续：${event.checkpointId}`, "agent-continued");
      return {...continued, phase: phaseAfterBoot(state, "running"), recoverableAgentTurnId: null, messages: ensureAssistantMessage(continued.messages, event.turnId)};
    }
    case "assistant.delta":
      return {
        ...state,
        messages: state.messages.map((message) =>
          message.id === `assistant-${event.turnId}`
            ? {...message, content: `${message.content}${event.delta}`}
            : message
        )
      };
    case "assistant.completed":
      return {
        ...state,
        phase: phaseAfterBoot(state, "idle"),
        messages: event.item.turnId
          ? state.messages.map((message) =>
              message.id === `assistant-${event.item.turnId}`
                ? {...message, content: event.item.content}
                : message
            )
          : state.messages
      };
    case "trace.event":
      return {...state, traces: [...state.traces, event.event].slice(-12)};
    case "token.usage":
      return {
        ...state,
        tokenLine: `token: ${event.used}/${event.limit} | auto compact: ${event.autoCompactAt}`
      };
    case "compact.started":
      return {...state, status: `正在压缩上下文: ${event.reason}`};
    case "compact.completed":
      return {...state, status: formatCompactionStatus(event)};
    case "api.status":
      return {...state, status: `API: ${event.status}`};
    case "config.api_key.requested":
      return {
        ...state,
        inputMode: "entering_api_key",
        status: `请输入当前供应商的 API key：${event.providerSummary}`
      };
    case "config.legacy_credential.reentry_required":
      return {
        ...state,
        messages: [
          ...state.messages,
          {
            id: nextMessageId(state.messages, "legacy-credential"),
            role: "system",
            content: [
              "Legacy workspace credential was not imported because the OS keyring is unavailable.",
              `The original file remains at ${event.path}.`,
              "Use /api to save a new scoped credential; the legacy file will be removed only after that save succeeds."
            ].join(" ")
          }
        ],
        status: event.hasUsableCredential
          ? "Legacy credential migration is pending; the current environment credential remains usable."
          : "Legacy credential migration is pending. Use /api to configure a scoped credential."
      };
    case "config.api_key.plaintext_confirmation_required":
      return {
        ...state,
        inputMode: "confirming_plaintext",
        status: [
          "明文（plaintext）凭据警告：API key 将保存到",
          event.path,
          "有该操作系统账户访问权限的人都可以读取。输入 YES 继续；其他输入取消。"
        ].join(" ")
      };
    case "config.api_key.plaintext_confirmed":
      return {
        ...state,
        inputMode: "entering_api_key",
        status: `已确认明文保存。请输入当前供应商的 API key：${event.providerSummary}`
      };
    case "config.api_key.saved":
      return {
        ...state,
        phase: phaseAfterBoot(state, "idle"),
        inputMode: "chat",
        status:
          event.location === "user-file"
            ? "API key 已保存到用户配置目录，不会写入当前项目。现在可以发一句测试。"
            : "API key 已保存到系统钥匙串。现在可以发一句测试。"
      };
    case "provider.listed":
      return {
        ...state,
        messages: [
          ...state.messages,
          {
            id: nextMessageId(state.messages, "provider"),
            role: "system",
            content: [
              `当前供应商：${event.current}`,
              "可用供应商：",
              ...event.providers
            ].join("\n")
          }
        ],
        status: "已显示供应商列表"
      };
    case "provider.changed":
      return {
        ...state,
        inputMode: event.hasApiKey ? "chat" : "api_setup_required",
        status: event.hasApiKey
          ? `已切换：${event.summary}`
          : `已切换：${event.summary}。请用 /api 设置这个供应商的 key。`
      };
    case "model.listed":
      return {
        ...state,
        messages: [
          ...state.messages,
          {
            id: nextMessageId(state.messages, "models"),
            role: "system",
            content: [
              `当前模型：${event.current.modelDisplayName} (${event.current.modelProfileId})`,
              "已注册模型：",
              formatModelCatalog(event.models)
            ].join("\n")
          }
        ],
        status: "已显示模型列表"
      };
    case "model.changed":
      return {
        ...state,
        inputMode: "chat",
        status: `已切换模型：${event.selection.modelDisplayName} | Provider: ${event.selection.providerDisplayName}`
      };
    case "model.change_failed":
      return {
        ...state,
        status: modelSelectionFailureMessage(
          event.code,
          event.configuredDefaultModelProfileId
        )
      };
    case "capability.listed":
      return {
        ...state,
        messages: [...state.messages, {
          id: nextMessageId(state.messages, "capabilities"),
          role: "system",
          content: [
            `能力索引：${event.snapshotVersion} | ${event.health} | ${event.mode}`,
            ...event.capabilities.map((item) =>
              `${item.id} | ${item.status} | ${item.safetyClass} | ${item.source}${item.shadowedBy ? ` | shadowed by ${item.shadowedBy}` : ""}`
            )
          ].join("\n")
        }],
        status: `已列出 ${event.capabilities.length} 个本机能力定义（仅报告）`
      };
    case "capability.searched":
      return {
        ...state,
        messages: [...state.messages, {
          id: nextMessageId(state.messages, "capability-search"),
          role: "system",
          content: [
            `本机能力检索：${event.query} | ${event.mode}${event.fallbackReason ? ` | fallback=${event.fallbackReason}` : ""}`,
            ...event.candidates.map((item) => `${item.id} | ${item.matchPath} | ${item.status}`)
          ].join("\n")
        }],
        status: `找到 ${event.candidates.length} 个候选；未执行任何能力`
      };
    case "capability.unavailable":
      return {...state, status: "本机能力目录当前未启用；聊天不受影响。"};
    case "permission.current":
      return {...state, status: `当前 Session 权限：${event.mode}`};
    case "permission.changed":
      return {...state, status: `当前 Session 权限已改为：${event.mode}；模型选择不受影响。`};
    case "trace.toggle.requested":
      return {
        ...state,
        traceOpen: !state.traceOpen,
        status: "已切换工作追溯面板"
      };
    case "app.exit.requested":
      return {
        ...state,
        phase: "stopped",
        inputMode: "disabled"
      };
    case "command.rejected":
      return {...state, status: event.message};
    case "error": {
      const messages = event.turnId
        ? state.messages.map((message) =>
            message.id === `assistant-${event.turnId}`
              ? {
                  ...message,
                  content: message.content
                    ? `${message.content}\n[发生错误]`
                    : "[请求失败，模型没有返回内容]"
                }
              : message
          )
        : state.messages;
      return {
        ...state,
        phase: event.turnId ? phaseAfterBoot(state, "idle") : state.phase,
        status: `错误: ${event.message}`,
        messages: [
          ...messages,
          {
            id: nextMessageId(messages, "error"),
            role: "system",
            content: event.message
          }
        ]
      };
    }
    case "ui.plaintext.cancelled":
      return state.inputMode === "confirming_plaintext"
        ? {
            ...state,
            phase: phaseAfterBoot(state, "idle"),
            inputMode: "api_setup_required",
            status: "已取消（cancelled）明文保存；未保存任何 API key。"
          }
        : state;
    case "ui.init.retrying":
      return {
        ...state,
        phase: "booting",
        inputMode: "disabled",
        status: "Retrying Runtime initialization..."
      };
    case "ui.input.invalid":
      return {...state, status: event.message};
    default:
      return assertNever(event);
  }
}

function phaseAfterBoot(state: UiState, next: UiPhase): UiPhase {
  if (
    state.phase === "booting" ||
    state.phase === "init_failed" ||
    state.phase === "stopped"
  ) {
    return state.phase;
  }
  return next;
}

function appendAgentStatus(state: UiState, content: string, prefix: string): UiState {
  return {
    ...state,
    status: content,
    messages: [...state.messages, {id: nextMessageId(state.messages, prefix), role: "system", content: content.slice(0, 4_000)}]
  };
}

function ensureAssistantMessage(messages: DisplayMessage[], turnId: string): DisplayMessage[] {
  return messages.some((message) => message.id === `assistant-${turnId}`)
    ? messages
    : [...messages, {id: `assistant-${turnId}`, role: "assistant", content: ""}];
}

function startTurnMessages(
  messages: DisplayMessage[],
  turnId: string,
  input: string
): DisplayMessage[] {
  const userId = `user-${turnId}`;
  const assistantId = `assistant-${turnId}`;
  const withoutTurn = messages.filter(
    (message) => message.id !== userId && message.id !== assistantId
  );
  return [
    ...withoutTurn,
    {id: userId, role: "user", content: input},
    {id: assistantId, role: "assistant", content: ""}
  ];
}

function nextMessageId(messages: DisplayMessage[], prefix: string): string {
  let suffix = 1;
  while (messages.some((message) => message.id === `${prefix}-${suffix}`)) {
    suffix += 1;
  }
  return `${prefix}-${suffix}`;
}

function modelSelectionFailureMessage(
  code: Extract<RuntimeEvent, {type: "model.change_failed"}>["code"],
  configuredDefaultModelProfileId?: string
): string {
  const messages: Record<typeof code, string> = {
    not_initialized: "模型选择器尚未初始化。",
    model_unavailable: "该模型不存在或当前不可用。",
    credential_unavailable: "该模型没有已经配置好的凭证。",
    turn_active: "当前回复仍在运行，请在两个 Turn 之间切换模型。",
    sticky_selection_forbidden: "工作区兼容模型需要先提升为用户级 Profile。",
    agent_feature_unsupported: "该模型不支持 Agent 所需的原生工具调用。",
    recovery_required: "模型指针需要显式恢复。",
    selection_failed: "模型切换失败，原模型保持不变。"
  };
  const fallback = configuredDefaultModelProfileId
    ? ` 可恢复的配置默认值：${configuredDefaultModelProfileId}`
    : "";
  return `${messages[code]}${fallback}`;
}

function assertNever(value: never): never {
  throw new Error(`Unhandled UI event: ${JSON.stringify(value)}`);
}
