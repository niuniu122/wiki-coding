# MiniMax CLI Command/Event Dispatcher 重构计划

**日期：** 2026-07-10
**状态：** 已实施并验证
**范围：** 稳定化第三阶段——让 Ink UI 不再直接调用 AgentRuntime 的工作流方法，而是只提交 Command、消费 RuntimeEvent。

## 当前问题

`App.tsx` 同时承担命令识别、业务流程、异常处理、Runtime 调用和界面更新。结果是：

- `Command` 类型只是声明，没有真实 dispatcher；
- 同一个命令的状态和错误处理散落在多个 UI 分支；
- RuntimeEvent 缺少 `turnId` 等关联信息，UI 必须靠局部闭包猜测事件属于哪个回复；
- 无法在没有 Ink 的情况下完整测试 CLI 命令行为；
- 未来增加工具、审批或 MCP 时，业务流程会继续堆进 React 组件。

## 目标边界

```text
用户文本
→ classifyChatInput（纯解析）
→ Command
→ CommandDispatcher（统一路由与错误边界）
→ AgentRuntime / config / provider
→ RuntimeEvent
→ Ink App（只更新视图）
```

Ink 可以继续拥有纯界面状态，例如输入框内容、busy、API key 输入模式、trace 是否展开和退出应用；它不再直接调用 `newThread`、`resumeThread`、`compact`、`submitUserInput`、provider 或 secret 方法。

## Command 契约

```ts
type Command =
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
```

API key 只存在于入站 Command 内存中，不能进入任何 RuntimeEvent、trace 或聊天记录。

## 新增/调整 RuntimeEvent

- `runtime.ready`：启动后携带 provider 摘要和是否已有 key。
- `thread.listed`：携带 thread records。
- `turn.started`：增加原始 input。
- `assistant.delta`：增加 turnId。
- `config.api_key.requested` / `config.api_key.saved`。
- `provider.listed` / `provider.changed`。
- `trace.toggle.requested`。
- `app.exit.requested`。

事件必须自描述，UI 不应依赖“当前正在执行哪个函数”的闭包才能关联消息。

## Dispatcher 规则

`CommandDispatcher` 包装一个 `RuntimePort`：

1. `init()` 初始化 Runtime，并追加 `runtime.ready`。
2. `dispatch(command)` 是统一 AsyncGenerator。
3. 数组结果逐条转发；流式 Turn 直接 `yield`。
4. 所有同步/异步异常统一转换为 `{type:"error", message}`。
5. dispatcher 不保存聊天 UI 状态，不打印、不退出进程。

## App 迁移

- `submitChat` 只处理 empty/busy，其他输入均得到 Command 并交给 dispatcher。
- `submitApiKey` 只构造 `config.api_key.set` Command。
- `applyRuntimeEvent` 负责所有显示：thread 列表、provider 列表、模式切换、turn 消息、增量、完成、中断、退出。
- 以 `assistant-${turnId}` 作为稳定 UI 消息 ID，去除 Date.now 闭包关联。
- busy 仍是视图交互策略，但不决定业务状态。

## 测试

1. parser 覆盖所有斜杠命令、参数错误、busy 和普通消息。
2. Fake RuntimePort 验证每种 Command 路由到唯一方法。
3. dispatcher 将异常统一变成 error event。
4. API key 不出现在事件中。
5. turn.started、assistant.delta 和 turn.interrupted 使用相同 turnId。
6. App 不再 import 或实例化 AgentRuntime，不再直接调用 Runtime 工作流方法。
7. 现有压缩、恢复、中断、多会话测试全部继续通过。

## 非目标

- 本阶段不增加文件、Shell 或 Git 工具。
- 不实现跨进程 RPC/app-server。
- 不拆分 Provider transport；属于第四阶段。
- 不改变密钥持久化位置；属于第五阶段。

## 实施结果

- Command 契约覆盖 thread、turn、compact、API key、provider、trace 和 app exit。
- `classifyChatInput` 现在只返回 Command、empty、busy 或 validation error。
- 新增 `CommandDispatcher` 和可替换的 `RuntimePort`，统一处理初始化、路由、流式转发和异常事件化。
- Dispatcher 的 set-key 成功和失败路径都不会在事件中泄漏 API key；异常回显中的原文会替换为 `[REDACTED]`。
- `turn.started` 携带 input，`assistant.delta` 携带 turnId；Ink 使用稳定的 `assistant-<turnId>` 关联消息。
- App 不再 import、实例化或直接调用 AgentRuntime，只依赖 dispatcher 并渲染 RuntimeEvent。
- 新增静态架构测试，防止 Runtime 工作流调用重新进入 App。
- 验证：38 个测试通过，类型检查和构建通过；dispatcher、parser 和 UI boundary 专项探针通过。
