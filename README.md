# MiniMax Codex

A first-pass Codex-style CLI shell for MiniMax.

## Run

```bash
npm install
npm run dev
```

Inside the CLI:

- `/new` creates a blank conversation, preserves the previous thread as
  history, and makes the new thread active.
- `/threads` lists stored conversations and marks the active thread.
- `/resume <threadId>` switches to a stored conversation, hydrates its history,
  and makes it the only active thread.
- `/api` changes the active provider's API key.
- `/provider` lists the active OpenAI-compatible provider.
- `/provider hashsight` switches to the Hashsight provider.
- `/provider minimax-official` switches back to the official MiniMax provider.
- `/trace` toggles the folded work trace panel.
- `/interrupt` cancels the active model request. Any received partial reply is
  saved as interrupted and is not reused as completed model context.
- `/compact` keeps the original JSONL transcript, writes a local summary with a
  coverage boundary, and replaces the covered messages in the next model-visible
  context with that summary.
- `/exit` quits.

Local runtime data is stored under `.mini-codex/`. API keys are excluded from
chat transcripts, trace logs, summaries, and git.

## Configuration, Credentials, and Recovery

Workspace configuration remains in `.mini-codex/config.json`, but it is
validated before use. Invalid provider IDs, protocols, URLs, storage drivers,
or context budgets fail with the affected field path instead of reaching the
Runtime as malformed state.

API keys use the system keychain when `keytar` is available. The fallback is a
user-level `credentials.json`, never the workspace:

- Windows: `%APPDATA%/minimax-codex/credentials.json`
- macOS: `~/Library/Application Support/minimax-codex/credentials.json`
- Linux: `${XDG_CONFIG_HOME:-~/.config}/minimax-codex/credentials.json`

`MINIMAX_CODEX_HOME` overrides that directory. A legacy
`.mini-codex/secrets.local.json` is migrated only after every stored provider
key has been written successfully, then the workspace copy is removed.

JSON snapshots are written through a same-directory temporary file, flushed,
and atomically renamed. The previous valid snapshot is kept as `.bak`. Startup
restores a valid backup when the primary config or thread index is damaged; if
both are invalid it reports the path and stops instead of silently resetting.
For append-only JSONL, only a malformed final line without a newline is treated
as an interrupted append and removed. Corruption in the middle remains a hard
error so history cannot disappear unnoticed.

## Command/Event Boundary

Ink is a view layer. Chat text is parsed into a typed `Command`,
`CommandDispatcher` routes it to the runtime, and the UI renders only
`RuntimeEvent` values. New workflow commands should be added to this protocol
boundary instead of calling `AgentRuntime` directly from `App.tsx`. Streamed
Turn events carry `turnId` so UI messages do not depend on local callback state.

The active thread is restored on startup. User and assistant messages are
loaded back into the UI, while Turn snapshots and streamed assistant deltas are
kept in append-only JSONL files under `.mini-codex/turns/`. If the previous
process stopped during a response, that Turn is marked `interrupted` on the
next startup and its saved partial reply is shown as incomplete. Interrupted
partial replies are never sent back to the model as completed context.

## Provider Pipeline and Safe Trace

Model calls are split into narrow layers:

```text
AgentRuntime
  -> ProviderModelAdapter
      -> ProviderProtocol (Responses or Chat Completions)
      -> HttpStreamTransport (fetch, abort, full-stream deadline)
```

The protocol layer owns request and SSE event shapes. The transport layer owns
network cancellation and timeout classification. Provider errors use stable
categories such as `authentication`, `rate_limit`, `timeout`, and `network`
before Runtime turns them into user-visible errors.

Raw reasoning fields and `<think>` blocks are removed inside the provider
boundary. Runtime receives only visible assistant deltas, token usage, and a
small set of structured diagnostics. Durable trace events are created from
known event codes with per-code fact allowlists; arbitrary prompts, response
bodies, API keys, and raw reasoning have no trace field through which they can
be persisted.

## OpenAI-Compatible Providers

The app follows the Codex-style provider split:

- `modelProvider` selects the active provider.
- `modelProviders` stores provider profiles.
- each provider owns its `baseUrl`, `protocol`, `envKey`, and default model.

Example:

```json
{
  "modelProvider": "hashsight",
  "modelProviders": {
    "hashsight": {
      "name": "Hashsight OpenAI Compatible",
      "baseUrl": "https://www.hashsight.cn/v1",
      "protocol": "chat_completions",
      "envKey": "HASHSIGHT_API_KEY",
      "defaultModel": "MiniMax-M3",
      "supportsThinkTags": true
    }
  }
}
```
