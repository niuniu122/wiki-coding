# MiniMax CLI 会话恢复实施计划

**日期：** 2026-07-10
**状态：** 已实施并验证
**范围：** 稳定化第二阶段的第一小片——持久化 Turn、恢复异常中断、启动时加载旧消息。主动取消正在进行的网络请求在下一小片处理。

## 用户可观察结果

1. CLI 关闭再打开后，当前 active thread 的旧用户消息和助手回复重新出现在界面。
2. 每个 Turn 都保存 `running` 到 `completed`、`failed` 或 `interrupted` 的生命周期。
3. 流式回复的增量单独追加保存；进程异常退出时，已收到的部分回复仍可恢复。
4. 启动时发现遗留 `running` Turn，会将其标记为 `interrupted`，不会继续假装请求仍在运行。
5. 被中断的助手草稿只用于界面恢复，不进入下一次模型上下文，也不能成为压缩边界。
6. 第二次启动不会重复制造恢复消息。

## 数据设计

Turn 使用单独的追加式 JSONL 事件文件：

```ts
type StoredTurnEvent =
  | {kind: "turn.snapshot"; turn: TurnRecord}
  | {kind: "assistant.delta"; threadId: string; turnId: string; delta: string; createdAt: string};
```

读取时按顺序重放事件：最新 snapshot 决定 Turn 状态，所有 delta 拼成 `assistantDraft`。这样每个流式片段只写一次，避免每次都重写越来越大的完整草稿。

## Runtime 启动顺序

```text
加载配置与存储
→ 找到 active thread
→ 读取并恢复遗留 running Turn
→ 将已保存草稿写成带 interrupted/partial 标记的展示消息
→ 读取当前 thread 历史
→ 发出 thread.loaded、history.loaded、turn.recovered 事件
```

## 测试顺序

1. Storage 能重放 Turn snapshot 和 assistant delta。
2. 正常提交后 Turn 状态是 completed，草稿与最终回复一致。
3. 模拟进程崩溃后重新创建 Runtime：running 变为 interrupted，用户消息和部分回复被加载。
4. 第二次重启不会重复生成部分回复。
5. partial/interrupted assistant message 不进入模型上下文和本地摘要。
6. UI 历史格式化保留角色，并明确标出未完成回复。

## 非目标

- 本小片不实现 `/resume <threadId>`、新建/切换 thread。
- 本小片不实现真正的网络 `AbortController` 中断；这是紧接着的下一小片。
- 不删除或迁移已有 session JSONL；旧会话仍按原格式读取。
- 不引入 SQLite 或新的第三方依赖。

## 实施结果

- 新增 `.mini-codex/turns/<threadId>.turns.jsonl` 追加事件流。
- Runtime 在 Turn 开始、流式 delta、完成、失败和启动恢复时写入生命周期数据。
- 启动时会将遗留 `running` Turn 改为 `interrupted`，并恢复已保存的部分回复。
- 正常失败前已经收到的部分回复也会被保存并明确标注。
- `history.loaded` 将当前 active thread 的旧消息交给 UI；`turn.recovered` 报告恢复结果。
- partial assistant message 不进入模型上下文、本地摘要或压缩边界。
- 重复启动具有幂等性，不会重复制造恢复消息。
- 验证：23 个测试通过，类型检查通过，构建通过，连续启动两次的专门恢复探针通过。
