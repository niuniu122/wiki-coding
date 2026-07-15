# MiniMax Codex

A contract-first, Codex-style interactive CLI for MiniMax and compatible
Providers. The CLI keeps Ink at the view boundary while the runtime kernel owns
session, Provider, context, storage, and command-lifecycle policy.

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
- `/retry` is available after startup initialization fails. It retries the same
  runtime and dispatcher; duplicate retries are coalesced while one is running.
- `/exit` quits.

Local runtime data is stored under `.mini-codex/`. API keys are excluded from
chat transcripts, trace logs, summaries, and git.

## Runtime Architecture and Ownership

`ApplicationKernel` is the single connection point between typed commands and
runtime events. It composes the workspace lease, command arbiter,
`ProviderService`, `SessionService`, `TurnEngine`, context engine, and JSONL
repository. The UI reduces runtime events into display state and does not own
Provider or command-concurrency decisions.

Only one live CLI process may own a `.mini-codex` workspace. Startup acquires an
atomic, nonce-protected lease before initializing services. A second live
process is rejected with the owning PID. If the owner process is no longer
alive, startup safely replaces the stale lease; an older process cannot later
delete the replacement lease.

## Configuration, Credentials, and Recovery

Workspace configuration remains in `.mini-codex/config.json`, but it is
validated before use. Invalid provider IDs, protocols, URLs, storage drivers,
or context budgets fail with the affected field path instead of reaching the
Runtime as malformed state.

API keys use the system keychain when the optional `@napi-rs/keyring` backend
is available. Plaintext fallback requires an explicit warning and confirmation,
and writes to a user-level `credentials.json`, never the workspace:

- Windows: `%APPDATA%/minimax-codex/credentials.json`
- macOS: `~/Library/Application Support/minimax-codex/credentials.json`
- Linux: `${XDG_CONFIG_HOME:-~/.config}/minimax-codex/credentials.json`

`MINIMAX_CODEX_HOME` overrides that directory. A legacy
`.mini-codex/secrets.local.json` entry is migrated only for an exact built-in
Provider target. With a working keyring, startup writes and verifies the scoped
key before removing the workspace source. If the keyring is unavailable,
startup keeps the source untouched, emits a secret-free re-entry warning, and
continues; it never converts that old secret into plaintext automatically.
Custom and redirected Provider targets never inherit provider-ID-only legacy
keys.

Every keychain and plaintext record is scoped to the Provider ID, canonical
endpoint, and authentication scheme. Changing `baseUrl` therefore requires a
new key. Workspace `envKey` values are honored only when the complete Provider
target exactly matches a built-in definition, so a repository cannot redirect
a trusted environment credential to another server.

When the OS keyring is unavailable, `/api` displays the absolute plaintext
credential path. Type exactly `YES` to grant one-time consent and continue to
API-key entry. Any other response cancels without saving. Consent is consumed
by one save attempt and is never inferred or remembered silently. After a new
scoped key is saved, pending legacy workspace files are cleaned up. If no save
occurs, they remain for a later startup or explicit re-entry.

JSONL is the only supported transcript and Turn storage format; SQLite
configuration is rejected during migration. New records use versioned
envelopes and per-file monotonic sequences. A version-0 workspace is validated
before replacement, every changed legacy file retains its original bytes in
`.v0.bak`, and
`manifest.json` is committed last with `schemaVersion: 1`. Legacy month-level
session paths remain readable and appendable after migration. Unknown future
versions fail closed.

JSON snapshots are written through a same-directory temporary file, flushed,
and atomically renamed. The previous valid snapshot is kept as `.bak`. Startup
restores a valid backup when the primary config or thread index is damaged; if
both are invalid it reports the path and stops instead of silently resetting.
For append-only JSONL, only a malformed final line without a newline is treated
as an interrupted append and removed. Corruption in the middle remains a hard
error so history cannot disappear unnoticed. Visible assistant deltas are
batched, and terminal Turns are checkpointed to one latest snapshot per Turn.

## Command/Event Boundary

Ink is a view layer. Chat text is parsed into a typed `Command`,
`CommandDispatcher` routes it to the runtime, and the UI renders only
`RuntimeEvent` values. `ApplicationKernel` owns command routing and concurrency;
new workflow commands should be added to this protocol boundary instead of
calling core services directly from `App.tsx`. Streamed Turn events carry
`turnId` so UI messages do not depend on local callback state.

The active thread is restored on startup. User and assistant messages are
loaded back into the UI, while Turn snapshots and streamed assistant deltas are
kept in append-only JSONL files under `.mini-codex/turns/`. If the previous
process stopped during a response, that Turn is marked `interrupted` on the
next startup and its saved partial reply is shown as incomplete. Interrupted
partial replies are never sent back to the model as completed context.

## Provider Pipeline and Safe Trace

Model calls are split into narrow layers:

```text
ApplicationKernel
  -> TurnEngine
      -> StrictProviderGateway
          -> ProviderProtocol (Responses or Chat Completions)
          -> HttpStreamTransport (fetch, abort, full-stream deadline)
```

The protocol layer owns request and SSE event shapes. The transport layer owns
network cancellation and timeout classification. Provider errors use stable
categories such as `authentication`, `rate_limit`, `timeout`, and `network`
before Runtime turns them into user-visible errors.

Provider streams succeed only after their protocol-specific terminal event:
`response.completed` for Responses or `[DONE]` for Chat Completions. Malformed
frames, premature EOF, duplicate completion, or data after completion fail the
Turn; partial visible output remains marked as failed rather than being treated
as a completed answer.

Raw reasoning fields and `<think>` blocks are removed inside the provider
boundary. Runtime receives only visible assistant deltas, token usage, and a
small set of structured diagnostics. Durable trace events are created from
known event codes with per-code fact allowlists; arbitrary prompts, response
bodies, API keys, and raw reasoning have no trace field through which they can
be persisted.

Context construction uses a conservative estimator for CJK, emoji, code, and
Latin prose. Compaction creates bounded structured summaries with separate
original-goal, constraints, decisions, open-items, and recent-exchanges
sections. Trace payloads, secrets, raw reasoning, and partial replies are not
included as completed model context.

## OpenAI-Compatible Providers

The app follows the Codex-style provider split:

- `modelProvider` selects the active provider.
- `modelProviders` stores provider profiles.
- each provider owns its `baseUrl`, `protocol`, `envKey`, and default model.
- HTTPS is required by default. Plain HTTP requires
  `allowInsecureLoopback: true` and a localhost, `127.0.0.0/8`, or `::1`
  endpoint.
- Provider `headers` are a strict public-metadata allowlist: `Accept`,
  `User-Agent`, `OpenAI-Beta`, `Anthropic-Version`, `HTTP-Referer`, and
  `X-Title`. Names are canonicalized; other names and control-character
  injection are rejected. `Authorization` and `Content-Type` are Runtime-owned.

Provider URLs cannot contain userinfo, a query, or a fragment. The Gateway
revalidates these rules before each request, and HTTP redirects are not
followed automatically.

At production startup the active trusted built-in target also resumes legacy
workspace credential migration. A scoped credential is written before the old
entry is removed; interrupted cleanup fails initialization safely and is
retried on the next startup. Custom Provider targets are not inspected by this
migration path.

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
      "defaultModel": "MiniMax-M3"
    }
  }
}
```

## Offline Tests and CI

`npm test` recursively discovers every `.test.ts` and `.test.tsx` file under
`test/`; adding a test never requires editing a registry. Discovery ignores
symlinks and non-test helpers, sorts absolute paths deterministically, converts
them to platform-safe file URLs, and imports them sequentially in one Node.js
process.

GitHub Actions runs the same locked, offline verification on Ubuntu and Windows
with Node.js 20:

```bash
npm ci
npm run check
npm test
npm run build
```

The workflow provides no Provider credentials and never runs the live smoke
command. Tests use injected fake or unavailable keyring backends, so CI does not
access the machine's real credential store even when the optional native package
is installable for that operating system.

## Explicit Live Provider Smoke Test

Offline tests use a fake Provider and never make a real API request. A live
connection check exists only as an explicit operator action:

```bash
npm run smoke:provider
```

Run it only after the user has explicitly authorized a real Provider request
and the credential is already available through the normal environment,
keyring, or confirmed user-file path. The command does not accept a key
argument and prints neither the prompt, credential, nor raw Provider frames.
Because it uses the normal kernel path, its Turn is stored in the current
workspace like any other submitted prompt.
`npm run check` and `npm run build` compile the smoke source as part of the
TypeScript project, and offline tests statically inspect its safety boundary.
Those automated commands never invoke `npm run smoke:provider` and therefore
never send its real Provider request.
