---
quick_id: 260717-fbi
status: complete
completed: 2026-07-17
---

# Ubuntu 24.04 hosted sandbox and evidence refresh summary

Ubuntu hosted execution now enables the AppArmor-governed unprivileged user-namespace facility, installs Bubblewrap, and proves a real user/PID/network namespace before any security evidence is accepted. The sandbox launch was also corrected to support Rust's safe child-launch socketpair and the host's `/etc/alternatives` linker aliases without reintroducing host networking, credentials, or an unsandboxed fallback.

A manual-only candidate mode refreshes stale hosted evidence while retaining every product, security, package, performance, offline, and malicious-canary gate. Push and pull-request events cannot activate it and remain strict.

## Evidence

- Candidate run `29553147648`: Windows `87799771241` and Ubuntu `87799771311` passed; fingerprint `12e41e7384a4474e8e1ed53ccb8942fd7992a6b7b0585a1ab537406b9c74cce4`, 406 files.
- Strict push run `29553650069`: Windows `87801243529` and Ubuntu `87801243532` passed; candidate-only steps were skipped and strict attestation/milestone gates passed.
- Local `cargo fmt --all -- --check`, `npm run check`, 440 TypeScript tests, `npm run build`, fingerprint comparison, and diff checks passed.

The repair preserves the two permission modes, fail-closed confirm behavior, full-access hard gates, Provider/retrieval/Vault/Wiki flows, BM25-first project discovery, the optional embedding path, and the no-SQLite architecture.
