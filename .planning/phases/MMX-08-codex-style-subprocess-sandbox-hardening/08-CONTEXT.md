# Phase 8: Codex-style Subprocess Sandbox Hardening - Context

**Gathered:** 2026-07-17
**Status:** Ready for execution
**Mode:** Decisions locked by the user's preceding Codex/claw-code architecture discussion and Phase 8 specification

<domain>
## Phase Boundary

Replace direct host launches for the four process-backed tool adapters with an explicit sandbox-policy boundary. Confirm-mode process calls must enter a proven OS sandbox or fail before target start. Process-scoped full access may explicitly choose direct execution but retains the fixed eight-tool registry and every existing hard gate. Provider HTTP, BM25-first project discovery, optional embedding reranking, Vault/Wiki storage, and the main-model Wiki workflow are regression-only surfaces in this phase.

</domain>

<spec_lock>
## Requirements (locked via 08-SPEC.md)

Seven requirements are locked: SBOX-01 through SBOX-07. The implementation and verifier must read `08-SPEC.md`; no implementation shortcut may weaken its fail-closed, adversarial-canary, or truthful-reporting criteria.

</spec_lock>

<decisions>
## Implementation Decisions

### Policy ownership and invocation snapshot

- **D-801:** Add a core-owned `ToolSandboxPolicy` with exactly `Restricted` and `Disabled`. It is not a third user permission mode and it is never stored as a user preference.
- **D-802:** `InvocationMachine` derives the sandbox policy from the permission snapshot when the call enters `Started` and includes it in the `Execute` effect. The CLI must not re-read mutable `RuntimeDriver.permission_mode` at adapter entry.
- **D-803:** Extend `ToolPort::execute` with the copied sandbox policy. File/list/write adapters ignore it because their existing canonical workspace checks are the effect boundary; all process-backed adapters pass it to `BoundedProcess`.

### Launch error and fallback contract

- **D-804:** Replace the launcher's raw `io::Result` boundary with a typed launch error that distinguishes ordinary spawn failure, `sandbox_unavailable`, and `sandbox_denied`.
- **D-805:** Restricted execution never catches a sandbox error and retries the target directly. Unit tests use a target side-effect canary to prove zero fallback.
- **D-806:** Sandbox error receipts contain only static safe fields: stable code, backend, platform, and remediation. They contain no host command line, environment value, secret path, or raw backend stderr.

### Linux enforcement

- **D-807:** Linux restricted execution uses Bubblewrap as the v1 platform backend. Production discovery accepts a canonical executable outside the project workspace and reports missing/unusable Bubblewrap before target start.
- **D-808:** The dedicated sandbox module requires new user, PID, IPC, UTS, and network namespaces, creates a cgroup namespace when available, drops all capabilities, and relies on Bubblewrap `no_new_privs`. It does not use `--disable-userns`: Bubblewrap 0.9 denies `clone3` for that option and prevents Cargo from spawning `rustc`; descendants remain inside the outer user/filesystem/network boundary and inherit the seccomp filter. A raw cBPF seccomp layer rejects socket/socketpair, io_uring, x32-ABI bypass, and kernel-keyring syscalls. The minimal system/runtime view is read-only; `/workspace` is writable; `.git`, `.wiki-coding`, `.minimax`, `.obsidian`, and `.minimax-runtime` are overlaid read-only without following symlinks; HOME and `/tmp` are private.
- **D-809:** Cargo support mounts Rust toolchains and non-credential registry/git caches read-only. It must not expose Cargo credential files. npm/node/rg/git runtimes are mounted only from resolved runtime/system locations outside the project.
- **D-810:** Sandbox process-tree cancellation reuses the existing bounded child lifecycle. A temporary sandbox-home handle remains alive until the child exits or is terminated.

### Platform and network boundary

- **D-811:** Windows and macOS restricted process calls return `sandbox_unavailable` until a real native backend exists. Job Objects, environment clearing, and command allowlists are not described as network/filesystem isolation.
- **D-812:** Confirm-mode child networking is `none`; full access child networking is `full`. Domain allowlisting remains deferred until a managed proxy plus non-bypassable OS route exists.
- **D-813:** Provider adapters do not receive `ToolSandboxPolicy`; their HTTP traffic remains host-owned and their existing fixture/evaluation suite must remain unchanged.

### Diagnostics and release proof

- **D-814:** Expose a read-only sandbox capability probe from `minimax-tools` for `doctor` and user-facing permission text. Capability states are `enforced`, `unavailable`, `unsupported`, and `disabled-by-full-access`; none imply safety from approval alone.
- **D-815:** Add a Linux-only adversarial integration test that creates a dependency-free Cargo fixture with a malicious build script, a host marker, a host-local TCP listener, and a Unix-domain control socket inside the workspace. The canary must prove host read/socket denial and workspace write allowance under restricted policy, then prove explicit access under disabled policy.
- **D-816:** Hosted Ubuntu CI installs/verifies Bubblewrap before Rust tests. Windows runs the mapping and fail-closed contract suite. No test needs public network or Provider credentials.

</decisions>

<canonical_refs>
## Canonical References

- `.planning/phases/MMX-08-codex-style-subprocess-sandbox-hardening/08-SPEC.md` - locked goal, platform boundary, edge coverage, and negative criteria.
- `.planning/phases/MMX-03-safe-tool-completion/03-CONTEXT.md` - fixed tool registry, common preflight, permission lifetime, and process bounds that remain in force.
- `crates/core/src/tool.rs` - invocation permission snapshot and effect emission.
- `crates/core/src/ports.rs` - adapter-neutral tool execution port.
- `crates/cli/src/driver.rs` - approval composition and effect execution.
- `crates/tools/src/adapter.rs` - one composition point for all eight built-in tools.
- `crates/tools/src/process.rs` - bounded process lifecycle, output/time/cancellation, and fixed diagnostic argv.
- `crates/tools/src/sandbox.rs` - platform sandbox construction, Bubblewrap mounts, namespace enforcement, and seccomp filter.
- `crates/tools/src/error.rs` - stable tool denial codes.
- `crates/cli/src/doctor.rs` - current diagnostic report shape.
- `.github/workflows/ci.yml` - Windows/Linux hosted release gate.
- OpenAI Codex source reviewed during the preceding discussion - behavioral reference for separating approval, sandbox, and network; not a license to copy its large Windows subsystem into this small tool.

</canonical_refs>

<deferred>
## Deferred Ideas

- Native Windows restricted-token/ACL/firewall-WFP sandbox backend.
- macOS Seatbelt backend.
- Managed HTTP/SOCKS proxy and domain allowlist child networking.
- Arbitrary shell, dynamic tools, MCP, plugins, or subagents.

</deferred>

---

*Phase: MMX-08-codex-style-subprocess-sandbox-hardening*
*Context gathered: 2026-07-17*
