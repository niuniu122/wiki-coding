# MiniMax Codex CLI 稳定化设计

**日期：** 2026-07-10
**状态：** 用户已确认；第一阶段已实施并验证
**实施方式：** 小步修复、边做边教、每步可运行验证

## 1. 背景

当前 CLI 已经具备 Ink 交互界面、MiniMax/OpenAI-compatible 流式调用、JSONL 会话保存、trace 展示和 `/compact` 命令。它是一个可运行的聊天壳，但已有功能中存在可验证的问题：压缩不真正减少模型上下文、Turn 状态不能可靠恢复、UI 绕过协议直接控制 Runtime、Provider 适配器职责过多、原始 reasoning 与工作追溯混在一起、密钥和本地状态边界不安全、缺少行为测试。

本设计先修已有功能，不增加 `read_file`、文件修改、Shell、MCP、多 Agent 或插件系统。

## 2. 目标

1. 让当前聊天、保存、压缩、恢复、配置和 trace 行为可信赖。
2. 每次只修一个可观察问题，并在修改前后运行验证。
3. 使用 Codex 的分层原则约束边界，参考 claw-code 的运行时和权限经验，但保留 Node.js、TypeScript、Ink 和 MiniMax Provider 体系。
4. 为后续工具循环打地基，但本阶段不实现工具。

## 3. 非目标

- 不复刻 Codex 或 claw-code 的完整功能。
- 不进行一次性大重写。
- 不把 SQLite 占位接口扩展成完整数据库。
- 不显示或持久化模型私密原始推理作为产品能力。
- 不依赖真实 API 才能运行核心回归测试。

## 4. 修复顺序

### 阶段一：真实上下文压缩与测试地基

修复 `/compact` 和自动压缩，使摘要真正替代已覆盖的旧上下文；引入不依赖真实 API 的自动测试。

### 阶段二：会话恢复与中断

持久化 Turn 生命周期，恢复历史消息，保存明确的 completed/failed/interrupted 状态，并让中断可以取消真实网络请求。

### 阶段三：Command/Event 成为真实边界

UI 只发送 Command、消费 RuntimeEvent；斜杠命令解析和流程控制离开 Ink 组件。

### 阶段四：Provider 与 trace 拆分

将传输、协议解析、错误翻译和 reasoning 过滤拆开；trace 只记录计划、状态、工具和结果等可审计事件，不保存原始 chain-of-thought。

### 阶段五：配置、密钥和文件可靠性

将 API Key 移到用户级安全位置，验证配置结构，对 JSON/索引采用原子写入并提供损坏恢复规则。

## 5. 第一阶段详细设计：真实上下文压缩

### 5.1 核心概念：覆盖边界

`ContextSummary` 增加可选字段：

```ts
coveredThroughItemId?: string;
```

它表示“这份摘要已经代表了从会话开始到该 Item 为止的历史”。构造下一次模型上下文时：

1. 加入稳定 System Prompt。
2. 加入最新有效摘要。
3. 找到 `coveredThroughItemId` 在会话 Item 列表中的位置。
4. 只加入该位置之后的新消息。

旧摘要没有该字段时视为旧格式，只能作为提示信息，不能据此删除模型可见历史。这保证向后兼容。

如果覆盖边界在历史中找不到，系统必须保守处理：忽略该边界、记录警告，不得静默丢失对话。

### 5.2 压缩不能吞掉当前问题

自动压缩发生在用户提交新问题之后。摘要只能覆盖当前 Turn 之前已经完成的历史，当前用户输入必须保持原文进入模型。

因此自动压缩选择的边界是“当前 Turn 之前最后一个已完成的 assistant message”，而不是刚刚写入的当前 user message。

手动 `/compact` 在没有 Turn 运行时，可以覆盖到最近一个已完成的 assistant message。没有足够旧历史时返回 no-op，不制造空摘要。

### 5.3 正确的自动压缩顺序

当前顺序是先构造上下文、再压缩、最后仍发送旧上下文。修改后必须是：

```text
保存当前用户输入
→ 第一次构造并估算上下文
→ 判断是否超线
→ 压缩当前 Turn 之前的历史
→ 重新读取摘要
→ 重新构造并重新估算上下文
→ 将新上下文发送给模型
```

如果压缩失败，不写覆盖边界，不假装压缩成功；Runtime 返回明确错误并保留完整历史。

### 5.4 摘要生成策略

第一阶段先把“摘要内容怎样生成”和“哪些历史被摘要覆盖”拆开：

```ts
interface SummaryGenerator {
  generate(items: ThreadItem[], reason: CompactReason): Promise<string>;
}
```

第一版使用改进后的本地生成器，汇总被覆盖区域中的最近目标和若干关键往返，并限制总长度。测试使用 FakeSummaryGenerator，确保不访问网络。

模型生成的高质量语义摘要属于后续增强。覆盖边界和上下文替换不依赖具体摘要生成方式，因此以后更换生成器不会改动 Runtime 主流程。

### 5.5 Token 预算

模型上下文不能把全部窗口都交给输入。有效输入预算为：

```text
workingContextLimit - maxCompletionTokens
```

自动压缩线在有效输入预算上应用 `autoCompactRatio`。压缩后必须重新计算 token 估算，并通过事件把压缩前、压缩后和阈值告诉 UI。

字符数除以四仍只是第一版估算；Provider 精确 tokenizer 不在本阶段引入。

### 5.6 数据保存

- 原始 ThreadItem 永久保留在 JSONL，不物理删除。
- ContextSummary 继续追加写入摘要 JSONL。
- 模型上下文通过覆盖边界过滤历史。
- trace 不进入模型上下文。
- 同一 Thread 可以有多次摘要，只使用最新且覆盖边界有效的一份。

### 5.7 事件与 UI

保留现有 `compact.started` 和 `compact.completed`，扩展 completed 事件，使其包含：

- summary 内容；
- 覆盖到的 Item ID；
- 压缩前 token 估算；
- 压缩后 token 估算；
- 是否实际发生压缩。

UI 只负责展示这些信息，不自行判断压缩规则。

### 5.8 测试

使用 Node 内置测试能力和现有 `tsx`，不新增测试框架依赖。至少覆盖：

1. 手动压缩后，覆盖范围内的消息不再进入模型上下文。
2. 覆盖边界之后的新消息仍完整进入上下文。
3. 自动压缩后会重新构造上下文，而不是复用旧对象。
4. 当前用户输入不会被摘要吞掉或重复加入。
5. 无效或缺失覆盖边界不会导致历史静默丢失。
6. 压缩失败时不持久化虚假成功状态。
7. trace 永远不会进入模型上下文。
8. 大消息样例在压缩后 token 估算确实下降到阈值以下，或明确报告无法压缩。

## 6. 错误处理原则

- 失败必须显式可见，不能把异常伪装成成功状态。
- 不确认新状态已持久化前，不改变模型可见历史边界。
- 兼容旧 JSONL 数据；新增字段优先可选，逐步迁移。
- 测试不得读取或输出真实 API Key。
- 修复不得删除现有 `.mini-codex` 会话文件。

## 7. 教学节奏

每个阶段固定采用以下循环：

1. 白话解释问题和本次目标。
2. 指出对应源码位置。
3. 先写一个会失败的行为测试。
4. 修改最小范围代码让测试通过。
5. 运行 TypeScript 检查、自动测试和必要的 CLI 冒烟测试。
6. 展示修改前后差异。
7. 总结一条可复用的 Agent 架构原则。

## 8. 第一阶段完成标准

- `/compact` 后模型可见上下文确实缩小。
- 原始历史仍能从 JSONL 读取。
- 自动压缩在模型请求前完成，并使用重建后的上下文。
- 当前用户输入保持原文。
- 旧摘要和旧会话仍可加载。
- 新增自动测试全部通过。
- `npm run check` 和 `npm run build` 通过。
- 不需要真实 MiniMax API 即可验证压缩核心逻辑。

## 9. 第一阶段实施结果

- 摘要现在带有 `coveredThroughItemId`，模型上下文只加载该边界之后的新消息；原始 JSONL 历史不删除。
- 自动压缩会在调用模型前重新读取摘要并重建上下文。
- 输入预算会预留 `maxCompletionTokens`，压缩阈值基于剩余输入预算计算。
- 当前用户输入不会被压缩、吞掉或重复加入。
- 如果当前输入本身在压缩后仍然超限，Runtime 会终止该轮并明确报错，不调用模型。
- `/compact` 的 UI 状态会显示压缩前后 token 估算或 no-op 原因。
- 离线验证结果：15 个测试通过，`npm run check` 通过，`npm run build` 通过。
- 行为探针：示例上下文从约 10,120 token 降到 130 token，当前问题只出现一次。

## 10. 第二阶段实施结果：会话恢复与中断

已完成 Turn 生命周期与历史水合的第一小片：Turn snapshot 和流式 assistant delta 采用追加式 JSONL 保存；启动时将遗留 `running` Turn 恢复为 `interrupted`；旧消息和未完成草稿重新加载到 UI；未完成助手草稿不会进入模型上下文。

### 主动中断小片实施结果

主动网络取消与 `/interrupt` 已完成：UI 在 busy 时只放行取消命令，Runtime 持有并触发当前请求的 AbortController，MiniMax fetch 收到外部 signal；取消后的 Turn 为 `interrupted`，部分回复保存但不进入模型上下文。

### 历史 Thread 导航实施结果

`/threads` 和 `/resume <threadId>` 已完成。Storage 在一次索引变换中保证唯一 active thread；Runtime 切换后恢复目标 stale Turn 并加载目标历史；下一次模型请求不会混入原 thread 内容。

### 新建 Thread 实施结果

`/new` 已完成。创建新 active thread 时，Storage 在同一次索引写入中归档旧 active；新会话从空历史开始，旧会话保持可列出、可恢复。至此多会话具备“新建、查看、切换”闭环，第二阶段真正完成。当前仍是一次一个 active thread，不支持多个模型请求并行运行；下一阶段进入真实 Command/Event 边界。

## 11. 第三阶段实施结果：Command/Event 边界

第三阶段已完成。文本输入先由纯 parser 生成类型化 Command，CommandDispatcher 统一调用 Runtime 并将结果或异常转换为 RuntimeEvent，Ink App 只维护视图状态和渲染事件。Turn 流式事件携带 turnId，不再依赖局部闭包关联助手消息；API key 不会从 dispatcher 事件边界泄漏。静态架构测试保证 App 不得重新直接调用 AgentRuntime。

## 12. 第四阶段实施结果：Provider 与安全 Trace

第四阶段已完成。原先集中在 `model-adapter.ts` 的请求构造、HTTP、SSE
解析、错误翻译和 reasoning 处理已拆成独立边界：

- `ProviderProtocol` 分别拥有 Responses 和 Chat Completions 的请求与事件形状；
- `HttpStreamTransport` 负责 fetch、外部取消、网络错误和覆盖完整响应流的截止时间；
- `ProviderModelAdapter` 只负责选择当前 Provider、组合协议与传输，并输出统一 ModelAdapterEvent；
- `ProviderError` 将认证、限流、超时、网络、服务端和请求错误分类，且在错误进入 UI 前删除上游回显的 API key；
- `ReasoningFilter` 丢弃 reasoning 字段和跨 chunk 的 `<think>` 内容，只报告被过滤的字符数；
- `SafeTraceRecorder` 根据固定 TraceCode 生成正文，并按事件码挑选允许保存的 facts，调用者不能再写任意 trace 文本。

这套分层吸收了 claw-code 的 Provider trait / wire protocol 思路，同时保留
Codex 风格的 Runtime 事件边界。验证不访问真实 MiniMax：49 个离线测试、
TypeScript 严格检查和生产构建全部通过。

## 13. 第五阶段实施结果：配置、密钥与文件可靠性

第五阶段已完成：

- `ConfigManager` 在合并默认值前验证配置根对象、Provider、协议、HTTP URL、存储驱动和上下文预算；显式选择不存在的 Provider 会直接报错。
- 普通 JSON 快照使用同目录临时文件、文件刷盘和原子 rename；写入新版本前保存上一份语法有效的 `.bak`。
- 主文件解析或结构验证失败时尝试同一套验证规则读取备份；备份有效则恢复主文件，主备都无效则显示路径并停止，不静默回到空状态。
- Thread 索引增加结构校验，并继续保证最多一个 active Thread。
- JSONL 只自动移除没有换行符的损坏尾记录；中间行损坏会指出行号并失败。
- keytar 可用时继续使用系统钥匙串；否则密钥写到用户级 `credentials.json`。`MINIMAX_CODEX_HOME` 可覆盖用户目录。
- 旧 `.mini-codex/secrets.local.json` 中的全部 Provider 密钥成功迁移后才删除旧文件；清洗后为空的密钥不会落盘。

这一步沿用了 claw-code 的临时文件 + rename 模式，同时增加备份验证和恢复边界。离线验证结果：60 个测试、TypeScript 严格检查和生产构建全部通过。
