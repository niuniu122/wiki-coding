# MiniMax Codex

MiniMax Codex is a local-first, Codex-style command-line agent for MiniMax and
compatible Providers. Rust is the sole product runtime and owns all product,
test, compatibility, Provider-evaluation, and retrieval-evaluation behavior.
The only JavaScript in the distribution is the thin CJS launcher and MJS
release orchestration. TypeScript-era files under `fixtures/compat/` are static
migration and compatibility data; they are never built or executed.

The product combines a typed agent loop, bounded workspace tools, BM25-first
project/Skill/MCP discovery, and an Obsidian-compatible per-project Vault.
SQLite is not used.

## Supported releases

The supported release targets are exactly:

- Windows x64 MSVC: `windows-x86_64-msvc` / `x86_64-pc-windows-msvc`;
- Linux x64 GNU: `linux-x86_64-gnu` / `x86_64-unknown-linux-gnu`.

`windows-x86_64-gnullvm-dev` is local development evidence only. It is not a
supported Windows release and cannot satisfy hosted MSVC verification. macOS
and other architectures are not supported by this release.

Each platform candidate contains one Rust binary, the shell-free
`bin/minimax-codex.cjs` launcher, documentation, licenses, and a strict release
manifest. No package contains a TypeScript runtime, fallback command, embedding
model, or install-time download.

## Install and run

Verify the archive or npm tarball against its matching `.sha256` sidecar before
installation. Common local installation paths are:

```bash
# Install the already-downloaded platform npm tarball globally.
npm install --global --ignore-scripts ./minimax-codex-v0.1.0-<target>-npm.tgz
minimax-codex --version

# Run the already-downloaded platform npm tarball once without a global install.
npx --offline --yes --package ./minimax-codex-v0.1.0-<target>-npm.tgz minimax-codex doctor
```

For a native installation, extract the matching base `.tar.gz` into a
versioned directory, verify `RELEASE-MANIFEST.json`, then run
`minimax-codex.exe` on Windows or `./minimax-codex` on Linux. Keep the previous
versioned directory until the new version has passed `doctor` and normal work.

After any installation:

```bash
minimax-codex doctor
minimax-codex run --prompt "inspect this project"
minimax-codex chat
```

The npm command launches only the fixed sibling Rust binary. It never searches
`PATH`, reads a binary override, invokes a shell, downloads a runtime, or falls
back to another implementation. Launcher failures use stable categories:
`E_UNSUPPORTED_HOST`, `E_BINARY_MISSING`, `E_BINARY_UNSAFE`,
`E_BINARY_NOT_EXECUTABLE`, `E_START_FAILED`, and `E_SIGNAL_TERMINATION`.
Reinstall the correct package for a supported target; there is no legacy route.

See [installation, upgrade, and rollback](docs/release/install-upgrade-rollback.md)
and the [Rust-only cutover contract](docs/release/cutover.md) before rollout or
migration.

## Permission and subprocess boundaries

Workspace reads are bounded by default. Writes and commands use exactly two
permission modes:

- `confirm`: ask before each effect that requires approval;
- `full-access`: allow effects for the current process only.

Approval and subprocess isolation are separate. In `confirm`, Linux process
tools require Bubblewrap with child networking denied and only the project
workspace writable. If the backend is missing or unusable, the process fails
before project code starts. Windows supports the CLI, Provider, file tools,
retrieval, Vault, and Wiki, but confirm-mode Cargo/Git/npm diagnostics fail
closed because this release does not ship a native Windows sandbox backend.

In `full-access`, approval prompts and subprocess isolation are disabled for a
project you already trust. Schema, path, secret, destructive-operation, size,
timeout, output, and cancellation gates remain active. Restart always returns
to `confirm`; there is no persistent “always allow” setting. Provider HTTPS is
host-owned and is not placed inside the child sandbox.

Read the [subprocess sandbox and platform boundary](docs/release/subprocess-sandbox.md)
before testing an unfamiliar repository.

## Sessions, Vault, and Wiki

The Rust runtime validates configuration, acquires one project writer lease,
and records each session as replayable append-only evidence. Unknown or
interrupted side effects are not replayed automatically.

When a durable session is finalized, a separate strict call to the same pinned
main model may propose Wiki updates. Only the bounded visible transcript,
durability markers, current pages, and validation context are eligible; tool
output and private reasoning are excluded. The local validator commits accepted
Markdown transactionally. Lookup-only sessions are a no-op and spend no Wiki
generation call.

The first run creates `.minimax/vault-binding.v1.json`. Unless explicitly
selected before that binding, the Vault is a sibling directory recommended by
the runtime. It remains plain Markdown and JSON for Obsidian and ordinary file
tools.

## Project, Skill, and MCP discovery

External metadata lives under [`capabilities/`](capabilities/README.md), not in
the fixed executable adapters under `crates/tools`. Ordinary prompts that ask
for an open-source project, Skill, MCP server, library, or tool automatically
receive bounded read-only discovery context:

1. Typed project, Skill, and MCP indexes run exact/BM25 retrieval first.
2. An optional, separately installed and hash-verified embedding resource may
   rerank only the bounded lexical candidate union.
3. Missing, incompatible, or unhealthy embeddings preserve BM25 order.
4. A local inventory reports `ready`, `needs_install`, or
   `needs_authorization` and prints a safe next action.

Discovery never downloads, installs, authorizes, or executes a candidate. The
base distribution never bundles or downloads model weights. Direct inspection
is available through:

```bash
minimax-codex index capabilities status
minimax-codex index workspace status
minimax-codex index workspace search "find a GitHub issue MCP" --kind mcp
minimax-codex index workspace search "find an API documentation skill" --kind skill
minimax-codex index projects search "local knowledge-base CLI"
minimax-codex index wiki search "release decision" --vault <path> --project-id <id>
```

See the [capability workspace guide](docs/capability-workspace.md) and the
[optional embedding package contract](docs/release/embedding-package.md).

## TypeScript-era data migration

Migration is explicit, source-preserving, receipt-bound, and narrowly
reversible:

```bash
minimax-codex migrate inventory
minimax-codex migrate dry-run --json
minimax-codex migrate apply --plan <plan> --confirmation <printed-value>
minimax-codex migrate verify --receipt <receipt>
minimax-codex migrate rollback --receipt <receipt> --confirmation ROLLBACK:<receipt-hash>
```

Inventory and dry-run write nothing. Apply stages and verifies allowlisted
artifacts, preserves every `.mini-codex` source byte, and writes an immutable
receipt. Rollback removes only unchanged targets marked `created` by that
receipt. Reused or modified files, the receipt, and source data remain.
Credentials, private reasoning, caches, locks, databases, and unknown records
are excluded. There is no `--force` path.

Static TypeScript-era migration fixtures remain supported until at least two
distinct, ordered public releases after cutover release `3.0.0` have been
recorded by the machine-checkable support-window fixture. This is a data
compatibility window, not an executable legacy runtime.

## Architecture

The Rust workspace keeps authority behind typed boundaries:

```text
CLI/TUI
  -> core agent, session, and permission policy
      -> Provider adapter
      -> bounded tools
      -> runtime journal
      -> Vault/Wiki workflow
      -> retrieval kernel (exact/BM25, optional candidate rerank)
          -> project/Skill/MCP source catalogs
          -> read-only readiness inventory
```

Provider adapters normalize Responses and Chat Completions streams into one
protocol. Malformed frames, premature EOF, duplicate completion, and content
after completion fail closed. Raw reasoning and `<think>` blocks are removed at
the Provider boundary.

The Vault crate owns Markdown parsing and transactions but has no Provider,
HTTP, credential, SQLite, or model-download path. The compatibility harness
uses immutable data fixtures and production Rust APIs; it does not build or run
a second product implementation.

## Source development and release verification

Rust 1.97.0 and Node.js 20 or newer are required. Node is used only for the npm
launcher, packaging, and release orchestration:

```bash
npm ci
npm run check:rust
npm run test:rust:candidate   # while the checked-in hosted record is stale
npm run eval:provider
npm run eval:retrieval
npm run verify:rust-contracts:candidate
npm run test:package
npm run build:rust:release
```

After candidate evidence is committed with `strictStatus: pending`, ordinary
push CI uses `npm run test:rust:strict-precondition` and
`npm run verify:rust-contracts:strict-precondition` to validate that exact
candidate record without pretending the later strict run already exists. Final
`npm run test:rust` and `npm run verify:rust-contracts` require the combined
candidate-plus-strict record to match the current product fingerprint.
CI has read-only repository permissions, no Provider credentials, and no live
Provider command. Candidate and strict jobs both run Rust checks, evaluations,
compatibility, migration, corruption tests, packaging, installed smoke,
checksums, licenses, security, and performance gates, then upload their exact
per-target evidence artifacts.

Product fingerprint v3 hashes current tracked and untracked product bytes,
including source, configuration, fixtures, launcher, release scripts, and docs.
Only `.planning/**` and the hosted evidence record itself are excluded. Any
product edit invalidates older artifacts and hosted evidence.

## License

Licensed under either [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at your
option.
