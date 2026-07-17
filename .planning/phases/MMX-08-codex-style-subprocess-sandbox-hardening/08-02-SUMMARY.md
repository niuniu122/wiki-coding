---
phase: MMX-08-codex-style-subprocess-sandbox-hardening
plan: "02"
subsystem: linux-subprocess-sandbox
tags: [rust, bubblewrap, seccomp, namespaces, adversarial-test]
requires:
  - phase: MMX-08-codex-style-subprocess-sandbox-hardening
    provides: independent restricted/disabled sandbox policy and fail-closed launch contract
provides:
  - Linux Bubblewrap filesystem and namespace isolation
  - seccomp child-network and sensitive-syscall restrictions
  - hostile transitive Cargo build-script release canary
  - workspace-only write boundary with private home and temporary directories
affects: [tools, ci, release-security, doctor]
key-decisions:
  - "Use the outer Bubblewrap user, PID, IPC, UTS, and network namespaces without disabling nested user namespaces required by the Rust toolchain."
  - "Allow local socketpair creation for safe child launch bookkeeping while denying network socket creation and proving TCP/Unix host endpoints remain unreachable."
  - "Mount only required runtime/toolchain paths and the system linker alias directory; credentials and host-private paths stay absent."
requirements-completed: [SBOX-02, SBOX-03, SBOX-04, SBOX-07]
completed: 2026-07-17
status: complete
---

# Phase 8 Plan 2: Linux Enforcement and Adversarial Canaries Summary

Confirm-mode process tools now launch through a dedicated Linux Bubblewrap backend with read-only runtime/toolchain mounts, private home and temporary directories, one writable project workspace, isolated namespaces, dropped capabilities, and a seccomp filter that denies child networking and sensitive kernel interfaces.

The hosted adversarial Cargo fixture executes transitive build code and proves that restricted execution cannot read a host marker or connect to host-local TCP/Unix endpoints while it can still compile and write inside the project workspace. Missing or broken backends fail before the target starts; explicit full access retains the direct bounded path.

Ubuntu 24.04 CI now enables the runner's AppArmor-controlled unprivileged user-namespace facility and performs a real namespace preflight. The final design keeps the Rust child-launch socketpair channel and required `/etc/alternatives` linker aliases without adding a host-network or credential path.

## Verification

- Hosted candidate Ubuntu job `87799771311` passed the namespace preflight and malicious Cargo canary.
- Strict Ubuntu job `87801243532` repeated the canary and all strict Rust/release/milestone gates.
- Parallel, repeat, cancellation, backend-unavailable, workspace-write, host-file, TCP, Unix-socket, and explicit-bypass tests passed.

No unsandboxed confirm-mode fallback, Provider secret exposure, package publication, or real user-data operation occurred.
