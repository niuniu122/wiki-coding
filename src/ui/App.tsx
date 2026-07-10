import React, {useEffect, useMemo, useState} from "react";
import {Box, Text, useApp} from "ink";
import type {Command, RuntimeEvent} from "../protocol.js";
import {CommandDispatcher} from "../runtime/command-dispatcher.js";
import type {TraceEvent} from "../types.js";
import {classifyChatInput} from "./chat-input-policy.js";
import {
  formatCompactionStatus,
  formatHistoryMessages,
  formatThreadList,
  type DisplayMessage
} from "./format-runtime-event.js";
import {TextInput} from "./TextInput.js";

type ChatMessage = DisplayMessage;

const WELCOME_MESSAGE: ChatMessage = {
  id: "welcome",
  role: "system",
  content: "MiniMax Codex 已启动。输入问题开始对话，或输入 /api 更换 API。"
};

export function App(): React.ReactElement {
  const {exit} = useApp();
  const dispatcher = useMemo(() => new CommandDispatcher(), []);
  const [initialized, setInitialized] = useState(false);
  const [mode, setMode] = useState<"chat" | "api">("chat");
  const [input, setInput] = useState("");
  const [apiInput, setApiInput] = useState("");
  const [busy, setBusy] = useState(false);
  const [traceOpen, setTraceOpen] = useState(false);
  const [status, setStatus] = useState("启动中");
  const [messages, setMessages] = useState<ChatMessage[]>([WELCOME_MESSAGE]);
  const [traces, setTraces] = useState<TraceEvent[]>([]);
  const [tokenLine, setTokenLine] = useState("token: waiting");

  useEffect(() => {
    let mounted = true;

    async function boot(): Promise<void> {
      try {
        const events = await dispatcher.init();
        if (!mounted) {
          return;
        }
        for (const event of events) {
          applyRuntimeEvent(event);
        }
      } catch (error) {
        setStatus(error instanceof Error ? error.message : String(error));
      } finally {
        setInitialized(true);
      }
    }

    void boot();
    return () => {
      mounted = false;
    };
  }, [dispatcher]);

  async function dispatchCommand(command: Command): Promise<void> {
    const blocking = isBlockingCommand(command);
    if (blocking) {
      setBusy(true);
    }
    try {
      for await (const event of dispatcher.dispatch(command)) {
        applyRuntimeEvent(event);
      }
    } finally {
      if (blocking) {
        setBusy(false);
      }
    }
  }

  async function submitApiKey(value: string): Promise<void> {
    setApiInput("");
    await dispatchCommand({type: "config.api_key.set", apiKey: value});
  }

  async function submitChat(value: string): Promise<void> {
    const action = classifyChatInput(value, busy);
    if (action.type === "empty") {
      return;
    }
    setInput("");
    if (action.type === "busy") {
      setStatus("当前正在回复；输入 /interrupt 可以取消本轮请求");
      return;
    }
    if (action.type === "invalid") {
      setStatus(action.message);
      return;
    }
    await dispatchCommand(action.command);
  }

  function applyRuntimeEvent(event: RuntimeEvent): void {
    if (event.type === "runtime.ready") {
      const recoverySuffix =
        event.recoveredTurns > 0 ? `；已恢复 ${event.recoveredTurns} 个异常中断的 Turn` : "";
      setMode(event.hasApiKey ? "chat" : "api");
      setStatus(
        event.hasApiKey
          ? `就绪：${event.providerSummary}${recoverySuffix}`
          : `需要填写 API key：${event.providerSummary}${recoverySuffix}`
      );
      return;
    }

    if (event.type === "thread.loaded") {
      setTraces([]);
      setStatus(`会话已加载: ${event.thread.id}`);
      return;
    }

    if (event.type === "thread.listed") {
      setMessages((current) => [
        ...current,
        {
          id: `threads-${Date.now()}`,
          role: "system",
          content: formatThreadList(event.threads)
        }
      ]);
      setStatus(`已列出 ${event.threads.length} 个历史会话`);
      return;
    }

    if (event.type === "history.loaded") {
      setMessages([WELCOME_MESSAGE, ...formatHistoryMessages(event.items)]);
      return;
    }

    if (event.type === "turn.recovered") {
      setStatus(`已恢复异常中断的 Turn: ${event.turn.id}`);
      return;
    }

    if (event.type === "turn.started") {
      setMessages((current) => [
        ...current,
        {id: `user-${event.turnId}`, role: "user", content: event.input},
        {id: `assistant-${event.turnId}`, role: "assistant", content: ""}
      ]);
      return;
    }

    if (event.type === "turn.interrupt.requested") {
      setStatus(`正在取消 Turn: ${event.turnId}`);
      return;
    }

    if (event.type === "turn.interrupt.ignored") {
      setStatus("当前没有正在进行的模型请求");
      return;
    }

    if (event.type === "turn.interrupted") {
      setMessages((current) =>
        current.map((message) =>
          message.id === `assistant-${event.turnId}`
            ? {
                ...message,
                content: message.content
                  ? `${message.content}\n[已取消]`
                  : "[已取消，模型尚未返回内容]"
              }
            : message
        )
      );
      setStatus(`已取消 Turn: ${event.turnId}`);
      return;
    }

    if (event.type === "assistant.delta") {
      setMessages((current) =>
        current.map((message) =>
          message.id === `assistant-${event.turnId}`
            ? {...message, content: `${message.content}${event.delta}`}
            : message
        )
      );
      return;
    }

    if (event.type === "assistant.completed") {
      const turnId = event.item.turnId;
      if (turnId) {
        setMessages((current) =>
          current.map((message) =>
            message.id === `assistant-${turnId}`
              ? {...message, content: event.item.content}
              : message
          )
        );
      }
      return;
    }

    if (event.type === "trace.event") {
      setTraces((current) => [...current, event.event].slice(-12));
      return;
    }

    if (event.type === "token.usage") {
      setTokenLine(`token: ${event.used}/${event.limit} | auto compact: ${event.autoCompactAt}`);
      return;
    }

    if (event.type === "compact.started") {
      setStatus(`正在压缩上下文: ${event.reason}`);
      return;
    }

    if (event.type === "compact.completed") {
      setStatus(formatCompactionStatus(event));
      return;
    }

    if (event.type === "api.status") {
      setStatus(`API: ${event.status}`);
      return;
    }

    if (event.type === "config.api_key.requested") {
      setMode("api");
      setStatus(`请输入当前供应商的 API key：${event.providerSummary}`);
      return;
    }

    if (event.type === "config.api_key.saved") {
      setMode("chat");
      setStatus(
        event.location === "keychain"
          ? "API key 已保存到系统钥匙串。现在可以发一句测试。"
          : "API key 已保存到用户配置目录，不会写入当前项目。现在可以发一句测试。"
      );
      return;
    }

    if (event.type === "provider.listed") {
      setMessages((current) => [
        ...current,
        {
          id: `provider-${Date.now()}`,
          role: "system",
          content: [`当前供应商：${event.current}`, "可用供应商：", ...event.providers].join("\n")
        }
      ]);
      setStatus("已显示供应商列表");
      return;
    }

    if (event.type === "provider.changed") {
      setStatus(
        event.hasApiKey
          ? `已切换：${event.summary}`
          : `已切换：${event.summary}。请用 /api 设置这个供应商的 key。`
      );
      return;
    }

    if (event.type === "trace.toggle.requested") {
      setTraceOpen((open) => !open);
      setStatus("已切换工作追溯面板");
      return;
    }

    if (event.type === "app.exit.requested") {
      exit();
      return;
    }

    if (event.type === "error") {
      setStatus(`错误: ${event.message}`);
      setMessages((current) => {
        const withFailedTurn = event.turnId
          ? current.map((message) =>
              message.id === `assistant-${event.turnId}`
                ? {
                    ...message,
                    content: message.content
                      ? `${message.content}\n[发生错误]`
                      : "[请求失败，模型没有返回内容]"
                  }
                : message
            )
          : current;
        return [
          ...withFailedTurn,
          {id: `error-${event.turnId ?? Date.now()}`, role: "system", content: event.message}
        ];
      });
      return;
    }
  }

  const visibleMessages = messages.slice(-10);

  return (
    <Box flexDirection="column" paddingX={1}>
      <Header status={status} tokenLine={tokenLine} />
      <Box borderStyle="single" borderColor="green" flexDirection="column" paddingX={1} minHeight={12}>
        {visibleMessages.map((message) => (
          <MessageLine key={message.id} message={message} />
        ))}
        {busy && <Text color="gray">MiniMax 正在回复...</Text>}
      </Box>

      <Box marginTop={1} flexDirection="column">
        <Text color="cyan">
          {traceOpen ? "Trace open" : "Trace folded"} | /new 新会话 | /threads 历史 | /resume 切换 | /trace 追溯 | /compact 压缩 | /interrupt 取消 | /exit 退出
        </Text>
        {traceOpen && (
          <Box borderStyle="single" borderColor="yellow" flexDirection="column" paddingX={1}>
            {traces.length === 0 ? (
              <Text color="gray">暂无工作追溯。</Text>
            ) : (
              traces.map((trace) => (
                <Text key={trace.id} color="yellow">
                  {trace.code}: {trace.message}
                  {trace.facts ? ` ${JSON.stringify(trace.facts)}` : ""}
                </Text>
              ))
            )}
          </Box>
        )}
      </Box>

      <Box marginTop={1} flexDirection="column">
        {mode === "api" ? (
          <>
            <Text color="green">填写当前供应商 API key。输入内容会被遮住，不写入聊天和 trace。</Text>
            <TextInput
              value={apiInput}
              onChange={setApiInput}
              onSubmit={(value) => void submitApiKey(value)}
              placeholder={initialized ? "paste API key" : "loading"}
              mask
              disabled={!initialized || busy}
            />
          </>
        ) : (
          <TextInput
            value={input}
            onChange={setInput}
            onSubmit={(value) => void submitChat(value)}
            placeholder={busy ? "输入 /interrupt 取消当前回复" : "输入消息"}
            disabled={!initialized}
          />
        )}
      </Box>
    </Box>
  );
}

function isBlockingCommand(command: Command): boolean {
  return ![
    "thread.list",
    "turn.interrupt",
    "config.api_key.request",
    "provider.list",
    "trace.toggle",
    "app.exit"
  ].includes(command.type);
}

function Header({status, tokenLine}: {status: string; tokenLine: string}): React.ReactElement {
  return (
    <Box justifyContent="space-between">
      <Text color="green">MiniMax Codex</Text>
      <Text color="gray">更换 API: /api | provider: /provider | {tokenLine} | {status}</Text>
    </Box>
  );
}

function MessageLine({message}: {message: ChatMessage}): React.ReactElement {
  const color = message.role === "user" ? "cyan" : message.role === "assistant" ? "white" : "gray";
  const label = message.role === "user" ? "you" : message.role === "assistant" ? "minimax" : "system";
  return (
    <Box flexDirection="column" marginBottom={1}>
      <Text color={color}>{label}</Text>
      <Text>{message.content || "..."}</Text>
    </Box>
  );
}
