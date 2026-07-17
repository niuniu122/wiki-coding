---
phase: MMX-08-codex-style-subprocess-sandbox-hardening
plan: "01"
subsystem: runtime-tool-launch-policy
tags: [rust, permissions, sandbox, fail-closed, process]
requires:
  - phase: MMX-03-safe-tool-completion
    provides: fixed tool registry, approval snapshots, and bounded process lifecycle
provides:
  - independent immutable sandbox policy on each execution effect
  - typed unavailable/denied sandbox receipts with no direct fallback
  - explicit full-access direct path that retains every hard preflight gate
  - Provider/tool isolation-policy separation
affects: [core, cli, tools, provider-boundary]
key-decisions:
  - "Approval and subprocess isolation are separate axes; a prompt is never an OS boundary."
  - "The invocation decision snapshot, not mutable driver state, selects restricted or disabled execution."
  - "Restricted backend failure is terminal; only explicit process-scoped full access selects direct execution."
requirements-completed: [SBOX-01, SBOX-03, SBOX-04, SBOX-05]
completed: 2026-07-17
status: complete
---

# Phase 8 Plan 1: Independent Policy and Fail-Closed Launch Summary

The runtime now carries a copied `ToolSandboxPolicy` from the durable invocation decision through `ToolPort` to process-backed adapters. Confirm mode selects `Restricted`; process-scoped full access selects `Disabled`; file tools and Provider adapters do not receive or reinterpret the policy.

Sandbox launch failures have stable `sandbox_unavailable`/`sandbox_denied` receipts and are never caught and retried as ordinary host processes. Windows production tests prove confirm-mode fails before the target starts, while explicit full access proves the direct path remains available for trusted projects and cannot bypass the fixed registry or common preflight.

## Verification

- Core tool-machine tests: 8 passed, including immutable policy snapshot mapping.
- CLI tool-loop tests: 11 passed, including confirm/full-access forwarding and hard-gate parity.
- Process-tool tests: 10 passed, including typed no-fallback failure and real Windows target canaries.
- Workspace Clippy with all targets and warnings denied passed.

No Provider call, credential read, model download, source deletion, publication, or real user-data operation occurred.
