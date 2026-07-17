---
phase: MMX-08-codex-style-subprocess-sandbox-hardening
reviewed: 2026-07-17
status: pending_hosted_linux_canary
severity_gate: high
---

# Phase 8 Security Review

## Result

The implementation is locally complete and has no unresolved high/critical finding in static review or Windows/cross-target tests. Final security acceptance is intentionally withheld until the exact committed tree executes the malicious build-script canary on hosted Ubuntu with Bubblewrap and seccomp active.

## Threat Model

| Attacker action | Asset at risk | Enforced control | Evidence |
|---|---|---|---|
| Transitive Cargo/npm code reads host files | user files and credentials | empty mount namespace, minimal read-only runtime view, private HOME/tmp | host-marker canary plus mount-plan tests |
| Child opens TCP/IPv6/Unix sockets | local services, internet, control sockets | required network namespace plus seccomp socket/socketpair denial | TCP and workspace Unix-socket canaries |
| Child uses io_uring or x32 ABI to bypass syscall comparison | network boundary | io_uring_setup denial and blanket x32 syscall denial | cBPF instruction test |
| Child reads Linux kernel keyrings | host secrets | add_key/request_key/keyctl denial | cBPF instruction test |
| Project replaces sandbox executable | all host assets | canonical `/usr/bin/bwrap` or `/bin/bwrap`; project-local backend rejected | discovery contract |
| Project hides host target behind protected metadata symlink | host filesystem | protected metadata overlays never follow symlinks | Unix symlink argument test |
| Sandbox setup fails | all host assets | typed fail-closed error; no direct retry | launcher side-effect/no-retry test |
| User explicitly selects full access | all ordinary user permissions | explicit process-scoped bypass, trusted-project warning, fixed tools and hard preflight remain | full-access and denial regression tests |

## Boundary Review

- Approval and OS isolation are separate. User confirmation alone is never described as protection.
- Only process-backed tools receive `ToolSandboxPolicy`; direct bounded file tools retain canonical workspace checks and Provider HTTPS remains host-owned.
- Linux enforcement lives in `sandbox.rs`; process lifetime/output/cancellation remains in `process.rs`; core owns only the platform-neutral policy enum.
- Provider credentials and non-allowlisted environment variables are never copied into child processes.
- Cargo credential/config files are not mounted. Read-only toolchain and dependency caches remain intentionally visible so locked offline diagnostics can run.
- Windows confirm-mode process execution fails closed. No Job Object, environment clearing, or command allowlist is mislabeled as filesystem/network isolation.

## Residual Risk

1. `full-access` is intentionally unsandboxed. Transitive project code can read host files and use host networking after the user explicitly selects it.
2. Read-only Rust toolchains and Cargo registry/git caches reveal their contents to restricted build code. They are treated as executable/runtime inputs, not credential stores.
3. Linux enforcement depends on kernel user namespaces, seccomp, and Bubblewrap. Unsupported or blocked environments reject process execution instead of weakening policy.
4. Native Windows and macOS sandboxes remain deferred; confirm-mode process tools are unavailable there.

## Release Blocker

Run the exact committed tree on hosted Ubuntu and require the malicious canary to prove:

- host marker unreadable;
- host TCP listener unreachable;
- workspace Unix-domain listener unreachable;
- workspace result writable;
- the same fixture reaches all three host targets only under explicit full access.

Until that run passes and hosted cutover evidence is refreshed, SBOX-02, SBOX-06, and SBOX-07 remain unclosed and Phase 8 must stay in progress.
