---
phase: MMX-01-contract-foundation
plan: "02"
subsystem: architecture
tags: [rust, cargo-workspace, crate-boundaries, serde]
requires:
  - phase: MMX-01-contract-foundation
    provides: language-neutral compatibility baseline
provides:
  - Pinned Rust 1.97.0 edition-2024 workspace with nine explicit crates
  - Audited and locked Serde-only dependency policy with no database crate
  - One-way crate boundaries and a development-only Rust CLI target
affects: [protocol, core, provider, tools, retrieval, vault, tui, cli, compat-harness]
tech-stack:
  added: [Rust 1.97.0, Cargo resolver 3, serde 1.0.228, serde_json 1.0.150]
  patterns: [composition-root CLI, inward-only dependencies, workspace-owned lints]
key-files:
  created:
    - rust-toolchain.toml
    - Cargo.toml
    - Cargo.lock
    - crates/protocol/src/lib.rs
    - crates/core/src/lib.rs
    - crates/cli/src/main.rs
  modified:
    - .gitignore
key-decisions:
  - "The existing npm CLI remains the product entry until Rust behavioral parity is proven."
  - "The local Windows 10 20H2 machine verifies pure Rust code with the official 1.97.0 windows-gnullvm toolchain; the product target remains MSVC and requires a supported runner gate."
patterns-established:
  - "Protocol is the lowest layer; core depends only on protocol; adapters point inward; CLI composes the system."
  - "Workspace dependencies are exact, audited, locked, and database-free."
requirements-completed: [ARCH-01, ARCH-02]
coverage:
  - id: D1
    description: "Rust 1.97.0, rustfmt, and Clippy are reproducibly pinned without changing the global default toolchain."
    requirement: ARCH-01
    verification:
      - kind: other
        ref: "rustc --version; cargo --version; rustup show active-toolchain"
        status: pass
    human_judgment: false
  - id: D2
    description: "One resolver-3 workspace contains exactly nine edition-2024 crates with locked Serde dependencies and no database package."
    requirement: ARCH-01
    verification:
      - kind: integration
        ref: "cargo metadata --locked --format-version 1; cargo tree --workspace --locked"
        status: pass
    human_judgment: false
  - id: D3
    description: "The crate graph points inward, every library declares its ownership boundary, and the existing npm CLI remains unchanged."
    requirement: ARCH-02
    verification:
      - kind: integration
        ref: "cargo +1.97.0-x86_64-pc-windows-gnullvm check --workspace --all-targets --locked"
        status: pass
      - kind: integration
        ref: "cargo +1.97.0-x86_64-pc-windows-gnullvm clippy --workspace --all-targets --locked -- -D warnings"
        status: pass
      - kind: integration
        ref: "npm test (432/432)"
        status: pass
    human_judgment: false
duration: 23min
completed: 2026-07-15
status: complete
---

# Phase 1 Plan 2: Rust Workspace Scaffold Summary

**A nine-crate Rust 1.97 workspace now makes protocol, orchestration, adapters, retrieval, Vault, UI, composition, and compatibility responsibilities explicit without replacing the TypeScript product.**

## Performance

- **Duration:** 23 min
- **Started:** 2026-07-15T08:28:00Z
- **Completed:** 2026-07-15T08:51:06Z
- **Tasks:** 3
- **Files modified:** 22

## Accomplishments

- Installed and repository-pinned the official minimal Rust 1.97.0 toolchain with rustfmt and Clippy while leaving the global rustup default unset.
- Created one resolver-3, edition-2024 workspace containing the eight production crates and one non-published compatibility harness, with exact audited Serde versions and no SQLite or other database dependency.
- Documented and compiled the one-way crate graph; the Rust binary identifies itself as development-only while `package.json` still launches `dist/cli.js`.

## Task Commits

1. **Task 1: Install and pin the audited Rust toolchain** - `553f269`
2. **Task 2: Define the root workspace and dependency policy** - `558457e`
3. **Task 3: Scaffold explicit inward-pointing crate entry points** - `2f3cc6f`

## Files Created/Modified

- `rust-toolchain.toml` - Pins Rust 1.97.0, minimal profile, rustfmt, and Clippy.
- `Cargo.toml` - Owns the nine workspace members, edition/rust-version, exact dependencies, and lints.
- `Cargo.lock` - Reproducibly locks the audited Serde dependency graph.
- `crates/*/Cargo.toml` - Declares the permitted one-way crate dependencies.
- `crates/*/src/lib.rs` - States each library's ownership boundary through crate documentation and `CRATE_ROLE`.
- `crates/cli/src/main.rs` - Provides an explicit development-only Rust binary without changing npm routing.
- `.gitignore` - Excludes only the Cargo `/target/` directory.

## Decisions Made

- Kept the npm CLI as the only product entry; the Rust binary cannot silently replace it before parity.
- Kept the target architecture on MSVC while using the official Rust `windows-gnullvm` host only as a machine-local compile path on this unsupported Windows installation.
- Kept dependency versions exact in the workspace root so later crates cannot independently drift.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Created minimal member targets while generating the workspace lockfile**

- **Found during:** Task 2 (root workspace and dependency policy)
- **Issue:** Cargo cannot generate a lockfile or metadata for declared members before their manifests and targets exist, although the plan listed package scaffolding in Task 3.
- **Fix:** Created the minimal nine package manifests and targets during Task 2, then replaced their boundary-only source bodies in Task 3.
- **Files modified:** `crates/*/Cargo.toml`, `crates/*/src/*.rs`
- **Verification:** `cargo metadata --locked` lists exactly nine members and `cargo tree --workspace --locked` resolves successfully.
- **Committed in:** `558457e`

**2. [Rule 3 - Blocking] Used the official Rust gnullvm host for local compilation**

- **Found during:** Task 3 compile verification
- **Issue:** The MSVC host could not link because Visual C++ Build Tools were absent. After explicit user approval, the current Microsoft installer passed its published hash check but rejected Windows 10 Education 20H2 as unsupported and required at least 6.8 GB on C: where 3.1 GB was free.
- **Fix:** Installed the official Rust 1.97.0 `x86_64-pc-windows-gnullvm` toolchain and used its bundled `rust-lld` for local pure-Rust compilation. No product file, target policy, global default, personal file, or project entry was changed.
- **Files modified:** None (machine-local toolchain only)
- **Verification:** fmt, check, Clippy with `-D warnings`, tests, and the development CLI all pass under 1.97.0 gnullvm.
- **Committed in:** N/A (environment-only remediation)

**Total deviations:** 2 auto-fixed (2 blocking execution prerequisites).
**Impact:** The planned crate architecture and product target are unchanged. Plan 01-04 must retain a supported Windows/MSVC runner as the release-facing proof.

## Issues Encountered

- The Visual Studio installer surfaced a generic policy message, but its detailed logs identified the real failures as unsupported OS and insufficient C: space. No disk cleanup or OS upgrade was performed because neither would safely solve both constraints.

## User Setup Required

None - the machine-local verification toolchain is installed and the repository selects its pinned toolchain normally.

## Next Phase Readiness

- The one-way workspace is ready for typed protocol events, the core sequence reducer, and Provider normalization in Plan 01-03.
- Local gnullvm proves the pure Rust workspace; supported Windows/MSVC and Linux evidence remains a Plan 01-04 CI responsibility.

## Self-Check: PASSED

- All 22 claimed workspace/toolchain files exist and all three task commits are present.
- Cargo metadata/tree report nine exact edition-2024, Rust-1.97 members with no database dependency.
- Rust fmt, check, Clippy, tests, and the development-only CLI pass under official Rust 1.97.0 gnullvm.
- `npm run check`, all 432 TypeScript tests, and `git diff --check` pass; `package.json` remains unchanged.

---
*Phase: MMX-01-contract-foundation*
*Completed: 2026-07-15*
