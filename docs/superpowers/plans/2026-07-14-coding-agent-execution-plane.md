# MiniMax Coding-Agent 执行平面实施计划

**设计：** `docs/superpowers/specs/2026-07-14-coding-agent-execution-plane-design.md`

**状态：** Task 1-22 实现已完成，确定性离线门禁已通过；真实 Granite 与 live Provider 验收尚未获资源/授权，因此 `agentDefaultRoute` 发布门保持关闭

**目标：** 在保留现有聊天、会话、工作区配置和本地文件的前提下，分阶段加入可拔插模型、本机能力索引、混合召回和受权限约束的 Agent 执行路径。

## 实施纪律

每个任务严格执行 `RED -> GREEN -> focused verification -> wave gate`：先写会失败的契约测试，只实现当前任务所需的最小代码，再运行目标测试。一个 Wave 完成前，不进入下一个 Wave。

全程遵守以下硬约束：

- `ApplicationKernel` 仍是唯一命令路由器；UI 不直接调用 Provider、索引或 Executor。
- 现有 `turn.submit -> TurnEngine` 保持可用；`/chat` 永远是明确回退入口。
- 普通文本在最终切流门禁通过前不改为 Agent 路径；早期只允许显式 `/agent`。
- 模型切换不重写工作区 `config.json`，不改 Thread、索引、工具、权限或项目文件。
- 新 Session 继承最后成功选择的模型，但权限始终恢复为 `confirm`。
- 本机能力发现不联网；只有用户明确要求安装具名项目时，才进入独立安装流程。
- embedding 资源缺失、损坏或超时时必须退回 exact + BM25；普通聊天不得因此阻塞。
- 第一轮不加载任意第三方 Adapter 代码，不开放任意 Shell、文件写入、Git 写入或隐藏网络工具。
- `npm` script 会执行项目代码；在 `confirm` 下必须确认，不能仅凭 `test/check/build` 名称自动放行。
- 离线测试和 CI 不访问真实 Provider，不下载模型资源，不读取真实凭证。
- 不引入 LangChain/LangGraph 核心依赖；仅保留未来可选边界。
- 本计划不授权自动提交或推送；Git 操作按用户后续明确指令执行。

## 分波次交付

| Wave | 结果 | 默认状态 | 前置门禁 |
|---|---|---|---|
| 0 | 规格、计划和基线锁定 | 文档 | 用户批准本计划 |
| A | 可拔插 Provider/模型与全局 sticky 选择 | MiniMax 仍为唯一必需实现 | 模型状态隔离测试通过 |
| B | 本机能力目录、索引和混合召回 | report-only | 召回、延迟和无网络门禁通过 |
| C | 只读 Agent 垂直切片 | 仅显式 `/agent` | 持久化、权限、恢复门禁通过 |
| D | 产品切流、评估和回退 | 分级开关 | 扩展数据集与完整回归通过 |

依赖顺序：`Wave A -> Wave B -> Wave C -> Wave D`。同一 Wave 内仍按任务编号执行，避免多个状态真相源并存。

## 设计到任务的追踪

| 设计要求 | 实施任务 |
|---|---|
| Provider/模型身份分离、MiniMax 兼容 | 1、3、4 |
| 全局 sticky 模型、不改本地设置 | 2、4、5、6 |
| 受管本机能力、重名和快照 | 7、8、9 |
| exact + BM25 + embedding + RRF | 10、11、12 |
| `/capabilities` 可见但不执行 | 13 |
| Provider-neutral tool call | 14 |
| `Thread -> Turn -> ordered AgentItem` | 15 |
| Session 权限与 Policy | 16 |
| 第一阶段 Executor | 17 |
| 有界 Agent 循环 | 18 |
| checkpoint、`/continue`、保守恢复 | 19 |
| UI 与显式 `/agent` | 20 |
| 默认切流、评估、回退 | 21、22 |

## Wave 0：锁定批准边界

### Task 0：计划批准与基线快照

**文件**

- 修改：`docs/superpowers/specs/2026-07-14-coding-agent-execution-plane-design.md`
- 新增：`docs/superpowers/plans/2026-07-14-coding-agent-execution-plane.md`
- 修改：`E:/Agenc/agent-logs/temporary/2026-07-14.md`

**步骤**

1. 把正式设计状态更新为用户已批准。
2. 记录 `npm test`、`npm run check`、`npm run build` 和 `git diff --check` 基线。
3. 记录源码、依赖、模型资源、Provider 配置和凭证均未改变。
4. 用户批准本计划前，不执行 Task 1。

**验证**

- 规格和计划互相引用，状态无矛盾。
- 计划包含每个设计章节的实施任务和明确回退门禁。
- 文档检查不产生尾随空白或损坏 Markdown fence。

## Wave A：Provider 与模型选择基础

### Task 1：建立 Provider、Profile、Model 和 Runtime 契约

**文件**

- 新增：`src/providers/provider-adapter.ts`
- 新增：`src/providers/provider-profile.ts`
- 新增：`src/providers/model-profile.ts`
- 新增：`src/runtime/model-runtime.ts`
- 修改：`src/types.ts`
- 新增测试：`test/provider-adapter-contract.test.ts`
- 新增测试：`test/model-profile.test.ts`

**RED**

- 证明 `adapterId`、`providerProfileId`、`modelProfileId` 不能混用。
- 证明未知 feature、未知 schema 版本和不完整 profile 均 fail-closed。
- 证明 MiniMax 内建 Adapter 标识受保护，不能被普通 profile 覆盖。
- 证明模型 profile 只描述模型/协议能力，不携带本机工具权限。

**GREEN**

- 定义版本化、可运行时校验的纯数据契约。
- 定义 `ProviderFeatureProfile`、`ModelFeatureProfile` 和统一错误分类。
- 定义 `ModelRuntime`/`ModelRuntimeFactory` 端口，不改现有请求路径。

**验证**

- `npm exec -- tsx --test test/provider-adapter-contract.test.ts test/model-profile.test.ts`
- `npm run check`
- 现有 Provider tests 保持通过。

### Task 2：抽取用户配置根并实现最小 ModelStateStore

**文件**

- 新增：`src/config/user-config-root.ts`
- 新增：`src/config/model-state-store.ts`
- 修改：`src/config/credential-store.ts`
- 新增测试：`test/user-config-root.test.ts`
- 新增测试：`test/model-state-store.test.ts`
- 修改测试：`test/credential-consent.test.ts`
- 修改测试：`test/file-reliability.test.ts`

**RED**

- 证明 `MINIMAX_CODEX_HOME` 与各 OS 默认目录解析行为保持兼容。
- 证明不存在状态文件时只返回“未选择”，不自动写盘。
- 证明记录只允许 `schemaVersion` 和完全限定 `lastSelectedModelProfileId`。
- 证明状态不接受凭证、权限、workspace path、prompt 或 endpoint 字段。
- 证明主文件损坏时可从备份恢复；主备均无效时返回显式恢复状态。
- 证明失败写入不会破坏旧指针。

**GREEN**

- 从 `CredentialStore` 抽出单一 `UserConfigRoot` 解析器，保持原路径不变。
- 新增用户级 `model-state.json` 原子写入、备份和 schema 校验。
- 不复用凭证文件，不把模型状态写入工作区。

**验证**

- `npm exec -- tsx --test test/user-config-root.test.ts test/model-state-store.test.ts test/credential-consent.test.ts test/file-reliability.test.ts`
- 比较抽取前后的凭证目标路径 fixture。
- 搜索确认 `PermissionMode` 不出现在 model-state 模块。

### Task 3：建立 Registry 与 MiniMax 兼容 Adapter

**文件**

- 新增：`src/providers/provider-adapter-registry.ts`
- 新增：`src/providers/builtin-provider-adapter.ts`
- 新增：`src/config/user-profile-store.ts`
- 新增：`src/runtime/model-profile-registry.ts`
- 新增：`src/runtime/profile-setup-service.ts`
- 修改：`src/runtime/provider-service.ts`
- 修改：`src/providers/provider-gateway.ts`
- 修改：`src/config/config-manager.ts`
- 新增测试：`test/provider-adapter-registry.test.ts`
- 新增测试：`test/user-profile-store.test.ts`
- 新增测试：`test/model-profile-registry.test.ts`
- 新增测试工具：`test/support/provider-conformance-suite.ts`
- 新增测试：`test/provider-conformance.test.ts`
- 修改测试：`test/provider-protocol.test.ts`
- 修改测试：`test/config-manager.test.ts`

**RED**

- 证明现有 Responses / Chat Completions 配置能规范化为内建 Adapter + ProviderProfile + ModelProfile。
- 证明旧 workspace config 仍可读取，但 registry 初始化不重写它。
- 证明用户级 Profile Store 将 ProviderProfile 与 ModelProfile 分开保存，不包含凭证或权限。
- 证明 workspace 专属 legacy profile 未经显式 setup/promotion 不能成为跨项目 sticky current model。
- 证明 setup/promotion 与模型切换是两个操作；保存 profile 不自动改变 current pointer。
- 证明可选 profile 损坏、重名或 feature 不匹配时只隔离该 profile。
- 证明第一轮 registry 拒绝动态加载任意 Tier-2 JavaScript 包。
- 证明每个准备启用的内建/Tier-1 profile 按其声明的当前能力通过统一离线 conformance suite；没有 fixture 或失败的 profile 不可激活。

**GREEN**

- 用现有 `ProviderProtocolFactory` 和 `StrictProviderGateway` 包装内建 Adapter。
- 提供由 legacy `AppConfig` 生成只读兼容 profile 的适配层。
- 通过原子、带备份的用户级 Profile Store 提供 Tier-1 配置扩展，并由显式 ProfileSetupService 验证/发布；不自动迁移 workspace profile。
- 建立可复用 conformance helper；这一阶段先覆盖纯聊天、streaming、usage、completion、cancel、malformed/EOF、failure、redaction 和 feature fail-closed，Task 14 再扩展 tool-call 矩阵。
- 保持 credential target、endpoint 校验、reasoning filter 和 transport 规则不变。

**验证**

- `npm exec -- tsx --test test/provider-adapter-registry.test.ts test/user-profile-store.test.ts test/model-profile-registry.test.ts test/provider-conformance.test.ts test/provider-protocol.test.ts test/config-manager.test.ts test/provider-security.test.ts`
- `npm run check`
- `npm run build`

### Task 4：实现事务性 ModelSelectionService

**文件**

- 新增：`src/runtime/model-selection-service.ts`
- 修改：`src/runtime/provider-service.ts`
- 修改：`src/runtime/turn-engine.ts`
- 修改：`src/runtime/runtime-application.ts`
- 修改：`test/kernel-test-utils.ts`
- 新增测试：`test/model-selection-service.test.ts`
- 修改测试：`test/turn-engine.test.ts`
- 修改测试：`test/application-kernel.test.ts`

**RED**

- 证明启动解析顺序是“有效 sticky 指针 -> workspace 兼容默认值”。
- 证明切换按“校验 profile -> 定位已配置凭证 -> 构建 Runtime -> 原子写指针 -> 发布 Runtime”执行。
- 证明任一步失败时旧 Runtime 和旧指针均保持有效。
- 证明运行中的 Turn 拒绝切换。
- 证明切换不调用 `ConfigManager.save()`，不触发索引重建或历史压缩。
- 证明 workspace 专属 legacy profile 未经显式 promotion 时不能写入全局 sticky pointer。
- 证明不支持 tool-call 的模型仍可按 profile 做纯聊天，但 Agent Turn 在请求前明确失败。

**GREEN**

- 引入不可变 `ActiveModelSelection` 与运行时快照。
- `TurnEngine` 从运行时端口获取当前请求投影，不再把“切模型”表达成改写整个 `AppConfig`。
- 保留旧 Runtime，直到新 Runtime 完成全部验证和状态持久化。

**验证**

- `npm exec -- tsx --test test/model-selection-service.test.ts test/turn-engine.test.ts test/application-kernel.test.ts`
- 使用临时目录比较切换前后 workspace 全量文件 hash。
- 使用 fake Runtime/credential，不发网络请求。

### Task 5：接入模型命令、RuntimeEvent 和 UI 状态

**文件**

- 修改：`src/protocol.ts`
- 修改：`src/runtime/application-kernel.ts`
- 修改：`src/runtime/command-arbiter.ts`
- 修改：`src/ui/chat-input-policy.ts`
- 修改：`src/ui/ui-state.ts`
- 修改：`src/ui/format-runtime-event.ts`
- 修改：`src/ui/App.tsx`
- 修改测试：`test/chat-input-policy.test.ts`
- 修改测试：`test/command-arbiter.test.ts`
- 修改测试：`test/application-kernel.test.ts`
- 修改测试：`test/ui-state.test.ts`
- 修改测试：`test/ui-status.test.ts`

**RED**

- `/models` 只列可用/不可用原因，不改状态。
- `/model <fully-qualified-id>` 只在 idle 时事务切换。
- `/provider` 保留兼容语法：解析到该 Provider 的默认 ModelProfile，并委托同一事务性切换流程；它不再改写整份 workspace config，也不与模型 ID 淆混。
- 切换成功、失败、恢复所需 RuntimeEvent 均不泄露凭证或 endpoint secret。
- UI 显示当前模型和 Provider，但不把模型选择显示成权限变化。

**GREEN**

- 新增 typed commands/events，继续由 `ApplicationKernel.route()` 唯一路由。
- 给 read-only 列表命令和 mutating 切换命令配置正确仲裁类型。
- 将旧 `provider.switch` 兼容命令改成 ModelSelectionService 的薄适配层，不再调用旧的整配置保存路径。
- 保持普通文本和原有 slash command 解析不变。

**验证**

- `npm exec -- tsx --test test/chat-input-policy.test.ts test/command-arbiter.test.ts test/application-kernel.test.ts test/ui-state.test.ts test/ui-status.test.ts`
- `npm run check`

### Task 6：保存未来 Turn 的模型来源并验证跨 Session 不变量

**文件**

- 修改：`src/types.ts`
- 修改：`src/runtime/session-service.ts`
- 修改：`src/storage/jsonl-storage.ts`
- 修改：`src/storage/session-repository.ts`
- 修改：`src/runtime/context-engine.ts`
- 新增 fixture：`test/fixtures/model-selection/`
- 新增测试：`test/model-selection-persistence.test.ts`
- 修改测试：`test/storage-versioning.test.ts`
- 修改测试：`test/storage-turns.test.ts`
- 修改测试：`test/context-manager.test.ts`

**RED**

- Session A 切到 M2 后，新 Session 仍使用 M2。
- 新 Session 的权限仍是 `confirm`。
- 历史 Turn 保留原 provider/model provenance，未来 Turn 才记录 M2。
- 旧 JSONL fixture 无新增字段也可读取。
- 指针损坏、profile 被移除或 credential 缺失时进入显式恢复，不静默选无关模型。
- 切换前后 Thread、summary、capability snapshot、tool definitions 和 workspace 文件保持不变。

**GREEN**

- 用可选、版本兼容字段扩展 Turn provenance。
- Session 初始化只读取模型状态；不在每次启动时重写 pointer。
- Context 继续从 provider-neutral durable history 生成临时请求投影。

**验证**

- `npm exec -- tsx --test test/model-selection-persistence.test.ts test/storage-versioning.test.ts test/storage-turns.test.ts test/context-manager.test.ts`
- 检查 legacy fixture 未被测试过程改写。

**Wave A 门禁**

- 全部离线测试、`npm run check`、`npm run build`、`git diff --check` 通过。
- MiniMax 现有聊天、interrupt、recovery、compaction 和 trace 测试无回归。
- 模型切换前后 workspace hash 不变。
- 无真实 Provider 请求，无 credential 输出。
- 门禁失败时回退为旧 `ProviderService` + `TurnEngine` 路径，不进入 Wave B。

## Wave B：本机能力目录与混合召回

### Task 7：定义 CapabilityManifest 和只读 SourceAdapter

**文件**

- 新增：`src/capabilities/types.ts`
- 新增：`src/capabilities/capability-manifest.ts`
- 新增：`src/capabilities/source-adapter.ts`
- 新增：`src/capabilities/sources/minimax-source.ts`
- 新增：`src/capabilities/sources/codex-source.ts`
- 新增：`src/capabilities/sources/claw-code-source.ts`
- 新增 fixture：`test/fixtures/capabilities/sources/`
- 新增测试：`test/capability-manifest.test.ts`
- 新增测试：`test/capability-source-adapters.test.ts`

**RED**

- 证明受管目录外定义、路径穿越、无效 ID/schema、安全类和执行描述均被隔离。
- 证明 Adapter 只读取元数据，不 import/require/execute 第三方代码。
- 证明 MiniMax、Codex skill/plugin 和 Claw Code 命令可规范化为统一描述。
- 证明缺少可选字段时有确定默认值，未知高风险字段不被静默接受。

**GREEN**

- 定义有限 `CapabilitySafetyClass`：`catalog_read`、`workspace_read`、`local_diagnostic` 及未来保留类。
- 定义静态 `CapabilityDescriptor`、来源、availability、执行入口和 intent document。
- YAML/frontmatter 只通过受控 parser 读取；若引入依赖，先固定版本并记录许可证/供应链检查。

**验证**

- `npm exec -- tsx --test test/capability-manifest.test.ts test/capability-source-adapters.test.ts`
- 测试进程断言扫描期间未创建 child process、未访问网络。

### Task 8：实现路径策略、优先级与 CapabilityCatalog

**文件**

- 新增：`src/capabilities/path-policy.ts`
- 新增：`src/capabilities/capability-catalog.ts`
- 新增：`src/capabilities/source-precedence.ts`
- 新增测试：`test/capability-path-policy.test.ts`
- 新增测试：`test/capability-catalog.test.ts`
- 新增测试：`test/capability-shadowing.test.ts`

**RED**

- 证明 symlink/junction 逃逸受管根时被拒绝。
- 证明内建保留 ID 永远不能覆盖；其他定义严格按“项目原生 -> 用户原生 -> 项目兼容导入 -> 用户兼容导入”处理。
- 证明 losers 标为 `shadowed`，仍可在诊断中看到但绝不进入执行候选。
- 证明 invalid/disabled/unavailable/stale 不会成为可执行候选。
- 证明大小写、Unicode 规范化和 Windows path 行为确定。

**GREEN**

- Catalog 只接收 SourceAdapter 输出，不直接扫描任意目录。
- 使用真实路径边界和有限状态机发布规范化 capability。
- 所有冲突和拒绝均产生脱敏诊断，不读取 capability 内容之外的数据。

**验证**

- `npm exec -- tsx --test test/capability-path-policy.test.ts test/capability-catalog.test.ts test/capability-shadowing.test.ts`
- Windows 与 POSIX path fixture 均通过。

### Task 9：实现 last-known-good Snapshot 与增量刷新

**文件**

- 新增：`src/capabilities/capability-snapshot.ts`
- 新增：`src/capabilities/snapshot-store.ts`
- 新增：`src/capabilities/refresh-coordinator.ts`
- 新增测试：`test/capability-snapshot.test.ts`
- 新增测试：`test/capability-refresh.test.ts`

**RED**

- 证明 query path 不触发目录全量扫描。
- 证明 managed mutation、显式刷新和指纹变化可以触发后台 rebuild。
- 证明 rebuild 中读者只看到旧完整快照或新完整快照，不看到半成品。
- 证明 rebuild 失败继续使用 last-known-good，并暴露 stale 原因。
- 证明主/备快照损坏有显式恢复行为。

**GREEN**

- 构建不可变 `CapabilitySnapshot`，通过原子引用 swap 发布。
- 持久化只含规范化 metadata/index；不复制凭证、脚本正文或隐私内容。
- 刷新采用 debounce、fingerprint 和单飞并发控制。

**验证**

- `npm exec -- tsx --test test/capability-snapshot.test.ts test/capability-refresh.test.ts test/file-reliability.test.ts`
- 并发测试重复运行，验证无半发布和重复 rebuild。

### Task 10：实现 exact、中文友好 tokenization 与 BM25

**文件**

- 新增：`src/capabilities/search/query-normalizer.ts`
- 新增：`src/capabilities/search/exact-index.ts`
- 新增：`src/capabilities/search/facet-index.ts`
- 新增：`src/capabilities/search/bm25-index.ts`
- 新增：`src/capabilities/search/lexical-retriever.ts`
- 新增 fixture：`test/fixtures/capabilities/retrieval-lexical.json`
- 新增测试：`test/capability-query-normalizer.test.ts`
- 新增测试：`test/capability-facet-index.test.ts`
- 新增测试：`test/capability-bm25.test.ts`
- 新增测试：`test/capability-exact-resolution.test.ts`

**RED**

- exact slash command、alias、完全限定 ID 必须 100% 命中正确 capability。
- 中英混合、大小写、连字符、路径和常见口语同义表达有稳定 token。
- domain/action/object facets 能提供稳定过滤/扩展入口，但不能绕过 availability 和 exact 优先级。
- disabled/invalid/shadowed/stale 不得进入可执行结果。
- no-match 不得由低分候选冒充确定命中。

**GREEN**

- exact 路径先行并短路语义召回。
- facet graph 与 exact/BM25 从同一 immutable snapshot 派生，不建立第二份 Catalog 真相源。
- BM25 只索引静态 intent document，不在请求时读取源文件。
- tokenizer 版本写入 snapshot，变化时触发明确 rebuild。

**验证**

- `npm exec -- tsx --test test/capability-query-normalizer.test.ts test/capability-facet-index.test.ts test/capability-bm25.test.ts test/capability-exact-resolution.test.ts`
- exact lookup 基准单独记录，不与冷启动混算。

### Task 11：建立独立 Granite Embedding 资源包边界

**文件**

- 新增：`src/capabilities/embedding/embedding-provider.ts`
- 新增：`src/capabilities/embedding/embedding-resource-manifest.ts`
- 新增：`src/capabilities/embedding/embedding-resource-locator.ts`
- 新增：`src/capabilities/embedding/granite-embedding-runtime.ts`
- 新增：`src/capabilities/embedding/embedding-worker.ts`
- 新增：`docs/embedding-resource-package.md`
- 新增 fixture：`test/fixtures/embedding-resource/`
- 新增测试：`test/embedding-resource.test.ts`
- 新增测试：`test/embedding-runtime.test.ts`

**RED**

- 证明资源包缺失、版本不兼容、hash 不符、模型 revision 不符和 CPU 不支持时明确降级。
- 证明 runtime 初始化失败或 query 超时不会联网、不会阻塞普通聊天。
- 证明只接受显式安装到受管资源目录的 `ibm-granite/granite-embedding-97m-multilingual-r2` qint8 AVX2 资源。
- 证明核心仓库不内嵌大权重，CI fixture 也不下载权重。

**GREEN**

- 资源包固定为 `@minimax-codex/embedding-granite-97m-r2-avx2` 独立 sidecar artifact；manifest 至少包含 model ID/revision、runtime ABI、架构、量化、license、文件 hash 和 tokenizer version。
- 实施前从正式来源复核设计中记录的 revision 与 SHA-256；复核失败即停止资源接入，不猜测或自动换模型。
- 核心只实现定位、校验、懒加载和 worker 生命周期；权重不进入主 bundle。
- Node 推理后端版本在实施时经 Node 20/Windows/Linux 兼容与许可证检查后精确锁定，不使用浮动版本。
- 安装资源是独立显式步骤；检索失败永远不触发自动下载。

**验证**

- `npm exec -- tsx --test test/embedding-resource.test.ts test/embedding-runtime.test.ts`
- 用 tiny fake vector fixture 验证契约；真实资源只在本地显式验收中测试。
- 运行网络哨兵，证明模块初始化与降级路径无 HTTP/DNS 请求。

### Task 12：实现 RRF、预算和检索评估器

**文件**

- 新增：`src/capabilities/search/vector-index.ts`
- 新增：`src/capabilities/search/rrf.ts`
- 新增：`src/capabilities/search/hybrid-retriever.ts`
- 新增：`src/capabilities/search/capability-card.ts`
- 新增：`src/capabilities/eval/retrieval-evaluator.ts`
- 新增 fixture：`test/fixtures/capabilities/retrieval-cases.json`
- 新增测试：`test/capability-rrf.test.ts`
- 新增测试：`test/capability-hybrid-retrieval.test.ts`
- 新增测试：`test/capability-retrieval-eval.test.ts`

**RED**

- 证明 exact 不被融合结果覆盖。
- 证明 BM25 与 embedding 通过稳定 RRF 合并，tie-breaker 可重复。
- 证明 embedding timeout/error 退回 exact + BM25，并记录脱敏 fallback reason。
- 证明只返回有效 capability ID，最多五张卡片，满足硬 token 上限。
- 证明低分、候选分歧和明确 no-match 会返回“未确定/候选列表”，不会伪装成可执行的唯一匹配。
- 证明 60 条中英混合初始数据集可分别报告 lexical、embedding 和 fused 指标。

**GREEN**

- 用 snapshot version 绑定 lexical/vector index。
- 引入 150 ms embedding deadline 和取消信号。
- capability cards 只含最小必要描述、schema 摘要和稳定 ID。
- 评估器输出 recall@5、top-1、MRR、no-match precision 与路径耗时。

**验证**

- `npm exec -- tsx --test test/capability-rrf.test.ts test/capability-hybrid-retrieval.test.ts test/capability-retrieval-eval.test.ts`
- exact correctness 和返回 ID validity 必须 100%。
- fused recall@5 必须高于 BM25-only，且达到设计门槛后才进入 Task 13。

### Task 13：增加 `/capabilities` 报告模式

**文件**

- 修改：`src/protocol.ts`
- 修改：`src/runtime/application-kernel.ts`
- 修改：`src/runtime/command-arbiter.ts`
- 修改：`src/ui/chat-input-policy.ts`
- 修改：`src/ui/ui-state.ts`
- 修改：`src/ui/format-runtime-event.ts`
- 修改：`src/ui/App.tsx`
- 新增测试：`test/capability-commands.test.ts`
- 修改测试：`test/ui-state.test.ts`
- 修改测试：`test/application-kernel.test.ts`

**RED**

- `/capabilities` 列出来源、状态、shadowing 和 snapshot health，但不执行能力。
- `/capabilities search <query>` 只展示候选和匹配路径，不把候选发给 Provider。
- catalog/index 不可用时聊天与旧命令不受影响。
- capability mode 关闭时不初始化 embedding、不扫描目录、不增加远程请求。

**GREEN**

- 新增 read-only commands/events，由 Kernel 路由到 catalog/retrieval service。
- UI 只展示有限诊断；路径和内容按安全日志规则脱敏。
- report-only 开关默认关闭执行面。

**验证**

- `npm exec -- tsx --test test/capability-commands.test.ts test/ui-state.test.ts test/application-kernel.test.ts`
- 确认 report-only 操作没有产生 invocation Item 或 child process。

**Wave B 门禁**

- 60 条数据集满足 exact 100%、ID validity 100%、recall@5 >= 95%、top-1 >= 85%、MRR >= 0.90、no-match precision >= 95%。
- reference machine 上 exact p95 <= 10 ms、warm fused p95 <= 100 ms；冷启动、build、加载分别报告。
- capability mode 关闭时聊天 p95 回归不超过 2%，远程 Provider 增量请求为 0。
- 网络哨兵证明发现、构建、检索和降级均不联网。
- 门禁失败时关闭 embedding 或整个 capability mode，保留 exact + BM25 或纯聊天路径。

## Wave C：只读 Agent 垂直切片

### Task 14：扩展 Provider-neutral ModelAction

**文件**

- 新增：`src/agent/model-action.ts`
- 修改：`src/providers/provider-protocol.ts`
- 修改：`src/providers/provider-gateway.ts`
- 修改：`src/providers/provider-model-adapter.ts`
- 新增 fixture：`test/fixtures/provider-actions/`
- 新增测试：`test/provider-action-normalization.test.ts`
- 修改测试：`test/provider-protocol.test.ts`
- 修改测试：`test/provider-model-adapter.test.ts`

**RED**

- Responses 和 Chat Completions fixtures 可产生同一 `text`、`tool_call`、`usage`、`completed`、`failure` 动作。
- 完整、分片和并行 tool call 均按稳定 call ID 组装。
- malformed event、premature EOF、未知动作和不支持 feature 必须 fail-closed。
- raw reasoning、raw frame、credential 不进入 RuntimeEvent/trace。

**GREEN**

- 给请求添加可选 tools schema，纯聊天请求保持原格式。
- Adapter 只做协议翻译，不执行工具、不决定权限。
- 统一 terminal completion、usage、cancellation 和错误分类。
- 扩展 Task 3 的统一 conformance helper，加入完整、分片、并行 tool calls 和不支持 tool feature 的 fail-closed fixtures。

**验证**

- `npm exec -- tsx --test test/provider-action-normalization.test.ts test/provider-protocol.test.ts test/provider-model-adapter.test.ts test/reasoning-filter.test.ts`
- 现有纯文本 streaming fixtures 无回归。

### Task 15：扩展 ordered AgentItem 持久化契约

**文件**

- 新增：`src/agent/agent-item.ts`
- 修改：`src/types.ts`
- 修改：`src/runtime/session-service.ts`
- 修改：`src/storage/jsonl-storage.ts`
- 修改：`src/storage/session-repository.ts`
- 修改：`src/runtime/context-engine.ts`
- 新增 fixture：`test/fixtures/agent-items/`
- 新增测试：`test/agent-item-storage.test.ts`
- 修改测试：`test/storage-versioning.test.ts`
- 修改测试：`test/summary-generator.test.ts`

**RED**

- 证明一个 Turn 可按序保存 user、assistant、tool request、tool result、checkpoint、error/final Item。
- 证明 request/result 使用稳定 `invocationId` 关联，顺序和 schema 可校验。
- 证明旧 Thread/Turn JSONL 无需重写即可读取。
- 证明通用聊天 context/summary 不意外注入完整工具输出。
- 证明模型 provenance、snapshot ID、budget 与 mode 可审计但不包含秘密。

**GREEN**

- 扩展现有 Thread/Turn 层级，不新建第二套 AgentRun 持久化真相源。
- 为每种 AgentItem 建立版本化 validator 和有界 payload。
- ContextEngine 仅按显式 Agent 请求组装需要的 tool result。

**验证**

- `npm exec -- tsx --test test/agent-item-storage.test.ts test/storage-versioning.test.ts test/storage-turns.test.ts test/summary-generator.test.ts test/context-manager.test.ts`
- 对 legacy fixtures 做只读兼容测试。

### Task 16：实现 Session PermissionService 与 PolicyEngine

**文件**

- 新增：`src/runtime/permission-service.ts`
- 新增：`src/capabilities/policy-engine.ts`
- 新增：`src/capabilities/capability-invocation.ts`
- 修改：`src/protocol.ts`
- 修改：`src/runtime/application-kernel.ts`
- 修改：`src/ui/chat-input-policy.ts`
- 新增测试：`test/permission-service.test.ts`
- 新增测试：`test/capability-policy-engine.test.ts`
- 修改测试：`test/application-kernel.test.ts`

**RED**

- 每个新 Session 默认 `confirm`，恢复 Thread 也不恢复 `full_access`。
- `/permissions` 可查看；显式命令可在当前 Session 升级/降级。
- 模型切换、检索分数和 Provider feature 均不能改变权限。
- `catalog_read` 可直接通过；验证后的 `workspace_read` 遵守设计策略。
- `local_diagnostic` 在 `confirm` 下要求确认，即使脚本名是 `test/check/build`。
- `full_access` 仍不能绕过 ID/schema/path/snapshot/task scope 或隐藏网络禁令。

**GREEN**

- PermissionService 只持有 Session 内状态，不写 ModelStateStore、workspace config 或 Thread durable state。
- PolicyEngine 返回 typed allow/confirm/deny decision 和脱敏原因。
- Dispatcher 前必须重新校验 capability availability 与 snapshot。

**验证**

- `npm exec -- tsx --test test/permission-service.test.ts test/capability-policy-engine.test.ts test/application-kernel.test.ts test/chat-input-policy.test.ts`
- 搜索确认 Provider 层不能 import PermissionService 的 mutation API。

### Task 17：实现工作区只读与受控 npm diagnostic Executor

**文件**

- 新增：`src/capabilities/capability-dispatcher.ts`
- 新增：`src/capabilities/executors/workspace-read-executor.ts`
- 新增：`src/capabilities/executors/npm-diagnostic-executor.ts`
- 新增：`src/capabilities/execution-limits.ts`
- 新增 fixture：`test/fixtures/executors/`
- 新增测试：`test/capability-dispatcher.test.ts`
- 新增测试：`test/workspace-read-executor.test.ts`
- 新增测试：`test/npm-diagnostic-executor.test.ts`

**RED**

- workspace reader 拒绝绝对逃逸、`..`、symlink/junction 逃逸、设备文件和超限输出。
- npm executor 只接受 manifest 中预声明 script 与固定 argv，不接受用户拼接命令。
- child process 使用 `shell: false`、固定 cwd、超时、输出上限和取消信号。
- `confirm` 未批准时 npm executor 不启动进程；`full_access` 也不能运行未声明 script。
- 执行前 request Item 已 durable；执行后 result Item 引用同一 `invocationId`。

**GREEN**

- Dispatcher 只接受 typed `CapabilityInvocation`，先过 Catalog + Policy 再选择有限 Executor。
- 第一轮只注册 `catalog_read`、`workspace_read`、`local_diagnostic`。
- 不提供通用 shell executor，不把 stderr/环境变量秘密写入 trace。

**验证**

- `npm exec -- tsx --test test/capability-dispatcher.test.ts test/workspace-read-executor.test.ts test/npm-diagnostic-executor.test.ts`
- 使用恶意路径、恶意 script、超时、取消和大输出 fixtures。
- 网络哨兵与文件 hash 证明 workspace-read 路径无写入/网络副作用。

### Task 18：实现有界 AgentRunEngine

**文件**

- 新增：`src/runtime/agent-run-engine.ts`
- 新增：`src/agent/agent-budget.ts`
- 新增：`src/agent/agent-context-builder.ts`
- 修改：`src/runtime/runtime-application.ts`
- 修改：`src/runtime/application-kernel.ts`
- 修改：`src/runtime/command-arbiter.ts`
- 新增测试：`test/agent-run-engine.test.ts`
- 新增测试：`test/agent-budget.test.ts`
- 修改测试：`test/application-kernel.test.ts`

**RED**

- 显式 `agent.submit` 执行“本地检索 -> capability cards -> 模型 -> policy -> tool -> 模型”的有限循环。
- 本地路由不增加额外远程“选择工具”请求。
- 无候选、低置信度或 feature 不支持时清晰停止，不虚构 capability ID。
- 超过 step/token/time/tool budget、用户 interrupt 或 terminal action 时确定结束。
- Tool call 必须重新校验当前 snapshot，不能只信模型返回的 ID/参数。
- Agent 失败不破坏 `turn.submit` 聊天路径。

**GREEN**

- AgentRunEngine 复用 SessionService、ContextEngine、Provider runtime、Dispatcher 和 Turn 生命周期。
- 用 ordered AgentItems 表达进度，不创建平行持久化实体。
- capability cards 使用设计中的数量/token 硬上限。

**验证**

- `npm exec -- tsx --test test/agent-run-engine.test.ts test/agent-budget.test.ts test/application-kernel.test.ts test/agent-runtime-interrupt.test.ts`
- fake Provider 覆盖 text-only、single tool、multiple step、invalid tool、timeout、cancel。

### Task 19：实现 checkpoint、`/continue` 与保守恢复

**文件**

- 新增：`src/agent/agent-checkpoint.ts`
- 修改：`src/runtime/agent-run-engine.ts`
- 修改：`src/runtime/session-service.ts`
- 修改：`src/storage/session-repository.ts`
- 修改：`src/protocol.ts`
- 修改：`src/ui/chat-input-policy.ts`
- 新增测试：`test/agent-run-recovery.test.ts`
- 修改测试：`test/agent-runtime-recovery.test.ts`
- 修改测试：`test/chat-input-policy.test.ts`

**RED**

- crash 在 dispatch 前、执行中、result durable 后分别产生确定状态。
- 未配对 request 进入 `indeterminate`，非幂等动作绝不自动重放。
- 只有声明幂等且有确定 invocation identity 的动作才可按策略重试。
- `/continue` 从最后可验证 checkpoint 继续，不重复已完成 tool result。
- `turn.submit` 旧恢复逻辑保持不变。

**GREEN**

- checkpoint 引用 Turn、last AgentItem sequence、snapshot、model provenance 和 budget，不保存秘密。
- 恢复先审计 durable items，再决定 continue/confirm/fail。
- invocation 状态机集中定义，避免 UI、Engine、Storage 各自猜测。

**验证**

- `npm exec -- tsx --test test/agent-run-recovery.test.ts test/agent-runtime-recovery.test.ts test/storage-index-recovery.test.ts test/file-reliability.test.ts`
- 对每个故障注入点重复运行，证明无重复副作用。

### Task 20：接入显式 `/agent`、AgentItems 和状态 UI

**文件**

- 修改：`src/protocol.ts`
- 修改：`src/ui/chat-input-policy.ts`
- 修改：`src/ui/ui-state.ts`
- 修改：`src/ui/format-runtime-event.ts`
- 修改：`src/ui/App.tsx`
- 修改：`src/runtime/application-kernel.ts`
- 新增测试：`test/agent-ui-state.test.ts`
- 修改测试：`test/ui-command-boundary.test.ts`
- 修改测试：`test/ui-dispatcher-ownership.test.tsx`
- 修改测试：`test/ui-status.test.ts`

**RED**

- `/agent <request>` 进入 Agent 路径；普通文本仍进入 `turn.submit`。
- `/chat <request>` 始终强制纯聊天。
- `/continue` 只在存在可恢复 Agent Turn 时启用。
- UI 能显示 retrieval、permission prompt、tool request/result、checkpoint、fallback 和 final 状态。
- UI 不能执行工具、切模型或直接读取索引。

**GREEN**

- RuntimeEvent reducer 成为 Agent UI 唯一状态入口。
- 长工具输出折叠/截断，秘密与原始 reasoning 不显示。
- 保留现有 boot/init_failed/running/stopped 生命周期。

**验证**

- `npm exec -- tsx --test test/agent-ui-state.test.ts test/ui-command-boundary.test.ts test/ui-dispatcher-ownership.test.tsx test/ui-status.test.ts`
- 手工 UAT 只使用 fake Provider 和临时 workspace。

**Wave C 门禁**

- 全部 Agent loop、storage、permission、executor、interrupt、recovery 和 UI tests 通过。
- 第一轮 Executor 无通用 Shell、文件写入、Git 写入、安装、发布和网络能力。
- 所有 invocation 有 durable request/result 或明确 indeterminate 状态。
- 新 Session 权限重置；模型继续 sticky；两者状态文件与 API 互不引用。
- `/agent` 失败时 `/chat` 与普通 `turn.submit` 仍可使用。
- 门禁失败时关闭 Agent execution，仅保留 `/capabilities` report-only 和聊天。

## Wave D：切流、评估与发布门禁

### Task 21：加入独立 Feature Flags 和可逆产品切流

**文件**

- 新增：`src/config/feature-flags.ts`
- 修改：`src/config/config-manager.ts`
- 修改：`src/runtime/runtime-application.ts`
- 修改：`src/ui/chat-input-policy.ts`
- 修改：`src/ui/ui-state.ts`
- 新增测试：`test/feature-flags.test.ts`
- 新增测试：`test/agent-route-cutover.test.ts`
- 修改测试：`test/legacy-workspace-migration.test.ts`

**RED**

- `capabilityCatalog`、`capabilityEmbedding`、`agentExecution`、`agentDefaultRoute` 可独立关闭。
- 缺省/旧配置不会自动写入新字段，也不会改变普通输入行为。
- 关闭 embedding 保留 exact + BM25；关闭 Agent 保留 catalog；关闭 catalog 返回纯聊天。
- `agentDefaultRoute` 只在所有前置开关和门禁通过后才能启用。
- `/chat` 在任何组合下均强制纯聊天。

**GREEN**

- Feature flags 采用向后兼容默认值和显式启动诊断。
- 切流只改变输入路由，不迁移 Thread、模型状态、权限或索引数据。
- 每层故障可独立回退，不自动修改用户配置。

**验证**

- `npm exec -- tsx --test test/feature-flags.test.ts test/agent-route-cutover.test.ts test/legacy-workspace-migration.test.ts test/chat-input-policy.test.ts`
- 组合测试覆盖四个开关的允许状态，不测试无效组合为“成功”。

### Task 22：扩大评估集、Provider conformance 与最终验收

**文件**

- 新增：`src/eval/capability-retrieval-report.ts`
- 新增：`src/eval/provider-conformance.ts`
- 新增：`test/fixtures/capabilities/retrieval-cases-expanded.json`
- 新增：`test/fixtures/providers/conformance/`
- 修改：`package.json`
- 修改：`.github/workflows/ci.yml`
- 新增：`docs/verification/coding-agent-execution-plane.md`
- 修改测试：`test/ci-contract.test.ts`
- 修改测试：`test/provider-conformance.test.ts`

**RED**

- 普通输入切流前至少 150 条真实/人工复核案例满足召回门槛。
- 每个启用的 Adapter/Profile/Model 声明必须通过对应离线 conformance fixtures。
- CI 不能执行 live smoke、下载模型、读取真实 key 或依赖 embedding 资源存在。
- disabled capability path 的聊天延迟回归、远程请求数和启动行为可测量。
- 零容忍失败任一出现即阻止 `agentDefaultRoute`。

**GREEN**

- 增加确定性离线报告命令，分别报告 lexical、embedding、fused 和 no-resource fallback。
- 增加 Provider conformance runner：请求校验、streaming、tool calls、usage、completion、cancel、malformed/EOF、failure、redaction、feature fail-closed。
- 真实模型资源性能验收作为显式本地步骤；CI 使用固定 fake vectors，不伪报真实性能。
- 验证文档记录参考机器、命令、结果、已知限制和每层回退方法。

**验证**

最终验收顺序：

1. 运行所有定向契约测试。
2. 运行 `npm test`。
3. 运行 `npm run check`。
4. 运行 `npm run build`。
5. 运行 `git diff --check`。
6. 运行离线 retrieval eval 和 provider conformance。
7. 在明确安装的 Granite 资源包上运行本地性能验收；不在 CI 下载。
8. 使用用户自己的 key 做 live Provider smoke 之前再次请求明确确认；结果不写入 key 或原始 frame。
9. 只有全部发布门禁通过，才建议把普通输入默认切到 Agent；否则保持显式 `/agent`。

## 明确延期到后续计划

以下内容不进入本计划的运行时实现：

- 任意第三方 Tier-2 Adapter 代码加载、签名体系和自定义网络/凭证 transport。
- Grok 专用 Adapter；未来只通过已定义的 Provider/Profile/Model 契约接入。
- LangChain/LangGraph 核心迁移；未来复杂分支、多 Agent 或 HITL graph 可做隔离 spike。
- 多模型并行比较器。
- 任意 Shell、文件修改、Git 写操作、安装/发布、远程 MCP 和 Provider 托管搜索。
- 未具名项目的联网发现或自动安装。

这些延期项必须各自重新进行威胁建模、设计复核和实施计划，不能借 `full_access` 或高召回分数绕过。

## 实施结果

Task 1-22 已按 `Wave A -> Wave B -> Wave C -> Wave D` 顺序完成实现和确定性离线验证。真实 Granite 资源验收与 live Provider smoke 仍遵守上方的显式本地步骤；在它们完成之前，普通输入不自动切换到 Agent。
