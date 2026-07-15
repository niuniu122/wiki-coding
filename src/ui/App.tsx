import React, {useCallback, useEffect, useMemo, useReducer, useRef, useState} from "react";
import {Box, Text, useApp} from "ink";
import type {Command} from "../protocol.js";
import {CommandDispatcher} from "../runtime/command-dispatcher.js";
import {createInitializationFailureEvent} from "../runtime/command-dispatcher.js";
import type {RuntimeApplication} from "../runtime/runtime-application.js";
import {
  classifyPlaintextConfirmation,
  classifyUiInput
} from "./chat-input-policy.js";
import type {DisplayMessage} from "./format-runtime-event.js";
import {TextInput} from "./TextInput.js";
import {
  initialUiState,
  reduceRuntimeEvent
} from "./ui-state.js";

export interface AppProps {
  dispatcher?: RuntimeApplication;
  createDispatcher?(): RuntimeApplication;
}

export function App({
  dispatcher: suppliedDispatcher,
  createDispatcher
}: AppProps = {}): React.ReactElement {
  const {exit} = useApp();
  const ownsDispatcher = suppliedDispatcher === undefined;
  const dispatcher = useMemo(
    () => suppliedDispatcher ?? createDispatcher?.() ?? new CommandDispatcher(),
    [createDispatcher, suppliedDispatcher]
  );
  const [state, dispatchEvent] = useReducer(
    reduceRuntimeEvent,
    undefined,
    initialUiState
  );
  const [input, setInput] = useState("");
  const [apiInput, setApiInput] = useState("");
  const [confirmationInput, setConfirmationInput] = useState("");
  const mountedRef = useRef(false);
  const stoppedRef = useRef(false);
  const initFlightRef = useRef<Promise<void> | null>(null);

  const initialize = useCallback((retry: boolean): Promise<void> => {
    if (initFlightRef.current) {
      return initFlightRef.current;
    }
    if (retry) {
      dispatchEvent({type: "ui.init.retrying"});
    }
    const operation = (async () => {
      let events;
      try {
        events = await dispatcher.init();
      } catch (error) {
        events = [createInitializationFailureEvent(error)];
      }
      if (!mountedRef.current || stoppedRef.current) {
        return;
      }
      for (const event of events) {
        dispatchEvent(event);
      }
    })();
    initFlightRef.current = operation;
    void operation
      .finally(() => {
        if (initFlightRef.current === operation) {
          initFlightRef.current = null;
        }
      })
      .catch(() => undefined);
    return operation;
  }, [dispatcher]);

  useEffect(() => {
    mountedRef.current = true;
    stoppedRef.current = false;
    void initialize(false);

    return () => {
      mountedRef.current = false;
      stoppedRef.current = true;
      if (ownsDispatcher) {
        void dispatcher.shutdown("user").catch(() => undefined);
      }
    };
  }, [dispatcher, initialize, ownsDispatcher]);

  useEffect(() => {
    if (state.phase === "stopped") {
      stoppedRef.current = true;
      exit();
    }
  }, [exit, state.phase]);

  async function dispatchCommand(command: Command): Promise<void> {
    try {
      for await (const event of dispatcher.dispatch(command)) {
        dispatchEvent(event);
      }
    } catch (error) {
      dispatchEvent({
        type: "error",
        message: error instanceof Error ? error.message : String(error)
      });
    }
  }

  async function submitApiKey(value: string): Promise<void> {
    setApiInput("");
    await dispatchCommand({type: "config.api_key.set", apiKey: value});
  }

  async function submitPlaintextConfirmation(value: string): Promise<void> {
    setConfirmationInput("");
    const action = classifyPlaintextConfirmation(value);
    if (action.type === "cancel") {
      dispatchEvent({type: "ui.plaintext.cancelled"});
      return;
    }
    await dispatchCommand(action.command);
  }

  async function submitChat(value: string): Promise<void> {
    const action = classifyUiInput(state, value);
    if (action.type === "empty") {
      return;
    }
    setInput("");
    if (action.type === "invalid") {
      dispatchEvent({type: "ui.input.invalid", message: action.message});
      return;
    }
    if (action.type === "retry_init") {
      await initialize(true);
      return;
    }
    await dispatchCommand(action.command);
  }

  const visibleMessages = state.messages.slice(-10);
  const busy = state.phase === "running";
  const inactive =
    state.inputMode !== "chat" ||
    state.phase === "booting" ||
    state.phase === "stopped";
  const credentialInputDisabled = state.phase === "stopped";

  return (
    <Box flexDirection="column" paddingX={1}>
      <Header status={state.status} tokenLine={state.tokenLine} />
      <Box borderStyle="single" borderColor="green" flexDirection="column" paddingX={1} minHeight={12}>
        {visibleMessages.map((message) => (
          <MessageLine key={message.id} message={message} />
        ))}
        {busy && <Text color="gray">当前模型正在执行聊天或 Agent 步骤...</Text>}
      </Box>

      <Box marginTop={1} flexDirection="column">
        <Text color="cyan">
          {state.traceOpen ? "Trace open" : "Trace folded"} | /agent 本机 Agent | /chat 强制聊天 | /continue 恢复 | /permissions 权限 | /new 新会话 | /interrupt 取消 | /exit 退出
        </Text>
        {state.traceOpen && (
          <Box borderStyle="single" borderColor="yellow" flexDirection="column" paddingX={1}>
            {state.traces.length === 0 ? (
              <Text color="gray">暂无工作追溯。</Text>
            ) : (
              state.traces.map((trace) => (
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
        {state.inputMode === "init_recovery" ? (
          <>
            <Text color="red">Runtime initialization failed. Use /retry or /exit.</Text>
            <TextInput
              value={input}
              onChange={setInput}
              onSubmit={(value) => void submitChat(value)}
              placeholder="/retry or /exit"
              disabled={state.phase === "stopped"}
            />
          </>
        ) : state.inputMode === "confirming_plaintext" ? (
          <>
            <Text color="yellow">
              输入 YES 允许在上方绝对路径中明文保存 API key；输入任何其他内容取消。
            </Text>
            <TextInput
              value={confirmationInput}
              onChange={setConfirmationInput}
              onSubmit={(value) => void submitPlaintextConfirmation(value)}
              placeholder="type YES to continue"
              disabled={credentialInputDisabled}
            />
          </>
        ) : state.inputMode === "entering_api_key" ? (
          <>
            <Text color="green">填写当前供应商 API key。输入内容会被遮住，不写入聊天和 trace。</Text>
            <TextInput
              value={apiInput}
              onChange={setApiInput}
              onSubmit={(value) => void submitApiKey(value)}
              placeholder="paste API key"
              mask
              disabled={credentialInputDisabled}
            />
          </>
        ) : state.inputMode === "api_setup_required" ? (
          <>
            <Text color="yellow">当前供应商还没有 API key。只能输入 /api 开始凭据设置。</Text>
            <TextInput
              value={input}
              onChange={setInput}
              onSubmit={(value) => void submitChat(value)}
              placeholder="输入 /api"
              disabled={credentialInputDisabled}
            />
          </>
        ) : (
          <TextInput
            value={input}
            onChange={setInput}
            onSubmit={(value) => void submitChat(value)}
            placeholder={
              busy
                ? "输入 /interrupt 取消当前回复"
                : inactive
                  ? "loading"
                  : "输入消息"
            }
            disabled={inactive}
          />
        )}
      </Box>
    </Box>
  );
}

function Header({status, tokenLine}: {status: string; tokenLine: string}): React.ReactElement {
  return (
    <Box justifyContent="space-between">
      <Text color="green">MiniMax Codex</Text>
      <Text color="gray">API: /api | models: /models | agent: /agent | local tools: /capabilities | {tokenLine} | {status}</Text>
    </Box>
  );
}

function MessageLine({message}: {message: DisplayMessage}): React.ReactElement {
  const color = message.role === "user" ? "cyan" : message.role === "assistant" ? "white" : "gray";
  const label = message.role === "user" ? "you" : message.role === "assistant" ? "minimax" : "system";
  return (
    <Box flexDirection="column" marginBottom={1}>
      <Text color={color}>{label}</Text>
      <Text>{message.content || "..."}</Text>
    </Box>
  );
}
