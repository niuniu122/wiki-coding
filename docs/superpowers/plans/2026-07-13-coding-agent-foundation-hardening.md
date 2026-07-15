# Coding-Agent Foundation Hardening Plan

**Design:** `docs/superpowers/specs/2026-07-13-coding-agent-foundation-hardening-design.md`

Each task follows RED -> GREEN -> focused verification -> full offline regression -> independent review. No task may run `npm run smoke:provider`.

## Task 1: Credential target and transport policy

- Add a canonical Provider security module for endpoint normalization, trusted environment bindings, credential targets, public headers, and HTTPS/loopback rules.
- Change CredentialStore and ProviderService to use scoped targets for keyring and user-file records.
- Permit provider-ID-only legacy migration only for exact built-in targets.
- Revalidate policy in StrictProviderGateway and disable automatic redirects in the HTTP transport.
- Start with failing tests for endpoint isolation, redirected built-ins, unsafe headers, HTTP rules, redirect behavior, and legacy compatibility.

## Task 2: Keyring runtime error classification

- Add typed unavailable/locked/denied/unknown failures at the keyring adapter boundary.
- Preserve explicit plaintext fallback only for unavailable.
- Make locked/denied/unknown errors actionable and redacted at the RuntimeEvent boundary.
- Start with injected failing keyring tests proving no unsafe fallback or plaintext mutation.

## Task 3: Initialization failure and recovery UI

- Add `runtime.init_failed`, `init_failed` UI phase, and `init_recovery` input mode.
- Accept only `/retry` and `/exit`; retry the same dispatcher single-flight.
- Keep booting mutations closed and allow exit during failed initialization.
- Cover init failure, retry success, repeated retry, exit, late-ready-after-exit, and lease/shutdown races.

## Task 4: Automatic test discovery and CI

- Replace manual imports with deterministic recursive discovery.
- Add discovery contract tests, including nested tests and ignored non-tests/symlinks.
- Add Ubuntu/Windows Node 20 GitHub Actions offline verification.
- Add a CI contract test proving all required commands exist and live smoke is absent.

## Task 5: Cross-module review and handoff

- Run full offline tests, `npm run check`, `npm run build`, and `git diff --check`.
- Independently review credential migration, transport bypasses, init/shutdown races, and test discovery.
- Commit atomic fixes and document the next phase: durable `AgentRun`, `AgentStep`, `ModelAction`, `ToolRequest`, and `ToolResult` contracts before any executor is enabled.
