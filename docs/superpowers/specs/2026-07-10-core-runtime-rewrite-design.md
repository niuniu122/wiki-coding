# MiniMax CLI Core Runtime Rewrite Design

**Status:** Approved design, pending written-spec review
**Date:** 2026-07-10
**Scope:** Rewrite the current CLI core while preserving its external connections. Do not add model-requested tools, a tool registry, permissions, a sandbox, MCP, or a model-tool-result loop.

## 1. Context

The five stabilization stages made the current single-process chat path reliable: context compaction, durable Turn recovery, active interruption, thread navigation, the Command/Event boundary, Provider separation, safe trace, configuration validation, and recoverable file writes are covered by 60 tests.

The remaining architecture cannot be fixed cleanly by continuing to add responsibilities to the 544-line `AgentRuntime`. The core will therefore be rewritten behind stable edge contracts. The rewrite is contract-first: the UI, Provider endpoints, existing workspace data, and user-visible commands are treated as connectors that must continue to fit after the internal replacement.

## 2. Goals

1. Replace the monolithic Runtime with focused services and one composition root.
2. Enforce one live CLI process per workspace and one mutating command at a time.
3. Fail closed when a Provider stream is malformed, truncated, or lacks a terminal event.
4. Version persisted data, migrate legacy data safely, batch stream writes, and checkpoint terminal Turns.
5. Prefer the operating-system keyring; require explicit consent before plaintext user-file storage.
6. Remove exposed-but-unimplemented SQLite configuration and canonicalize legacy Provider configuration.
7. Improve local summaries and token estimation without making another model request.
8. Move UI state transitions into a pure RuntimeEvent reducer.
9. Prove that the rewritten layers still connect through offline end-to-end contract tests.

## 3. Non-goals

- No `tool_call` or `tool_result` model items.
- No file, shell, Git, MCP, hook, plugin, sub-agent, permission, or sandbox implementation.
- No concurrent writers to one workspace.
- No SQLite implementation.
- No model-generated context summary.
- No automatic real API call during tests or migration.
- No breaking change to `/new`, `/threads`, `/resume`, `/api`, `/provider`, `/trace`, `/compact`, `/interrupt`, or `/exit`.

## 4. Fixed Product Decisions

- A second CLI process for the same `.mini-codex` workspace is rejected.
- A stale lock from a dead process is recovered safely.
- Environment variables have first priority for credentials.
- If the OS keyring is unavailable, the CLI shows a plaintext warning and requires explicit confirmation before accepting an API key for user-file storage.
- The core is rewritten, but current Commands, RuntimeEvents, Provider URLs/protocols, and legacy session data remain connected through compatibility adapters.

## 5. Target Architecture

```text
Ink App
  -> chat command parser
  -> CommandDispatcher
  -> ApplicationKernel
       |- WorkspaceLease        one live process owns the workspace
       |- CommandArbiter        one mutating command owns Runtime state
       |- ProviderService       config, provider selection, credentials
       |- SessionService        threads, turns, recovery, migration
       |- TurnEngine            context, compaction, streaming, interruption
       |- ProviderGateway       protocol + transport + terminal state machine
       `- TraceRecorder         safe operational facts only

ApplicationKernel
  -> RuntimeEvent stream
  -> pure UI reducer
  -> Ink rendering
```

`AgentRuntime` is replaced by `ApplicationKernel`. A temporary compatibility export named `AgentRuntime` may delegate to `ApplicationKernel` during the rewrite so the existing tests can be migrated incrementally, but it must contain no workflow logic at completion.

## 6. Stable Connection Contracts

### 6.1 UI to core

Existing `Command` variants remain valid. Existing `RuntimeEvent` variants retain their meaning. New lifecycle and credential-confirmation events may be added, but existing consumers must not be forced to change all at once.

```ts
export interface RuntimeApplication {
  init(): Promise<RuntimeEvent[]>;
  dispatch(command: Command): AsyncGenerator<RuntimeEvent>;
  shutdown(reason: "user" | "signal" | "fatal"): Promise<void>;
}
```

`CommandDispatcher` depends on `RuntimeApplication`, not concrete services. It converts thrown boundary failures into safe RuntimeEvents and never contains domain workflow logic.

### 6.2 Core to Provider

```ts
export interface ProviderGateway {
  stream(request: ProviderRequest): AsyncGenerator<ProviderGatewayEvent>;
}

export type ProviderGatewayEvent =
  | {type: "text.delta"; delta: string}
  | {type: "usage"; inputTokens?: number; outputTokens?: number; totalTokens?: number}
  | {type: "diagnostic"; code: ModelDiagnosticCode; facts: ModelDiagnosticFacts}
  | {type: "completed"};
```

The gateway either emits exactly one terminal `completed` event or throws a structured `ProviderError`. Normal EOF is not a success signal.

### 6.3 Core to storage

```ts
export interface SessionRepository {
  init(): Promise<RepositoryInitResult>;
  createThread(thread: ThreadRecord): Promise<void>;
  updateThread(thread: ThreadRecord): Promise<void>;
  activateThread(threadId: string, activatedAt: string): Promise<ThreadRecord | null>;
  appendTurnSnapshot(turn: TurnRecord): Promise<void>;
  appendTurnDelta(batch: TurnDeltaBatch): Promise<void>;
  checkpointTurns(threadId: string): Promise<void>;
  appendItem(item: ThreadItem): Promise<void>;
  appendTrace(event: TraceEvent): Promise<void>;
  appendSummary(summary: ContextSummary): Promise<void>;
  readThread(threadId: string): Promise<ThreadSnapshot>;
  listThreads(): Promise<ThreadRecord[]>;
}
```

The application does not know file paths or JSONL shapes. The first implementation is `JsonlSessionRepository`. SQLite is not an accepted configuration value.

### 6.4 Core to credentials

```ts
export type CredentialBackend = "environment" | "os-keyring" | "user-file" | "unavailable";

export interface CredentialStore {
  inspect(providerId: string, envKey?: string): Promise<CredentialStatus>;
  get(providerId: string, envKey?: string): Promise<string | null>;
  saveToKeyring(providerId: string, value: string): Promise<void>;
  saveToUserFile(providerId: string, value: string, consent: PlaintextConsent): Promise<void>;
}
```

`PlaintextConsent` is an in-memory, single-use value created only after the user accepts the warning. Passing a boolean from arbitrary code is not sufficient.

## 7. Runtime Ownership and Concurrency

### 7.1 Workspace lease

The lock lives at `.mini-codex/locks/runtime.lock/owner.json`. Ownership is acquired by atomically creating the `runtime.lock` directory. Metadata contains PID, start time, workspace path, and a random nonce.

If the directory already exists:

1. Read and validate `owner.json`.
2. If the recorded process is alive, initialization fails with a clear owner/PID message.
3. If the process is dead or metadata is invalid, atomically rename the directory to a nonce-specific stale path.
4. Retry atomic acquisition.
5. Delete only the renamed stale directory.

Release succeeds only when the on-disk nonce matches the current owner. A process may never remove another process's live lease.

### 7.2 Command arbiter

The kernel lifecycle is:

```text
booting -> idle -> running_turn -> idle -> shutting_down -> stopped
```

During `running_turn`:

- `turn.interrupt`, thread/provider listing, trace toggle, and shutdown are accepted.
- Thread mutation, manual compaction, Provider switching, API-key mutation, and another Turn are rejected immediately with a typed busy event.
- Commands are not silently queued behind a long model request.

This policy is enforced in `ApplicationKernel`; Ink's `busy` state is display-only.

Shutdown first requests interruption, waits for the Turn to reach a terminal persisted state, flushes pending deltas, and then releases the workspace lease.

## 8. Provider Stream State Machine

Both Responses and Chat Completions protocols normalize wire events into:

```text
ignored | text.delta | reasoning.hidden | usage | completed | failed
```

Rules:

- Invalid JSON is a `protocol` error, not an ignored event.
- Unknown valid event types may be ignored explicitly.
- Responses completes only on `response.completed` or a protocol-approved terminal sentinel.
- Chat Completions completes only on `[DONE]`.
- Provider-declared failure events become structured `ProviderError` values.
- EOF before a terminal event is a retryable `protocol` failure.
- A second terminal event or visible delta after completion is a protocol failure.
- Raw event bodies, prompts, reasoning, and credentials never enter RuntimeEvent or durable trace.

`TurnEngine` persists an assistant message as completed only after the gateway terminal event. A truncated partial reply is persisted with `partial: true` and `failed: true`.

## 9. Versioned Storage and Migration

### 9.1 Manifest

`.mini-codex/manifest.json` contains:

```json
{
  "schemaVersion": 1,
  "storage": "jsonl",
  "createdAt": "ISO-8601 timestamp"
}
```

Absence of a manifest with recognizable legacy files means schema version 0. Unknown future versions fail closed.

### 9.2 Event envelope

New JSONL writes use:

```ts
interface StoredEnvelope<T> {
  schemaVersion: 1;
  sequence: number;
  kind: string;
  payload: T;
  createdAt: string;
}
```

Sequence is monotonic within each file. Readers accept bare version-0 records and version-1 envelopes. Domain validators run after unwrapping; syntactically valid but structurally invalid records are corruption errors.

### 9.3 Migration

Migration follows prepare, validate, commit:

1. Acquire the workspace lease.
2. Read and validate all files that will be changed.
3. Write migrated temporary files in the same directories.
4. Read the temporary files back through the version-1 reader.
5. Rename the original files to migration backups.
6. Atomically rename the validated replacements into place.
7. Write the manifest last.

Failure before step 7 leaves the legacy workspace authoritative. Backup deletion is not automatic in the rewrite.

### 9.4 Delta batching and Turn checkpoints

`TurnDeltaBuffer` batches visible assistant text and flushes when either condition is met:

- 1,024 accumulated characters; or
- 250 milliseconds since the first unflushed delta.

Interruption, failure, completion, and shutdown force a flush. This bounds crash loss to the current batch while avoiding one disk append per network fragment.

After a Turn reaches a terminal state, `checkpointTurns` rewrites the Turn log atomically to one latest snapshot per Turn. The latest assistant draft remains available for recovery/audit, but obsolete delta events are removed.

## 10. Configuration Rewrite

Canonical configuration removes:

- `storage.driver=sqlite`;
- the duplicated legacy `api` object;
- unused Provider capability fields unless the new gateway consumes them.

`modelProvider` and `modelProviders` are the only Provider source of truth. Version-0 configuration is parsed by a migration adapter and rewritten only after validation. Custom Provider profiles remain supported.

Selecting SQLite in legacy configuration produces a migration error explaining that only JSONL is supported; it is not accepted and allowed to fail later during Runtime initialization.

## 11. Credential Design

The secure backend is `@napi-rs/keyring`, loaded through an adapter so native-load failure does not prevent CLI startup. It is distributed as an optional dependency. The previous archived `keytar` module is not added as a new dependency; the service and account naming remain stable so existing OS-vault entries can be found when the platform maps them compatibly.

Credential resolution order:

1. Provider-specific environment variable.
2. OS keyring.
3. Previously consented user file.
4. Missing.

The `/api` flow when the keyring is unavailable is:

1. Show that the file is plaintext, its absolute location, and that OS account access can read it.
2. Require explicit confirmation before showing the API-key input mode.
3. Create a single-use consent token in memory.
4. Save atomically with mode `0600` where supported.
5. Report `user-file` honestly; never label it secure storage.

The legacy workspace credential migration remains transactional and deletes legacy files only after the selected destination is durably written.

## 12. Context Engine

`ContextEngine` owns context selection, compaction boundaries, structured summaries, and input-budget decisions. It depends on a `TokenEstimator` interface rather than a global helper.

The deterministic local summary contains bounded sections:

- Original goal: earliest relevant user request.
- Constraints: user statements containing requirement/prohibition language.
- Decisions: selected Provider, architecture, and explicit choices present in visible conversation text.
- Open items: incomplete user requests and failures that still need action.
- Recent exchanges: the latest completed exchanges.

Trace, errors, raw reasoning, failed partial assistant replies, and secrets remain excluded. Every section has a per-entry and total character cap; the summary records its coverage boundary.

The default estimator is conservative:

- CJK characters and emoji are counted approximately one token each.
- Latin prose receives a characters-per-token estimate.
- Code-heavy punctuation and long identifiers receive a tighter ratio.
- Per-message overhead and a 15% safety margin are added.
- `maxCompletionTokens` remains reserved before calculating the input threshold.

The interface permits a future Provider-specific tokenizer without changing `TurnEngine`.

## 13. UI Rewrite Boundary

`App.tsx` retains input widgets, rendering, and `useApp().exit()`. State transitions move to:

```ts
export interface UiState {
  phase: "booting" | "idle" | "running" | "confirming_plaintext" | "stopped";
  messages: DisplayMessage[];
  traces: TraceEvent[];
  status: string;
  tokenUsage?: TokenUsageView;
}

export function reduceRuntimeEvent(state: UiState, event: RuntimeEvent): UiState;
```

The reducer is pure and exhaustively handles RuntimeEvent. It does not call Runtime services or persist data. `App.tsx` no longer contains command concurrency policy.

## 14. Error Handling

- Boundary errors use stable categories and safe messages.
- Initialization failure never transitions the UI to ready.
- Lease conflicts include the owning PID and workspace, but no command line or environment.
- Migration failure identifies the file and leaves the previous authoritative data untouched.
- Provider protocol failure preserves partial visible output as failed, never completed.
- Credential backend failure does not silently write plaintext.
- An unknown schema version, invalid event sequence, or structurally invalid record stops startup with a repair-oriented error.

## 15. Connectivity and Verification Strategy

### 15.1 Existing behavior net

The current 60 tests must remain green or be replaced by stricter equivalent tests that preserve the same user-visible behavior.

### 15.2 New contract tests

1. `Command -> ApplicationKernel -> fake Provider -> repository -> RuntimeEvent` end-to-end flow.
2. Existing Command and RuntimeEvent compatibility tests.
3. Responses and Chat Completions fixture tests for normal terminal events.
4. Malformed JSON, premature EOF, duplicate completion, post-completion delta, and Provider failure tests.
5. Two-process workspace lease test proving the second process is rejected.
6. Stale-lease recovery and nonce-safe release tests.
7. Legacy version-0 workspace migration fixture with byte-preserved backups.
8. Delta batching, forced flush, terminal checkpoint, and restart recovery tests.
9. Keyring available, keyring unavailable, plaintext consent, and consent-reuse rejection tests.
10. Legacy SQLite and duplicated `api` configuration migration tests.
11. Chinese, code-heavy, and English token-estimation tests.
12. Pure UI reducer tests and an Ink boundary test.

### 15.3 Optional live smoke test

An opt-in command may send one minimal prompt through the active Provider and verify:

- authentication;
- request serialization;
- SSE parsing;
- terminal completion;
- final Turn persistence.

It is never run automatically. It requires explicit user approval because it uses a real credential and may incur Provider cost.

## 16. Rewrite Sequence

1. Add connection contract tests around the old core.
2. Implement workspace lease and kernel lifecycle.
3. Implement strict Provider gateway state machines.
4. Implement versioned repository, migration, batching, and checkpoints.
5. Implement ProviderService and credential consent flow.
6. Implement ContextEngine and token estimator.
7. Implement SessionService and TurnEngine.
8. Switch `ApplicationKernel` to the new services.
9. Move UI transitions to the pure reducer.
10. Remove the old Runtime implementation, SQLite placeholder, legacy config source, and dead types.
11. Run the full offline verification matrix.
12. Offer the opt-in live Provider smoke test.

Each step must use test-driven development: a failing contract or behavior test is observed before production code is written.

## 17. Acceptance Criteria

- The CLI starts, resumes legacy history, submits a Turn, streams visible output, interrupts, compacts, switches threads, and exits through the rewritten core.
- A second process cannot open the same workspace.
- A stale lock is recovered without deleting a new owner's lock.
- Truncated or malformed Provider streams cannot produce completed assistant items.
- New persisted records are versioned and validated.
- Stream writes are batched and terminal Turn logs are checkpointed.
- Legacy data is preserved and migration is atomic.
- SQLite is not a selectable Runtime backend.
- Canonical Provider configuration has one source of truth.
- Plaintext credential storage requires explicit, single-use consent and a visible warning.
- Context summaries preserve goals, constraints, decisions, open items, and recent exchanges within a bound.
- Token estimation is replaceable and conservative for Chinese and code.
- `App.tsx` contains no Runtime ownership or command concurrency policy.
- All offline tests, TypeScript checks, and production build pass.
- No real API request occurs without explicit approval.

## 18. Rollback

The rewrite is delivered in atomic commits. Migration backups are retained. If the new kernel fails before migration commit, the old data remains authoritative. If a later code commit regresses behavior, the last green commit can be reverted without reversing user data because version-1 readers retain version-0 compatibility throughout the rewrite.

## 19. Dependency Evidence

- The former `keytar` repository is archived and read-only, so it is not selected as a new dependency: <https://github.com/atom/node-keytar>.
- `@napi-rs/keyring` provides current Node bindings over OS keyrings and documents a keytar-compatible surface: <https://github.com/Brooooooklyn/keyring-node>.
