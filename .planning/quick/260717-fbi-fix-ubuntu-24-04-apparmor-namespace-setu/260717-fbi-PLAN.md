---
quick_id: 260717-fbi
status: complete
date: 2026-07-17
---

# Fix Ubuntu 24.04 hosted sandbox and evidence refresh

## Goal

Restore a trustworthy hosted path for the Phase 8 sandbox canary without weakening the product sandbox or the normal push/pull-request release gate.

## Tasks

1. Configure the ephemeral Ubuntu runner for Bubblewrap user/network namespaces and replace the version-only check with a real namespace preflight.
2. Add an explicit, manual-only hosted-evidence candidate mode. Candidate mode skips only the stale hosted-attestation comparison while retaining command, architecture, security, package, performance, offline, and malicious-sandbox gates. Normal push and pull-request runs remain strict.
3. Improve Ubuntu/AppArmor remediation text and release documentation, update structural CI tests, run local verification, dispatch the candidate matrix, refresh the machine-readable Windows/Linux evidence, then require a final strict CI pass.

## Verification

- CI contract tests reject candidate mode on push/pull_request and altered Bubblewrap setup or canary commands.
- Rust tests distinguish candidate prerequisites from the strict hosted attestation.
- Local TypeScript, Rust formatting/Clippy, focused Rust tests, and documentation checks pass.
- A manual candidate matrix succeeds on Ubuntu and Windows and publishes both release-evidence artifacts.
- The evidence fixture is bound to the candidate run and current product fingerprint.
- A subsequent ordinary push CI run succeeds without candidate mode.
