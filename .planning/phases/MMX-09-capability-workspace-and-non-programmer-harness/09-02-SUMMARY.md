---
phase: MMX-09-capability-workspace-and-non-programmer-harness
plan: "02"
subsystem: capability-cli-and-readiness
tags: [rust, cli, tui, protocol, safety]
requires:
  - phase: MMX-09-capability-workspace-and-non-programmer-harness
    provides: typed capability catalogs and retrieval
provides:
  - immutable inventory-derived readiness
  - read-only workspace status and kind-filtered search
  - strict JSONL, plain rendering, and bounded prompt evidence
affects: [cli, tui, protocol, agent-prompt]
key-decisions:
  - "Installation takes precedence over declared access requirements."
  - "Discovery explains the next action but never performs it."
requirements-completed: [CAPW-05, CAPW-06, CAPW-07]
completed: 2026-07-17
status: complete
---

# Phase 9 Plan 2: Readiness and Read-Only CLI Summary

Added `index workspace status` and `index workspace search` for all, project, Skill, or MCP results. A separate immutable inventory overlay derives `ready`, `needs_install`, or `needs_authorization`; source catalogs contain no credential values or mutable runtime state.

Text and strict JSONL expose the same kind, reason, next action, source, optional facts, actual retrieval mode, and deterministic rank evidence. Agent prompt augmentation uses a marked read-only evidence block and explicitly forbids automatic download, installation, authorization, or execution.

## Verification

- Protocol strict round-trip, CLI routing, forbidden-flag, renderer parity, and prompt tests passed.
- Existing CLI, session, migration, tool-approval, Vault, and Wiki tests remained green.
