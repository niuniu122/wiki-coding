# MiniMax CLI 新建 Thread 实施计划

**日期：** 2026-07-10
**状态：** 已实施并验证
**范围：** 补齐完整多会话能力，为 CLI 增加 `/new`。

## 用户可观察结果

1. `/new` 创建一个全新的空白 thread，并立即切换过去。
2. 原 active thread 不删除，转为 archived，可通过 `/threads` 查看并用 `/resume` 切回。
3. 新 thread 没有继承旧消息、摘要、Turn 或 trace。
4. 新 thread 收到第一条用户消息后，仍沿用现有规则自动更新标题。
5. 任意时刻索引中仍然只有一个 active thread。
6. 模型正在回复时不能 `/new`，必须先 `/interrupt`。

## 存储规则

`JsonlStorageProvider.createThread()` 在创建 `status=active` 的新 thread 时，必须在同一次 index 写入中把其他 active thread 改为 archived。这样 `/new` 不需要“先归档旧会话、再创建新会话”两次写入。

## Runtime 顺序

```text
确认没有活动模型请求
→ 创建新的 ThreadRecord(active)
→ storage.createThread(newThread)
→ currentThread = newThread
→ 发出 thread.loaded + history.loaded([])
```

## UI

- 输入策略将 `/new` 解析成独立动作。
- 应用 Runtime 事件后，聊天区只保留欢迎信息，旧 trace 被清空。
- 状态栏显示新 thread ID。

## 测试

1. Storage 创建第二个 active thread 后恰好只有新 thread active。
2. Runtime `/new` 返回空 history，thread 数量增加，旧历史仍存在。
3. `/resume <oldId>` 能切回旧会话并恢复旧消息。
4. `/new` 在 busy 时被输入策略拒绝。
5. 原有压缩、恢复、中断和 thread 隔离测试继续通过。

## 非目标

- 不支持多个 active thread 同时运行。
- 不实现删除、重命名或永久归档。
- 不处理多个 CLI 进程同时写索引；文件锁属于后续可靠性阶段。

## 实施结果

- `JsonlStorageProvider.createThread()` 创建 active thread 时，会在同一次索引写入中归档旧 active thread。
- Runtime 新增 `newThread()`，活动请求期间拒绝新建；成功时返回新 thread 和空 history 事件。
- UI 输入策略将 `/new` 识别为独立会话动作，聊天区与 trace 随新 thread 清空。
- 新会话保留旧 thread 数据，可通过 `/threads` 查看并用 `/resume` 切回。
- README 和 CLI 帮助栏已加入 `/new`。
- 验证：33 个测试通过，类型检查和构建通过；新建-切回与唯一 active 创建专项探针通过。
