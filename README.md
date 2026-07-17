# MiniMax Codex

A local-first, Codex-style command-line agent for MiniMax and compatible
Providers. The supported product runtime is Rust. It combines a typed agent
loop, bounded workspace tools, a BM25-first project/Skill/MCP capability
workspace, and an Obsidian-compatible per-project Vault. SQLite is not used.

## Install and run

Supported release platforms are Windows x64 MSVC and Linux x64 GNU. Verify the
published SHA-256 sidecar before using either distribution:

- the versioned base archive contains the launcher, one native Rust binary,
  release manifest, documentation, and licenses;
- the platform npm package additionally contains the explicit TypeScript legacy
  command in the same installable artifact.

After extraction or platform npm installation:

```bash
minimax-codex doctor
minimax-codex run --prompt "inspect this project"
minimax-codex chat
```

Linux confirm-mode process tools require the small system `bubblewrap` package.
Install it with your distribution package manager before running `doctor`. If a
real sandbox is missing or unusable, process tools fail before project code
starts; they never silently fall back to ordinary host execution. Windows keeps
the CLI, Provider, file tools, retrieval, Vault, and Wiki features, but
confirm-mode Cargo/Git/npm diagnostics fail closed until a native Windows
sandbox backend is shipped.

`minimax-codex` always launches the fixed sibling Rust binary without a shell,
download, `PATH` search, or silent fallback. `minimax-codex-legacy` is the
operator-selected TypeScript fallback during the documented support window.

See [installation, upgrade, and rollback](docs/release/install-upgrade-rollback.md)
and the [cutover contract](docs/release/cutover.md) before migration or rollout.

## What happens during a normal run

The Rust runtime validates configuration, acquires one project writer lease,
and records the session as replayable append-only evidence. Workspace reads are
bounded by default. Writes and commands follow exactly two permission modes:

- `confirm`: ask before an effect that requires approval;
- `full-access`: allow effects for the current process only.

Approval and isolation are separate. In `confirm`, process tools also require an
OS-enforced sandbox: Linux uses Bubblewrap with child networking denied and only
the project workspace writable. In `full-access`, the prompt and subprocess
sandbox are explicitly disabled for trusted projects, but schema, path, secret,
destructive-operation, size, timeout, output, and cancellation gates remain.
Provider HTTPS is host-owned and is not placed inside the child sandbox.

Restart always returns to `confirm`. See the
[subprocess sandbox and platform boundary](docs/release/subprocess-sandbox.md).

There is no persistent global “always allow” switch. Unknown or interrupted
side effects are not replayed automatically.

When a durable session is finalized, a separate strict call to the same pinned
main model may propose Wiki updates. Only the bounded visible transcript,
durability markers, current pages, and validation context are eligible; tool
output and private reasoning are excluded. The local validator commits accepted
Markdown transactionally into the project Vault. Lookup-only sessions are a
no-op and spend no Wiki-generation call.

The first run creates a stable project-to-Vault binding at
`.minimax/vault-binding.v1.json`. Unless explicitly chosen before that first
binding, the Vault is a sibling directory recommended by the runtime. It stays
plain Markdown and JSON so Obsidian and ordinary file tools can inspect it.

## Project, Skill, and MCP discovery

External metadata is kept under the dedicated source-only [`capabilities/`](capabilities/README.md)
workspace, not in the fixed executable adapters under `crates/tools`.
Non-programmers do not need to prepare a catalog flag. Ordinary agent prompts
that ask for an open-source project, Skill, MCP server, library, or tool
automatically receive bounded read-only discovery context:

1. Three typed project, Skill, and MCP indexes run exact/BM25 first.
2. An optional, separately installed and hash-verified embedding resource may
   rerank only the bounded lexical candidate union.
3. Missing, incompatible, or unhealthy embeddings leave the BM25 order intact.
4. A separate local inventory overlay reports `ready`, `needs_install`, or
   `needs_authorization` and prints a safe next action.

Discovery never downloads, installs, authorizes, or executes a candidate.
Catalogs contain source facts only; they do not contain credentials, mutable
installed flags, shell commands, or process state. The embedded catalogs are
the zero-configuration default; `--catalog-root` and `--inventory` are strict
read-only expert overrides for the workspace command.

Read-only inspection is also available directly:

```bash
minimax-codex index capabilities status
minimax-codex index workspace status
minimax-codex index workspace search "帮我找一个管理 GitHub issue 的 MCP" --kind mcp
minimax-codex index workspace search "查 OpenAI API 官方文档" --kind skill
minimax-codex index projects search "本地知识库命令行工具"
minimax-codex index wiki search "release decision" --vault <path> --project-id <id>
```

The base distribution never bundles or downloads model weights. See the
[capability workspace guide](docs/capability-workspace.md) and the
[optional embedding package contract](docs/release/embedding-package.md).

## Vault maintenance and migration

Vault maintenance is report-first and narrow: status/lint are read-only;
repair and rebuild are allowlisted; garbage collection, purge, and privacy
forget require action-specific plan-bound confirmations. Referenced raw evidence
is preserved, trash can be undone before purge, and there is no `--force` path.

TypeScript data migration follows the same rules:

```bash
minimax-codex migrate inventory
minimax-codex migrate dry-run --json
minimax-codex migrate apply --plan <plan> --confirmation <printed-value>
minimax-codex migrate verify --receipt <receipt>
```

Inventory and dry-run write nothing. Apply stages and verifies allowlisted
artifacts before commit, preserves every source byte, and writes an immutable
receipt. Rollback removes only unchanged targets created by that receipt.
Credentials, private reasoning, caches, locks, databases, and unknown records
are excluded.

## Architecture

The Rust workspace keeps authority behind typed boundaries:

```text
CLI/TUI
  -> core agent and permission policy
      -> Provider adapter
      -> bounded tools
      -> runtime journal
      -> Vault/Wiki workflow
      -> retrieval kernel (exact/BM25, optional candidate rerank)
          -> dedicated project/Skill/MCP source catalogs
          -> separate read-only readiness inventory
```

Provider adapters normalize Responses and Chat Completions streams into one
protocol. Success requires the protocol terminal event; malformed frames,
premature EOF, duplicate completion, and content after completion fail closed.
Raw reasoning and `<think>` blocks are removed at the Provider boundary.

The Vault crate owns Markdown parsing and transactions but has no Provider,
HTTP, credential, SQLite, or model-download path. Wiki generation is a narrow
port supplied by the CLI with the exact session model binding.

During the legacy support window, the TypeScript reference keeps its own typed
boundary: `ApplicationKernel` owns command concurrency and
`StrictProviderGateway` owns protocol and transport validation. Its Ink view
dispatches typed commands and renders runtime events; it does not own Provider
or tool authority. These names document the explicit legacy implementation,
not the Rust default entry.

## Source development

Rust 1.97.0 and Node.js 20 are pinned for the complete compatibility and release
gate:

```bash
npm ci
npm run check
npm test
npm run check:rust
npm run test:rust
npm run verify:rust-contracts
npm run build
```

`npm run dev` runs the legacy TypeScript reference for development; it is not
the default product entry. `npm run smoke:provider` is the only live Provider
smoke command and must be invoked explicitly with separate authorization. CI
has no Provider credentials and uses fixtures only.

`npm run check` and `npm run build` compile the smoke source but do not execute
it. Offline tests statically inspect the smoke safety boundary. Automated
scripts never invoke `npm run smoke:provider`; only the operator can choose the
live command.

Release verification deterministically packages both distributions, extracts
and starts the actual packaged Rust default, verifies the legacy mapping, checks
licenses/security/size/startup/RSS/Wiki search budgets, and records a product
fingerprint. Any tracked product input change invalidates older hosted evidence.

## License

Licensed under either [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at your
option.
