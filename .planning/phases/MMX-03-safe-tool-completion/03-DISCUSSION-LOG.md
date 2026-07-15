# Phase 3: Safe Tool Completion - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-07-15
**Phase:** MMX-03-safe-tool-completion
**Mode:** Auto-selected from decisions the user already locked during the rewrite architecture discussion
**Areas discussed:** Provider-native tool conversation, durable invocation state and recovery, approval behavior and process lifetime, finite adapter schemas and limits, workspace and secret boundaries

---

## Provider-native tool conversation

| Option | Description | Selected |
|--------|-------------|----------|
| Native typed messages | Shared tool definitions/calls/results mapped natively by both Provider adapters. | ✓ |
| Text envelope | Put tool calls/results into ordinary prompt text. | |
| CLI-specific loop | Keep Provider-specific call types in the composition root. | |

**Selection:** Native typed messages.
**Notes:** Preserves the protocol/core/provider layering and prevents Provider wire details from entering core.

## Durable invocation state and recovery

| Option | Description | Selected |
|--------|-------------|----------|
| Durable core state machine | Persist each request/decision/start/result boundary and recover uncertain effects as indeterminate. | ✓ |
| Adapter-owned retry | Let tools retry themselves after interruption. | |
| Best-effort transient loop | Keep tool progression only in memory. | |

**Selection:** Durable core state machine.
**Notes:** Directly implements the locked rule that unknown side effects cannot be reported as success or automatically replayed.

## Approval behavior and process lifetime

| Option | Description | Selected |
|--------|-------------|----------|
| Explicit once-or-reject | Per-call approval in interactive confirm; structured rejection when approval is unavailable; in-memory full-access only. | ✓ |
| Remember approvals | Persist tool- or project-level grants. | |
| Implicit headless approval | Auto-approve because no terminal prompt exists. | |

**Selection:** Explicit once-or-reject.
**Notes:** Keeps exactly two modes and makes every process restart in `confirm`.

## Finite adapter schemas and limits

| Option | Description | Selected |
|--------|-------------|----------|
| Finite diagnostic actions | Static strict schemas; direct `shell=false` processes; dedicated non-mutating Git/npm adapters. | ✓ |
| Arbitrary command string | Send a free-form command through a shell. | |
| Full shell with denylist | Permit most commands and block recognized dangerous tokens. | |

**Selection:** Finite diagnostic actions.
**Notes:** A structural allowlist is auditable and does not pretend a text denylist is a sandbox.

## Workspace and secret boundaries

| Option | Description | Selected |
|--------|-------------|----------|
| Canonical protected workspace | Resolve canonical targets/ancestors, reject escapes/metadata/secrets, and use conflict-aware atomic writes. | ✓ |
| Lexical checks only | Reject `..` but trust the remaining path string. | |
| OS sandbox claim | Treat application validation as a complete sandbox. | |

**Selection:** Canonical protected workspace.
**Notes:** Both modes share this preflight and the product explicitly makes no OS sandbox claim.

## the agent's Discretion

- Private Rust module/helper naming.
- Neutral approval wording and colors within the locked information requirements.
- Conservative finite run budget defaults.
- Secret-pattern and npm diagnostic allowlist implementation details, with fail-closed fixtures.

## Deferred Ideas

- Arbitrary shell, MCP, plugins, subagents, daemons, remote tools, network actions, Git mutation, package installation, Vault/Wiki tools, retrieval tools, and product cutover remain in v2 or their owning later phases.
