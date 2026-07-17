---
phase: MMX-08-codex-style-subprocess-sandbox-hardening
verified: 2026-07-17
status: passed
---

# Phase 8 Verification

## Result

Phase 8 passed its local regression gates, the manual hosted-evidence candidate matrix, and the subsequent ordinary push matrix in strict mode. Confirm-mode process tools now enter an enforced Linux Bubblewrap/seccomp boundary or fail closed before target start; process-scoped full access remains the only explicit sandbox bypass and does not bypass the fixed tool registry or hard preflight gates.

## Security Evidence

| Gate | Result |
|---|---|
| Approval and sandbox policy remain independent immutable snapshots | Passed |
| Missing, unsupported, or failed backend rejects with no unsandboxed retry | Passed |
| Linux user/PID/IPC/UTS/network namespace preflight | Passed |
| Host marker, host TCP, and host Unix-socket denial | Passed |
| Writable project workspace with private home/tmp and protected metadata | Passed |
| Transitive Cargo build-script adversarial canary | Passed |
| Child environment excludes Provider secrets and non-allowlisted host state | Passed |
| Full access bypass remains explicit, process-scoped, and bounded | Passed |
| Doctor, permissions, documentation, and CI report the real platform boundary | Passed |
| Provider, retrieval, Vault/Wiki, migration, package, and milestone regressions | Passed |

## Hosted Evidence

- Candidate run: `29553147648` (`workflow_dispatch`) - Windows job `87799771241` and Ubuntu job `87799771311` passed.
- Candidate product fingerprint: `12e41e7384a4474e8e1ed53ccb8942fd7992a6b7b0585a1ab537406b9c74cce4` across 406 product files.
- Candidate head/tree: `fe765daf55d7712cc5f5f61d4077d08f7797bdfc` / `30b79993f230d37c06116936c298fa76c1c874fa`.
- Strict run: `29553650069` (`push`) - Windows job `87801243529` and Ubuntu job `87801243532` passed without candidate mode.
- Ubuntu strict run executed the namespace preflight and adversarial sandbox canary; both platforms passed strict Rust tests/contracts, release verification, and milestone-flow verification.

## Local Evidence

- `cargo fmt --all -- --check` - passed.
- `npm run check` - passed.
- `npm test` - 440/440 passed.
- `npm run build` - passed.
- Hosted artifacts matched the committed fingerprint, file count, archive/binary hashes, environment facts, and performance records.
- Local native Rust linking was unavailable because this Windows installation lacks the MSVC `link.exe`; the hosted Windows MSVC and Linux GNU jobs supplied the required native evidence.

## Release Budgets

| Platform | Compressed archive | Cold start p95 | Idle RSS | BM25 p95 |
|---|---:|---:|---:|---:|
| Windows x64 MSVC | 4,223,738 bytes | 29.033 ms | 7,163,904 bytes | 1.421 ms |
| Linux x64 GNU | 5,005,309 bytes | 4.261 ms | 5,722,112 bytes | 1.020 ms |

All Phase 8 requirements and the retained REL-01/REL-03/REL-04 release gates are satisfied.
