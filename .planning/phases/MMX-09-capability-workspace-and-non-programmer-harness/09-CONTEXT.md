# Phase 9 Context: Capability Workspace and Non-Programmer Harness

## User Intent

The user wants Wiki-Coding to treat open-source projects, Skills, and MCP servers as a separate tool workspace. The discovery harness is for non-programmers and should use embedding together with BM25, informed by the separation patterns in Codex and Claw.

## Locked Decisions

- `capabilities/` is the source-controlled external metadata boundary; `crates/tools` remains the fixed internal adapter set.
- Project, Skill, and MCP cards share one strict field contract but live in separate catalog files and typed indexes.
- Exact/BM25 is always available offline. Embedding is optional, verified, bounded, and candidate-only.
- Mutable install/authorization facts come from a separate inventory overlay, never from catalog source truth.
- Readiness has exactly three user-facing states: `ready`, `needs_install`, and `needs_authorization`.
- Search and prompt augmentation are read-only. They can describe a next action but cannot perform it.
- No new database, background daemon, Python/JavaScript RAG framework, model bundle, remote catalog fetch, credential read, or general plugin runtime is added.

## Workspace Boundary

```text
capabilities/
  README.md
  catalogs/
    projects.v1.json
    skills.v1.json
    mcp.v1.json

user runtime root (future installer/runtime milestone)
  catalogs/      verified downloaded metadata
  inventory/     installed and authorized IDs
  indexes/       rebuildable lexical/vector data
  installs/      versioned external packages
  sandboxes/     runtime-specific working state
```

The repository contains only the first block in this phase. The second block is a documented contract and an in-memory/read-only inventory input; no installer creates it yet.

## Readiness Precedence

1. Not installed and not bundled -> `needs_install`.
2. Installed/bundled but missing required authorization -> `needs_authorization`.
3. Installed/bundled with all required authorization -> `ready`.

This order prevents an item that still needs installation from misleadingly asking for credentials first.
