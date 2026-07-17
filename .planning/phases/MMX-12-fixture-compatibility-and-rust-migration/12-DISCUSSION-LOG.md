# Phase 12: Fixture Compatibility and Rust Migration - Discussion Log

> **Audit trail only.** Planning agents consume `12-CONTEXT.md`.

**Date:** 2026-07-17
**Areas discussed:** legacy command, existing-user data, compatibility basis

| Area | Options considered | Selected |
|------|--------------------|----------|
| Legacy executable | remove now; keep one release; remove all support | remove executable |
| Old data | Rust migration; discard | Rust migration for two releases |
| Baseline | immutable fixtures; live TS runtime | immutable fixtures |

## the agent's Discretion

Exact fixture IDs and support-window metadata format.

## Deferred Ideas

Eventual removal of TypeScript-era fixtures after the support window.
