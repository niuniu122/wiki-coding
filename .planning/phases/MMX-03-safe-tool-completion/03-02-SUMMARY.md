---
phase: MMX-03-safe-tool-completion
plan: "02"
subsystem: bounded-tool-adapters
tags: [rust, tools, permissions, approval, filesystem, process, git, npm, ci]
requires:
  - phase: MMX-03-safe-tool-completion
    plan: "01"
    provides: native tool conversation, durable invocation state, recovery, and two-mode policy
provides:
  - exactly eight strict bounded Rust v1 tool adapters behind one shared preflight
  - interactive exact-yes approval plus fail-closed headless confirmation
  - working /agent, /continue, and process-local /permissions commands
  - executable TOOL-01 through TOOL-05 compatibility evidence on Windows and Linux
affects: [vault-workflow, wiki-workflow, retrieval-agent, release-gates]
tech-stack:
  added: [sha2]
  reused: [tempfile]
  patterns: [shared hard-gate preflight, canonical workspace containment, atomic hash-guarded writes, shell-free fixed argv, bounded kill-and-wait, exact-call approval]
key-files:
  created:
    - crates/tools/src/adapter.rs
    - crates/tools/src/policy.rs
    - crates/tools/src/path.rs
    - crates/tools/src/read.rs
    - crates/tools/src/write.rs
    - crates/tools/src/process.rs
    - crates/tools/src/git.rs
    - crates/tools/src/npm.rs
    - fixtures/compat/tools/e2e.v1.json
  modified:
    - crates/cli/src/driver.rs
    - crates/cli/src/main.rs
    - crates/tui/src/render.rs
    - crates/tui/src/shell.rs
    - fixtures/compat/baseline-status.v1.json
key-decisions:
  - "Full access skips only the per-call prompt; it still crosses the identical shared preflight and is never serialized."
  - "The process surface is a finite action enum with fixed executable/argv, safe environment, bounded output/time, and verified tree cleanup rather than a generic shell."
  - "Approval requires the exact lowercase answer yes for the visible call ID; invalid, no, EOF, interrupt, and non-interactive input reject once without retry pressure."
patterns-established:
  - "Concrete adapters compose only behind BuiltinToolPort, so the CLI cannot accidentally publish schemas without their matching policy and limits."
  - "Every tool result returned to the Provider and terminal is typed, bounded, secret-screened, and control-sanitized."
requirements-completed: [TOOL-04, TOOL-05]
coverage:
  - id: D1
    description: Exactly eight strict schemas and one permission-independent preflight deny unknown, malformed, secret, protected, escaped, or cancelled input.
    requirement: TOOL-05
    verification:
      - kind: unit
        ref: "crates/tools/tests/tool_schemas.rs"
        status: pass
      - kind: integration
        ref: "crates/tools/tests/workspace_tools.rs"
        status: pass
    human_judgment: false
  - id: D2
    description: Read/list and hash-conflict-aware patch/write stay inside the canonical workspace and never leave partial writes.
    requirement: TOOL-04
    verification:
      - kind: integration
        ref: "crates/tools/tests/workspace_tools.rs"
        status: pass
    human_judgment: false
  - id: D3
    description: Diagnostics, Git inspection, and npm diagnostics use fixed shell-free requests, bounded output/time, and kill-and-wait cancellation.
    requirement: TOOL-04
    verification:
      - kind: integration
        ref: "crates/tools/tests/process_tools.rs"
        status: pass
    human_judgment: false
  - id: D4
    description: Both Provider protocols complete two ordered concrete calls and a final answer under full access, while confirm binds one exact decision per call and headless rejects.
    requirement: TOOL-01
    verification:
      - kind: integration
        ref: "crates/cli/tests/tool_loop.rs#concrete_builtin_tools_complete_on_both_provider_protocols_in_full_access"
        status: pass
      - kind: integration
        ref: "crates/cli/tests/tool_loop.rs#fixture_confirm_mode_binds_one_answer_to_each_ordered_call"
        status: pass
    human_judgment: false
  - id: D5
    description: Ubuntu and Windows MSVC run the complete offline TypeScript, Rust, compatibility, architecture, retrieval, and Provider gates.
    requirement: TOOL-05
    verification:
      - kind: hosted-ci
        ref: "https://github.com/niuniu122/minimax-codex/actions/runs/29427086563"
        status: pass
    human_judgment: false
duration: 83min
completed: 2026-07-15
status: complete
---

# Phase 3 Plan 2: Bounded V1 Tool Adapters and Approval Summary

**The Rust shell now performs the complete finite v1 tool set through understandable two-mode approval while preserving the same non-bypassable safety gates in both modes.**

## Performance

- **Duration:** 83 min
- **Started:** 2026-07-15T13:55:22Z
- **Completed:** 2026-07-15T15:18:45Z
- **Tasks:** 3
- **Files modified:** 41

## Accomplishments

- Added exactly eight strict tools: bounded read/list, conflict-aware atomic patch/write, finite diagnostics, Git status/diff, and validated existing npm diagnostics.
- Added canonical workspace containment, protected/secret screening, deterministic byte/count limits, cancellation checks, and honest cancelled/failed/indeterminate results.
- Added shell-free fixed process execution with safe environment, 30-second/64-KiB limits, no raw stderr, and process-tree kill-and-wait on cancellation, timeout, overflow, or I/O failure.
- Made `/agent`, `/continue`, and `/permissions confirm|full-access` operational; restart always returns to confirm and headless confirm never auto-approves.
- Promoted TOOL-01 through TOOL-05 only after concrete two-Provider E2E fixtures, 138 Rust tests, 432 TypeScript tests, compatibility checks, and hosted Ubuntu/Windows CI passed.

## Task Commits

1. **Task 1: Build shared preflight and bounded workspace adapters** - `25f6eb4`
2. **Task 2: Implement finite diagnostic, Git, and npm adapters** - `fe32a8a`
3. **Task 3: Expose approval, commands, concrete E2E, and compatibility evidence** - `bcb2f26`

## Decisions Made

- Kept permission state out of preflight, adapter APIs, configuration, journal, and restart state so convenience cannot alter hard-gate outcomes.
- Used a single concrete `BuiltinToolPort` composition root for registry definitions, workspace policy, and process limits.
- Kept `package.json` on `dist/cli.js`; this phase makes the Rust development shell capable but does not perform the Phase 6 product cutover.
- Used only scripted Providers and local fixtures; no credential, live API request, embedding download, install, migration, deletion, PR, or merge occurred.

## Deviations from Plan

None. The planned finite adapters, approval behavior, compatibility evidence, local gates, branch push, and hosted Windows/Linux verification were completed without widening scope.

## Issues Encountered

- The local Windows host required the official Rust 1.97 gnullvm fallback because current MSVC Build Tools are unavailable; hosted Windows/MSVC CI passed.
- Initial Git HTTPS pushes resolved `github.com` to an unreachable regional address. A reachable address from GitHub's official metadata was supplied to one TLS-verified push command without persisting a Git/network override.
- Hosted CI emitted a non-failing GitHub Actions Node 20 deprecation annotation for `actions/checkout@v4` and `actions/setup-node@v4`; product Node 20 tests still passed and no workflow change was required for Phase 3.

## User Setup Required

None.

## Next Phase Readiness

- Phase 3 is complete and the tool boundary is ready for Phase 4 Vault transactions and the separately visible main-model Wiki workflow.
- Phase 4 must preserve these hard gates and must not let Vault or Wiki code introduce SQLite, generic shell authority, secret persistence, or automatic deletion.

## Self-Check: PASSED

- Rust workspace: 138/138 tests passed; formatting and workspace Clippy with `-D warnings` passed.
- TypeScript reference: type checking/build passed; 432/432 tests passed.
- Retrieval evaluation: 175 cases passed; Provider conformance passed all 8 checks for both protocols.
- Compatibility/architecture verifier and `git diff --check` passed.
- Hosted CI run `29427086563`: Ubuntu and Windows/MSVC jobs passed.

---
*Phase: MMX-03-safe-tool-completion*
*Completed: 2026-07-15*
