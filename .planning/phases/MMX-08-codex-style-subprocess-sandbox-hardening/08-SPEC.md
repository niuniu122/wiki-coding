# Phase 8: Codex-style subprocess sandbox hardening — Specification

> **Supersession note (2026-07-21):** Phase 8's fixed-tool-only statement remains historical evidence for the completed Phase 8 scope. For the new `shell_command` and `shell_session` tools only, it is superseded by `docs/superpowers/specs/2026-07-21-full-access-shell-design.md`, which permits arbitrary host Shell commands exclusively in process-scoped `full-access`. The original eight tools retain the Phase 8 contract.

**Created:** 2026-07-17
**Ambiguity score:** 0.08 (gate: <= 0.20)
**Requirements:** 7 locked

## Goal

Change process tools from ordinary host child processes into a fail-closed, operating-system-enforced execution boundary whose approval and sandbox settings follow the same separation used by Codex.

## Background

The Rust runtime already exposes only a fixed bounded tool set, clears the child environment, applies offline flags to package diagnostics, limits output and time, and keeps schema/path/secret/destructive-operation gates active in both permission modes. However, `TokioProcessLauncher` currently starts Cargo, Git, and npm directly on the host. Transitive project code, such as a Cargo build script or an npm lifecycle script, can therefore read host files or open sockets even after the visible command itself passed preflight.

The approved contract separates two axes. `confirm` versus `full-access` determines whether a user approval prompt is required. A separate execution policy determines whether process tools are sandboxed. For the two v1 permission modes, `confirm` maps to a restricted sandbox and `full-access` maps to disabled sandboxing. Provider HTTP remains outside the child-process sandbox.

## Requirements

1. **Independent approval and sandbox axes (SBOX-01)**: Approval policy and subprocess isolation are represented separately; `confirm` selects the restricted sandbox, `full-access` selects disabled sandboxing only for the current process, and a restart returns to `confirm`.
   - Current: `RuntimeDriver` stores only `PermissionMode`, and the tools adapter receives no execution-isolation policy.
   - Target: Each accepted process invocation receives an immutable sandbox policy snapshot derived from the current permission mode; approval and isolation remain distinct types and responsibilities.
   - Acceptance: Driver and adapter tests prove `confirm -> restricted`, `full-access -> disabled`, restart -> `confirm`, repeated mode selection is idempotent, and changing mode does not mutate a process invocation already in flight.

2. **Enforced confirm-mode boundary (SBOX-02)**: Every confirm-mode process tool enters an operating-system-enforced sandbox before target code starts, with child network denied, only the project workspace writable, and host-private paths absent from the child view.
   - Current: Confirmed process tools are ordinary host processes with a cleared/allowlisted environment but no OS filesystem or network boundary.
   - Target: On Linux, a dedicated Bubblewrap-plus-seccomp backend constructs a minimal read-only system view, a writable project bind, a private temporary home, a required user/network namespace boundary, and a syscall filter that blocks new sockets, io_uring, and kernel-keyring access. Each process has its own sandbox lifecycle and cancellation still terminates its process tree.
   - Acceptance: Linux adversarial tests run transitive Cargo build code that can write inside the workspace but cannot read a host marker outside it or connect to host TCP or Unix-domain listeners; repeated and parallel runs produce the same denial without leaked shared state.

3. **Fail-closed backend handling (SBOX-03)**: A missing, unsupported, or failed sandbox backend rejects the process with a stable actionable denial before the target executable starts.
   - Current: Process spawn errors collapse into the generic `spawn_failed` result and there is no sandbox capability check.
   - Target: The launcher distinguishes `sandbox_unavailable` and `sandbox_denied` from ordinary spawn failures, performs backend validation before target execution, and never silently falls back to an unsandboxed child.
   - Acceptance: A target side-effect canary remains absent when Bubblewrap is missing, when backend startup fails, and on platforms with no implemented backend; error code and remediation text remain valid UTF-8 and stable across repeated/concurrent requests.

4. **Explicit full-access bypass with unchanged hard gates (SBOX-04)**: `full-access` runs the same fixed bounded process tools directly on the host while preserving all schema, path, secret, destructive-operation, timeout, output, and cancellation gates.
   - Current: Full access skips approval, but its process launch path is indistinguishable from confirm mode because neither mode has a sandbox.
   - Target: Disabled sandboxing is an explicit policy selected only by process-scoped full access; it bypasses only the OS sandbox and user prompt, not the fixed tool registry or preflight controls.
   - Acceptance: Full-access canaries demonstrate host network/file access while existing denial, timeout, output-bound, cancellation, and destructive-operation tests remain green; concurrent invocations retain the policy snapshot they started with.

5. **Provider and environment separation (SBOX-05)**: Provider HTTP remains a host-owned network path and is not routed through or disabled by the subprocess sandbox; subprocesses continue receiving only the existing allowlisted environment.
   - Current: Provider and process networking are separate in code but this separation is not expressed as a sandbox contract.
   - Target: The sandbox policy is passed only through the tool execution port, never the Provider port, and no proxy credential or Provider secret is added to child environments.
   - Acceptance: Provider fixture tests are unchanged and pass while process-network canaries are denied in confirm mode; environment contract tests prove repeated/parallel child launches receive only the allowlist.

6. **Truthful capability reporting (SBOX-06)**: `doctor`, permission/status text, release documentation, and CI report the actual backend, enforcement state, supported platforms, and remediation without claiming protection that is unavailable.
   - Current: `/permissions` describes prompting only, and diagnostics do not report a subprocess sandbox backend.
   - Target: Diagnostics identify `bubblewrap+seccomp` on enforced Linux, `unavailable` on unsupported or misconfigured systems, and `disabled-by-full-access` when applicable; Windows/macOS confirm-mode process execution fails closed until a real native backend exists.
   - Acceptance: Snapshot/CLI tests cover present, missing, empty, unsupported, Unicode-path, confirm, and full-access outputs; release docs state the Linux Bubblewrap prerequisite and the fail-closed Windows/macOS limitation; hosted Linux CI installs and exercises Bubblewrap.

7. **Adversarial release evidence (SBOX-07)**: Security verification executes malicious transitive project code rather than inspecting wrapper arguments alone.
   - Current: Existing process tests cover argument construction, timeout, output, cancellation, environment clearing, and direct preflight, but not hostile build/lifecycle code crossing the host boundary.
   - Target: A release-gated canary suite proves confirm-mode file/socket denial, fail-closed backend behavior, workspace write allowance, and explicit full-access bypass, alongside unit tests of the policy mapping.
   - Acceptance: The Linux hosted job fails if the malicious canary can read the marker, reach the listener, skip the sandbox, or cannot write its intended workspace result; cross-platform unit/contract tests fail if policy mapping or fail-closed behavior regresses.

## Boundaries

**In scope:**
- A sandbox policy type in the core/tool port that is independent of approval policy.
- Linux Bubblewrap-plus-seccomp process isolation with denied child network and a workspace-scoped writable view.
- Stable typed sandbox launch errors and zero-target-start fail-closed behavior.
- Explicit full-access direct execution with all existing hard gates retained.
- Sandbox diagnostics, permission messaging, release documentation, and hosted Linux canaries.
- Regression protection for Provider networking, project discovery, Vault/Wiki behavior, and the fixed tool registry.

**Out of scope:**
- A custom Windows restricted-token/WFP backend — Codex's implementation is a large privileged subsystem; a partial imitation would create false confidence, so unsupported confirm-mode process execution fails closed.
- macOS Seatbelt support — macOS remains a v2 platform target.
- Domain allowlist networking for child processes — this requires a managed proxy plus an OS boundary that prevents proxy bypass; until both exist, confirm mode is `none` and full access is `full`.
- Arbitrary shell, MCP, plugins, or subagents — the v1 fixed tool registry remains unchanged.
- Sandboxing Provider HTTP — model API traffic is host-owned and separately configured.
- Changes to BM25-first project discovery, optional embedding reranking, Vault storage, or the main-model Wiki workflow.

## Constraints

- Rust remains the default implementation and the base artifact stays within the existing 50 MB compressed release budget.
- Core must not depend on a platform adapter; sandbox policy flows outward through a port.
- Confirm mode must fail closed whenever enforcement cannot be proven.
- No test may perform a real Provider call, download an embedding model, or depend on public internet access.
- Bubblewrap discovery must never trust an executable located inside the project workspace.
- Existing bounded output, timeout, cancellation, environment clearing, offline package flags, and process-tree termination remain intact.

## Acceptance Criteria

- [ ] **AC-01:** Tests prove `confirm -> restricted`, `full-access -> disabled`, restart -> `confirm`, and repeated selection of either mode is idempotent.
- [ ] **AC-02:** Each accepted process invocation snapshots its sandbox policy; later permission changes and parallel invocations cannot mutate that snapshot.
- [ ] **AC-03:** On Linux with Bubblewrap, the sandbox is established before the target executable starts and the target can write only to its project workspace and private temporary home.
- [ ] **AC-04:** A transitive Cargo build script in confirm mode cannot connect to host-local TCP or Unix-domain listeners and records denied results.
- [ ] **AC-05:** The same build script cannot read a host marker outside the workspace but can write its result inside the workspace.
- [ ] **AC-06:** Repeated and parallel confirm-mode canaries remain isolated and cancellation terminates the complete sandboxed process tree.
- [ ] **AC-07:** Missing, unsupported, or failing backends return `sandbox_unavailable` or `sandbox_denied`; a target side-effect proves no unsandboxed fallback occurred.
- [ ] **AC-08:** Sandbox errors are stable UTF-8 records with a machine code, backend name, platform, and concrete remediation, including empty/missing values and Unicode project paths.
- [ ] **AC-09:** Full access directly runs the fixed tool target and permits the file/socket canaries while the existing schema/path/secret/destructive/timeout/output/cancellation suite remains green.
- [ ] **AC-10:** Provider fixture traffic remains functional and no Provider credential, proxy secret, or non-allowlisted host environment reaches a subprocess.
- [ ] **AC-11:** `doctor` and permission/status output distinguish enforced, unavailable, unsupported, and disabled-by-full-access states without claiming false protection.
- [ ] **AC-12:** Hosted Linux CI installs/verifies Bubblewrap and runs the real malicious canary; Windows/macOS contract tests prove confirm mode fails closed where no backend exists.
- [ ] **AC-13:** README/release/security documentation explains the trust boundary, platform matrix, full-access consequence, and remediation in non-programmer-readable language.
- [ ] **AC-14:** The BM25-first automatic project finder, optional embedding reranker, Vault/Wiki workflow, and Provider adapters pass their existing regression suites unchanged.
- [ ] **AC-15 (negative):** Confirm mode must never silently fall back to ordinary host execution when the sandbox is missing or fails.
- [ ] **AC-16 (negative):** A user approval prompt must never be presented as filesystem or network isolation.
- [ ] **AC-17 (negative):** Full access must never become arbitrary shell access or disable the existing hard preflight gates.
- [ ] **AC-18 (negative):** The subprocess sandbox must never intercept Provider HTTP or copy Provider credentials into child environments.

## Edge Coverage

**Coverage:** 17/17 applicable edges resolved · 0 unresolved

| Category | Requirement | Status | Resolution / Reason |
|----------|-------------|--------|---------------------|
| idempotency | SBOX-01 | covered | AC-01 repeats mode selection without state drift. |
| concurrency | SBOX-01 | covered | AC-02 snapshots policy per invocation. |
| idempotency | SBOX-02 | covered | AC-05 and AC-06 repeat the enforced boundary. |
| concurrency | SBOX-02 | covered | AC-06 covers parallel isolation and cancellation ordering. |
| empty | SBOX-03 | covered | AC-08 covers missing/empty backend data. |
| encoding | SBOX-03 | covered | AC-08 requires stable UTF-8 and Unicode paths. |
| idempotency | SBOX-03 | covered | AC-07 repeats backend failure with no side effect. |
| concurrency | SBOX-03 | covered | AC-07 requires concurrent fail-closed behavior. |
| idempotency | SBOX-04 | covered | AC-09 repeats direct execution while gates stay active. |
| concurrency | SBOX-04 | covered | AC-02 and AC-09 cover simultaneous policy snapshots. |
| idempotency | SBOX-05 | covered | AC-10 repeats Provider and environment separation. |
| concurrency | SBOX-05 | covered | AC-10 covers parallel child environment isolation. |
| empty | SBOX-06 | covered | AC-11 covers missing/unsupported backend status. |
| encoding | SBOX-06 | covered | AC-08 and AC-11 cover Unicode-safe diagnostics. |
| concurrency | SBOX-06 | covered | AC-11 reports state from an immutable capability snapshot. |
| idempotency | SBOX-07 | covered | AC-12 makes canaries repeatable release gates. |
| concurrency | SBOX-07 | covered | AC-06 exercises parallel malicious canaries. |

## Prohibitions (must-NOT)

**Coverage:** 4/4 applicable prohibitions resolved · 0 unresolved

| Prohibition (must-NOT statement) | Requirement | Status | Verification / Reason |
|----------------------------------|-------------|--------|------------------------|
| MUST NOT silently execute on the host after a restricted-backend failure. | SBOX-03 | resolved | test — AC-07 and AC-15 use a target side-effect canary. |
| MUST NOT describe user confirmation as an isolation boundary. | SBOX-01 | resolved | judgment — AC-11, AC-13, and AC-16 review exact user-facing language. |
| MUST NOT turn full access into arbitrary shell or remove hard preflight gates. | SBOX-04 | resolved | test — AC-09 and AC-17 retain the existing denial suite. |
| MUST NOT weaken Provider, automatic project discovery, or Wiki behavior while isolating process tools. | SBOX-05 | resolved | test — AC-10, AC-14, and AC-18 run existing regressions. |

Canon security controls such as path traversal, command injection, and secret detection remain owned by the existing security suite and secure-phase workflow; this phase does not duplicate them as bespoke prohibitions.

## Ambiguity Report

| Dimension | Score | Min | Status | Notes |
|-----------|-------|-----|--------|-------|
| Goal Clarity | 0.96 | 0.75 | met | Ordinary host process changes to an enforced/fail-closed boundary. |
| Boundary Clarity | 0.94 | 0.70 | met | Windows native backend, macOS, allowlist proxy, and unrelated product flows explicitly bounded. |
| Constraint Clarity | 0.88 | 0.65 | met | Platform, artifact, architecture, offline, and executable-discovery constraints locked. |
| Acceptance Criteria | 0.94 | 0.70 | met | 18 pass/fail criteria include real adversarial code and negative assertions. |
| **Ambiguity** | **0.08** | **<= 0.20** | **met** | No unresolved edge or prohibition remains. |

## Interview Log

| Round | Perspective | Question summary | Decision locked |
|-------|-------------|------------------|-----------------|
| Prior discussion | Researcher | How do Codex and claw-code handle this risk? | Follow Codex's separation of approval, sandbox, and network; do not copy claw-code's advisory-only boundary. |
| Prior discussion | Simplifier | What is the smallest honest v1 behavior? | Confirm is sandboxed/default-deny network; full access explicitly bypasses; Provider stays separate. |
| Prior discussion | Boundary Keeper | What if the platform has no real backend? | Fail closed with remediation; never claim a partial Windows imitation is safe. |
| Prior discussion | Failure Analyst | Can transitive build code bypass wrapper checks? | Release gate must execute malicious build code against a host file and host-local listener. |
| Auto resolution | Edge/prohibition probe | What happens under repetition, concurrency, failed startup, and misleading UI? | Snapshot per invocation, zero fallback, stable typed errors, truthful status, and negative canaries. |

---

*Phase: MMX-08-codex-style-subprocess-sandbox-hardening*
*Spec created: 2026-07-17*
*Next step: plan and execute Phase 8 from this locked specification.*
