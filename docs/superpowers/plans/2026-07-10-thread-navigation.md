# MiniMax CLI 历史 Thread 导航实施计划

**日期：** 2026-07-10
**状态：** 已实施并验证
**范围：** 稳定化第二阶段最后一小片——列出历史 thread、选择一个 thread 作为唯一 active 会话，并恢复它的历史与遗留 Turn。

## 用户可观察结果

1. `/threads` 显示历史会话的 ID、状态、标题和最后更新时间。
2. `/resume <threadId>` 切换到指定会话，并用目标会话的历史替换当前聊天区。
3. 切换后新问题只使用目标 thread 的消息和摘要，不能混入原 thread。
4. 任意时刻索引中最多只有一个 `active` thread。
5. 切换到含有遗留 `running` Turn 的历史会话时，沿用现有恢复规则将其改成 `interrupted`。
6. 不存在的 threadId 返回明确错误，当前 active thread 保持不变。

## 存储规则

切换 active thread 不能由 Runtime 连续调用两次 `updateThread`，否则进程可能在两次写入之间退出，留下零个或两个 active thread。StorageProvider 增加一个单次索引变换：

```ts
activateThread(threadId: string, activatedAt: string): Promise<ThreadRecord | null>;
```

它在内存里完成以下变换，再一次写回 threads index：

- 目标 thread → `active`，更新时间为 `activatedAt`；
- 其他 active thread → `archived`；
- 目标 thread 放到索引首位；
- 找不到目标时不写文件并返回 `null`。

## Runtime 顺序

```text
确认没有活动模型请求
→ storage.activateThread(target)
→ currentThread = target
→ 恢复目标 thread 的 stale running Turns
→ 读取目标历史
→ 发出 thread.loaded + history.loaded + recovery events
```

## UI 命令

- `/threads`：只展示列表，不改变当前 thread。
- `/resume <threadId>`：执行切换；缺少 ID 时显示使用方式。
- thread.loaded 时清空旧 trace 面板，history.loaded 用目标历史替换聊天区。

## 测试

1. Storage 激活目标后恰好一个 active thread，目标排在首位。
2. 不存在的 ID 不改变索引。
3. Runtime resume 返回的 history 只包含目标 thread。
4. 目标 stale running Turn 在 resume 时恢复为 interrupted。
5. resume 后提交新问题，模型上下文包含目标历史、不包含原 thread 历史。
6. UI thread 列表格式稳定并清楚标识当前会话。

## 非目标

- 本小片不新增 `/new`、删除或永久归档会话。
- 不实现并发 Turn。
- 不在这里完成完整 Command dispatcher；这是下一稳定化阶段。
- 不修改 JSON 索引的原子落盘方式；文件级原子写入属于第五阶段。

## 实施结果

- StorageProvider 新增 `activateThread`，在一次索引变换中激活目标并归档其他 active thread。
- 不存在的 ID 返回 `null`，不写入索引。
- Runtime 新增 `listThreads()` 与 `resumeThread(threadId)`；resume 复用现有 stale Turn 恢复流程。
- `resumeThread` 返回目标的 `thread.loaded`、`history.loaded` 和恢复事件；活动模型请求期间拒绝切换。
- UI 新增 `/threads` 与 `/resume <threadId>`，切换时替换聊天历史并清空旧 trace。
- 切换后的下一次模型调用只包含目标 thread 历史和摘要。
- 验证：31 个测试通过，类型检查和构建通过；唯一 active 与跨 thread 上下文隔离专项探针通过。
