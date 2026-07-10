# MiniMax CLI 主动中断实施计划

**日期：** 2026-07-10
**状态：** 已实施并验证
**范围：** 稳定化第二阶段的第二小片——让 `/interrupt` 真正取消正在进行的模型网络请求，并把 Turn 保存为 `interrupted`。

## 用户可观察结果

1. 模型正在回复时，输入框仍允许输入 `/interrupt`。
2. `/interrupt` 会调用当前请求的 `AbortController.abort()`，而不只是隐藏 loading 文案。
3. Runtime 立即报告取消请求已收到；网络流结束后报告 Turn 已中断。
4. 已收到的部分回复会保存为 partial/interrupted，下一次启动仍能看到。
5. 主动中断不显示为供应商错误，也不把 Turn 标成 `failed`。
6. 没有活动请求时输入 `/interrupt` 返回温和的 no-op 状态。
7. 原有 300 秒网络超时仍然保留，并与用户主动取消区分。

## 数据流

```text
UI /interrupt
→ Runtime.interruptCurrentTurn()
→ 当前 AbortController.abort()
→ ModelAdapter 的 fetch 收到外部 AbortSignal
→ Runtime 捕获主动取消
→ 保存部分回复
→ Turn = interrupted
→ turn.interrupted 事件
→ UI 结束 busy 状态并标注已取消
```

## 接口调整

```ts
interface ModelAdapter {
  streamResponse(params: {
    config: AppConfig;
    apiKey: string;
    messages: ModelContextMessage[];
    signal?: AbortSignal;
  }): AsyncGenerator<ModelAdapterEvent>;
}
```

Runtime 只允许取消自己持有的当前请求。ModelAdapter 负责把 Runtime 的外部 signal 与内部超时合并，并在异常时区分“用户取消”和“请求超时”。

## 测试

1. 忙碌时普通输入被拒绝，但 `/interrupt` 被路由为取消动作。
2. 可阻塞 FakeModelAdapter 收到 signal；取消后生成 `turn.interrupted`，不生成 `error`。
3. Turn 持久状态为 `interrupted`，已有草稿保存为 partial/interrupted。
4. 中断完成后再次取消返回 no-op。
5. MiniMaxModelAdapter 的真实 fetch 层响应外部 AbortSignal，并保留 AbortError 供 Runtime 分类。
6. 原有压缩、恢复、正常完成和失败测试继续通过。

## 非目标

- 本小片不实现多个并发 Turn。
- 不增加 Escape/Ctrl+C 快捷键，先提供可测试的 `/interrupt`。
- 不实现 thread 切换或恢复指定 thread。
- 不改变供应商协议解析和 reasoning/trace 设计。

## 实施结果

- `ModelAdapter.streamResponse` 接受可选 `AbortSignal`；MiniMax 适配器将外部 signal 转发到 fetch，并保留原有 300 秒超时。
- 外部取消保留 AbortError，不再被误译成供应商超时。
- Runtime 持有当前模型请求的 `AbortController`，提供幂等的 `interruptCurrentTurn()`。
- 活动请求被取消后，Turn 保存为 `interrupted`；已有草稿保存为 partial/interrupted，不生成 error item。
- 无活动请求时返回 `turn.interrupt.ignored`，不修改持久状态。
- Ink 忙碌时输入框保持可用，但输入策略只放行 `/interrupt`，阻止第二个用户 Turn。
- 当前界面和重启后的历史都会明确标注已取消的部分回复。
- 验证：27 个测试通过，类型检查和构建通过；Runtime active-interrupt 与适配器 fetch-abort 专项探针通过。
