# Phase 10: Rust Authority and Source Boundaries - Discussion Log

> **Audit trail only.** Planning agents consume `10-CONTEXT.md`, not this file.

**Date:** 2026-07-17
**Areas discussed:** implementation authority, npm boundary, state ownership

| Area | Options considered | Selected |
|------|--------------------|----------|
| Product implementation | near-pure Rust with thin JS; remove Node/npm; staged dual runtime | near-pure Rust with thin JS |
| Legacy command | remove now with Rust migration; keep one release; remove migration too | remove executable legacy, keep Rust migration |
| Compatibility baseline | current Rust/public contract; port every TS internal behavior | current Rust/public contract |

**User rationale:** reduce future conflicts and bugs by eliminating duplicate business implementations.

## the agent's Discretion

Manifest schema and scanner placement, subject to the locked fail-closed behavior.

## Deferred Ideas

Native GUI installers, macOS, ARM, and complete Node removal.
