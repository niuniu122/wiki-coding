# Capability Workspace and Non-Programmer Harness

## 用一句话理解

Wiki-Coding 现在把“系统自己会执行的内部工具”和“从外部发现的项目、Skill、MCP”分开了。

- `crates/tools` 是内部工具箱，里面的适配器才可能进入 Agent 的执行流程。
- `capabilities/catalogs` 是外部能力目录，只保存可核查的介绍和来源。
- 本地 inventory 是另一张只读清单，说明某个能力是否已安装、是否已授权。

搜索结果不是执行许可。系统可以告诉你下一步是什么，但不会因为“推荐了它”就自动下载、安装、索取密钥或启动进程。

## 为什么分成三类目录

项目、Skill 和 MCP 看起来都像“工具”，实际含义不同：

| 类型 | 它是什么 | 典型下一步 |
|------|----------|------------|
| Project | 普通开源软件或命令行项目 | 审查来源和安装文档 |
| Skill | 给 Agent 的说明、脚本和资源包 | 审查 Skill 目录后安装到兼容宿主 |
| MCP | 通过协议暴露工具或数据的服务器 | 审查安装方式、权限和授权范围 |

三者使用同一张严格的 capability card，但分别进入 project、skill、mcp 三种 Rust 文档类型和索引。这样可以复用 BM25 与 embedding 算法，又不会把 Skill 当成 MCP，或把一个外部项目混入内部命令索引。

## 检索顺序

```text
用户自然语言
  -> 选择全部类型或一种类型
  -> 各类型 exact/BM25 召回
  -> 合并最多 20 个候选
  -> 可选的已验证 embedding 只重排这些候选
  -> 输出来源、匹配词、实际模式和准备状态
```

Embedding 不能新增 BM25 没有召回的条目。模型缺失、指纹不符、超时、崩溃、维度错误、NaN 或返回陌生 ID 时，系统保留原来的 BM25 结果，并给出稳定的 degraded reason。

## 三种准备状态

状态按固定顺序计算：

1. 外部能力不在 inventory 中：`needs_install`。
2. 已安装或随产品提供，但声明的授权尚未满足：`needs_authorization`。
3. 已安装或随产品提供，且没有未满足的已声明授权：`ready`。

“没有声明授权”不等于“保证不需要任何权限”。目录中未核实的 license、platform、permission、authorization 和 maintenance 字段会显示为 unknown，不会被猜测补齐。

## 命令

```bash
# 查看三类目录的数量、来源和指纹
minimax-codex index workspace status

# 搜索全部类型
minimax-codex index workspace search "我需要查官方 API 文档"

# 只搜索一种类型
minimax-codex index workspace search "管理 GitHub issue" --kind mcp

# 专家只读覆盖：固定读取目录下的三个 v1 文件和一份 inventory
minimax-codex index workspace search "管理 GitHub issue" \
  --catalog-root <catalog-directory> \
  --inventory <inventory.v1.json>
```

Inventory v1 示例：

```json
{
  "schemaVersion": 1,
  "installed": ["mcp:github/github-mcp-server"],
  "authorized": []
}
```

命令没有 `--install`、`--authorize`、`--execute` 或 `--start` 开关。这些动作需要以后单独设计带来源校验、权限说明、用户确认、回滚和沙箱的工作流。

## 源码区与未来运行区

源码仓库只拥有：

```text
capabilities/
  README.md
  catalogs/
    projects.v1.json
    skills.v1.json
    mcp.v1.json
```

未来的运行区应位于用户级数据目录，并把 verified catalogs、inventory、rebuildable indexes、versioned installs 和 sandboxes 分开。Phase 9 只锁定这个边界，不创建安装器，也不改变现有 confirm/full-access 权限和 subprocess sandbox 规则。
