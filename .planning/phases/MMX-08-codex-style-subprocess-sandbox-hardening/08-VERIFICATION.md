---
phase: MMX-08-codex-style-subprocess-sandbox-hardening
verified: 2026-07-17
status: pending_hosted_linux_canary
---

# Phase 8 Verification

## Local Result

All locally executable implementation, regression, lint, TypeScript, evaluation, and development-package gates pass. The phase is not marked complete because this Windows host cannot execute a Linux Bubblewrap/seccomp boundary and the product fingerprint correctly invalidated the previous hosted evidence.

## Passed Gates

| Gate | Result |
|---|---|
| Confirm/full-access policy mapping and immutable invocation snapshot | Passed |
| Typed sandbox failure with zero unsandboxed retry | Passed |
| Windows confirm-mode production launch fails before target start | Passed |
| Full-access production path remains explicit and bounded | Passed |
| Bubblewrap namespace/mount/private-home/protected-metadata plan | Passed |
| cBPF architecture, x32, socket, keyring, and io_uring instruction contract | Passed |
| Cargo credentials/config absent from runtime mounts | Passed |
| Doctor and permission status separate approval from isolation | Passed |
| CI Bubblewrap installation and malicious-canary steps remain exact allowlisted steps | Passed |
| Rust format and workspace Clippy with warnings denied | Passed |
| Core, tools, CLI, TypeScript, retrieval, and Provider regressions | Passed |
| Development release archive and performance/security budgets | Passed |

## Commands and Evidence

- `cargo fmt --all -- --check` - passed.
- GNU-LLVM `cargo clippy --workspace --all-targets --locked -- -D warnings` - passed.
- GNU-LLVM `cargo test -p minimax-core -p minimax-tools -p minimax-cli --locked` - passed.
- Linux x86_64 cross-target `cargo clippy -p minimax-tools --all-targets --target x86_64-unknown-linux-gnu --locked -- -D warnings` - passed.
- `npm run check` - passed.
- `npm test` - 439/439 passed.
- `npm run build` - passed.
- Retrieval evaluation - 175 cases, fused top-1/recall@5/MRR all 1.0, passed.
- Provider conformance - both supported protocols passed with no live Provider call.
- Development release verification - 3,217,017-byte compressed base archive; cold-start p95 19.289 ms; idle RSS maximum 4,796,416 bytes; zero credential reads/model downloads/Provider calls.

## Expected Stale-Evidence Failures

- Full workspace Rust test reaches the compatibility harness and rejects the old hosted cutover fingerprint.
- Milestone-flow verification rejects the same stale hosted release record.

These are correct release-gate failures after product code changes, not implementation regressions. They may be cleared only by a new hosted Windows/Linux run for the exact product fingerprint.

## Pending Hosted Gates

1. Ubuntu installs/verifies Bubblewrap and runs `sandbox_adversarial` without skipping.
2. The restricted canary records host-file/TCP/Unix-socket denial and workspace write success.
3. Windows MSVC and Linux GNU complete the remaining matrix.
4. Machine-readable hosted evidence is refreshed to the exact product fingerprint, after which full workspace and milestone-flow verification pass.
