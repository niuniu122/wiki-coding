# Coding-Agent Foundation Hardening Design

**Date:** 2026-07-13
**Branch:** `codex/coding-agent-foundation`
**Baseline:** `a9659db` (`196/196` offline tests)

## 1. Purpose

Before this CLI can safely execute local file or shell tools, four existing boundaries must be hardened:

1. a workspace must not be able to redirect a previously stored credential to another endpoint;
2. initialization failure must become an explicit recoverable UI state instead of a disabled boot screen;
3. an installed but locked, denied, or broken OS keyring must not be confused with an unavailable keyring;
4. every test file must be discovered automatically and the same offline verification must run in CI.

This phase prepares the runtime for a later `AgentRun -> ToolRequest -> PolicyEngine -> Executor -> ToolResult` loop. It does not execute tools or make a live Provider request.

## 2. Non-goals

- No model tool-call/result parsing.
- No file, shell, Git, network, MCP, plugin, sandbox, or sub-agent executor.
- No change from one live process per workspace.
- No deletion of the previously approved explicit plaintext fallback.
- No automatic Provider smoke request.

## 3. Credential identity and trust

### 3.1 Scoped identity

A credential is identified by a `CredentialTarget`:

```ts
interface CredentialTarget {
  providerId: string;
  endpoint: string;
  authScheme: "bearer";
  trustedEnvironmentKey?: string;
  legacyProviderId?: string;
}
```

`endpoint` is produced by one canonical normalizer: lowercase scheme/host, remove default ports and trailing path slashes, and reject username/password, query, and fragment components. Keyring and user-file accounts use a versioned SHA-256 fingerprint of `providerId + endpoint + authScheme`, never `providerId` alone.

Changing an endpoint or auth scheme therefore produces a new target with no credential. The CLI asks for a new key instead of reusing an old one.

### 3.2 Trusted environment bindings

Workspace configuration may describe `envKey`, but the runtime honors an environment variable only when provider ID, canonical endpoint, protocol, auth scheme, and expected environment name exactly match a built-in provider definition. A redirected built-in ID or a custom provider cannot read `MINIMAX_API_KEY`, `AWS_SECRET_ACCESS_KEY`, or another workspace-selected environment variable.

### 3.3 Legacy migration

Provider-ID-only keyring and user-file records are read or migrated only for an exact trusted built-in target. A custom or redirected endpoint never receives a legacy credential. Migration is atomic and idempotent; it never creates a secret backup.

The explicit plaintext fallback remains available only after the existing single-use `YES` consent. Its new records are scoped by the same target fingerprint and remain in the user-level configuration directory.

Workspace legacy migration ignores environment credentials when deciding
whether the old source can be deleted. With an available keyring it writes and
reads back a scoped credential before cleanup. With an unavailable keyring it
does not use or copy the workspace secret: initialization continues with a
typed, secret-free re-entry notice and leaves the source intact. A later
successful `/api` save or startup with a recovered keyring resumes cleanup.
Cancellation and failed persistence leave the source untouched.

## 4. Provider transport policy

- HTTPS is allowed by default.
- HTTP is rejected unless `allowInsecureLoopback: true` and the host is `localhost`, `127.0.0.0/8`, or `::1`.
- `0.0.0.0`, private LAN addresses, remote hosts, URL userinfo, query, and fragments are rejected.
- Workspace headers use a strict case-insensitive public metadata allowlist: `Accept`, `User-Agent`, `OpenAI-Beta`, `Anthropic-Version`, `HTTP-Referer`, and `X-Title`. Names are canonicalized; all other names and every HTTP control character in a value are rejected. `Authorization` and `Content-Type` remain runtime-owned.
- The gateway revalidates transport policy even for a programmatically constructed `AppConfig`.
- Fetch uses `redirect: "manual"`; redirects become structured Provider errors so credentials are never forwarded to a different endpoint.

## 5. Keyring failure taxonomy

```ts
type KeyringFailureKind = "unavailable" | "locked" | "denied" | "unknown";
class KeyringAccessError extends Error {
  kind: KeyringFailureKind;
  operation: "load" | "read" | "write";
}
```

- Missing native backend and backend-not-running errors are `unavailable`.
- Locked keychains are `locked`.
- Permission or policy rejection is `denied`.
- Unrecognized native failures are `unknown`.

Only `unavailable` may enter the existing explicit plaintext-consent flow. Locked, denied, and unknown failures fail closed with a redacted actionable message. Existing scoped user-file credentials may be read when no keyring backend is available; no failure silently creates or modifies a plaintext file.

## 6. Initialization failure lifecycle

`CommandDispatcher.init()` emits a typed `runtime.init_failed` event on failure. The UI reducer enters:

```text
phase = init_failed
inputMode = init_recovery
```

Only `/retry` and `/exit` are accepted. `/retry` is a UI action that calls `init()` again on the same dispatcher; it does not create another Kernel. `ApplicationKernel.init()` already clears a failed initialization operation and releases its lease, so the same instance preserves idempotent retry and shutdown behavior.

The UI ignores late ready events after exit. Repeated retry input is single-flight. `app.exit` is allowed through the arbiter while booting/init-failed, but other runtime mutations remain rejected.

## 7. Test discovery and CI

`test/run-tests.ts` recursively discovers `.test.ts` and `.test.tsx` through `fs/promises.readdir`, sorts absolute paths deterministically, ignores symlinks, and imports each file through `pathToFileURL`. This avoids shell-glob differences on Windows while preserving the current single-process Node test behavior.

GitHub Actions runs on Ubuntu and Windows with Node 20:

```text
npm ci
npm run check
npm test
npm run build
```

CI never provides Provider credentials and never invokes `smoke:provider`.

## 8. Completion gates

- Same provider ID on a different endpoint cannot read the original key.
- A redirected built-in provider cannot read a trusted environment variable or legacy credential.
- Unsafe HTTP, redirect, and sensitive workspace headers fail before a request is sent.
- Keyring error kinds preserve their semantics; only unavailable offers explicit plaintext fallback.
- Init failure supports safe retry and exit without duplicate lease or shutdown operations.
- A newly created test file runs without editing a registry.
- Linux and Windows CI definitions include all offline gates and exclude the live smoke command.
- Full offline tests, type check, build, diff-check, and independent review pass.
