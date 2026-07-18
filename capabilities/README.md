# Capability Workspace

This directory is the source-controlled, metadata-only workspace for external
capabilities. It is deliberately separate from `crates/tools`, which contains
the fixed internal adapters that the agent can execute.

## Catalogs

- `catalogs/projects.v1.json` describes ordinary open-source projects.
- `catalogs/skills.v1.json` describes instruction/resource packages that an
  agent can load progressively.
- `catalogs/mcp.v1.json` describes Model Context Protocol servers.

All three files use the same strict card fields but declare one `kind` and are
loaded into different Rust document/index types. Catalog facts are immutable
source evidence: they may describe installation guidance, required
authorization names, or permissions, but they must never contain a credential,
an installed flag, an executable command, or process state.

The bundled Skill seed is sourced from the official
[OpenAI Skills catalog](https://github.com/openai/skills). The bundled MCP seed
is sourced from the official
[GitHub MCP Server](https://github.com/github/github-mcp-server). Unknown
license, platform, permission, authorization, or maintenance facts are omitted
rather than inferred.

## Runtime Boundary

A future explicitly confirmed installer/runtime may use a user-level layout
such as:

```text
capabilities-runtime/
  catalogs/     verified downloaded metadata
  inventory/    installed and authorized capability IDs
  indexes/      rebuildable lexical and vector data
  installs/     versioned external packages
  sandboxes/    runtime-specific process state
```

That mutable runtime does not live in this directory. Phase 9 accepts an
immutable inventory overlay for readiness calculation but does not create an
installer, request authorization, download a model, or start a project, Skill,
or MCP server.

## User-Facing Readiness

Readiness is derived in this order:

1. An external item not present in inventory is `needs_install`.
2. An installed or bundled item with unmet declared authorization is
   `needs_authorization`.
3. An installed or bundled item with no unmet declared authorization is
   `ready`.

Every next action is guidance only. The calling workflow must still show the
source and request explicit confirmation before it performs any side effect.
