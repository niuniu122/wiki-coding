# Post-Merge Audit Remediation Implementation Plan

> **Execution:** Apply the approved design incrementally with test-driven
> changes. Do not refresh hosted evidence locally and do not publish.

**Goal:** Close the post-merge security, state-consistency, first-use,
diagnostic, and release-contract defects while preserving the existing npm
distribution architecture.

**Architecture:** Keep provider parsing protocol-specific, then pass Chat
Completions text through a per-request stateful reasoning filter before runtime
sequence acceptance. Keep interactive state in one active `ModelBinding`, make
provider rebinding explicit, and expose trace records from the durable runtime
driver. Add a read-only runtime probe for doctor instead of opening the writer
store. Preserve hosted fingerprints as externally generated evidence.

**Tech stack:** Rust 1.97 workspace, Tokio, reqwest/SSE, Clap, JSONL runtime
store, Node.js 20 package-contract tests.

---

## Task 1: Block embedded Chat Completions reasoning

**Files:**

- Modify: `crates/provider/tests/http_stream.rs`
- Modify: `crates/provider/src/client.rs`
- Modify if required: `crates/provider/src/fixture_protocol.rs`
- Modify: `crates/provider/tests/provider_fixtures.rs`

1. Add regression cases where `<think>` blocks appear in one content delta and
   where opening/closing tags are split across multiple SSE events.
2. Assert the tests fail because raw reasoning currently appears in visible
   deltas.
3. Implement a per-stream fail-closed filter that emits only text outside the
   reasoning block and reports the existing `ReasoningFiltered` signal.
4. Assert filtered material is absent from the collected event serialization.
5. Run `cargo test -p minimax-provider --test http_stream --locked` and provider
   fixture tests.

## Task 2: Make model switching transactional

**Files:**

- Modify: `crates/cli/src/driver.rs`
- Modify: `crates/cli/src/main.rs`
- Modify: `crates/cli/tests/lifecycle_wiki.rs`
- Add or modify: `crates/cli/tests/state_authority.rs`

1. Add a regression test that switches from model A to model B, reports model
   B, and successfully performs Wiki generation with model B.
2. Assert the old provider binding causes the test to fail.
3. Add explicit provider rebinding and mutable active binding access, updating
   state only after session creation succeeds.
4. Route `/new`, `/models`, turns, retry, and Wiki finalization through the
   active binding.
5. Run the focused CLI state and Wiki lifecycle tests.

## Task 3: Connect the safe trace

**Files:**

- Modify: `crates/cli/src/driver.rs`
- Modify: `crates/cli/src/main.rs`
- Modify: `crates/core/src/trace.rs` only if an additional allowlisted fact is
  required
- Modify: `crates/cli/tests/restart.rs`
- Modify: `crates/tui/tests/command_render.rs`

1. Add tests proving lifecycle-derived trace entries persist across reopen and
   contain no prompt, response, secret, raw provider frame, or tool body.
2. Add a shell-level regression showing `/trace` toggles and renders the active
   trace rather than only printing a mode label.
3. Record allowlisted entries for turn start, provider completion/failure,
   interruption/recovery, and compaction through `SafeTraceRecorder`.
4. Expose the active session trace through the driver and render it with
   `EventRenderer::trace` in folded or expanded mode.
5. Run focused core, driver, restart, and TUI tests.

## Task 4: Repair first-use and command guidance

**Files:**

- Modify: `crates/cli/src/app.rs`
- Modify: `crates/cli/src/main.rs`
- Modify: `crates/cli/src/doctor.rs`
- Modify: `README.md`
- Modify: `docs/release/install-upgrade-rollback.md`
- Modify: `crates/cli/tests/headless.rs`

1. Add parsing/CLI tests proving an empty argument list selects chat and that
   missing credentials name `MINIMAX_API_KEY` without printing a secret.
2. Add regression assertions for `/api` and `/vault` installed-command text.
3. Make the top-level subcommand optional and map absence to default `ChatArgs`.
4. Centralize credential setup guidance for errors and `/api`.
5. Document PowerShell and POSIX environment setup plus no-Rust startup.
6. Run focused CLI tests.

## Task 5: Make doctor read-only

**Files:**

- Modify: `crates/vault/src/runtime/mod.rs`
- Modify: `crates/vault/src/runtime/journal.rs` or add a small read-only module
- Modify: `crates/cli/src/doctor.rs`
- Modify: `crates/vault/tests/runtime_store.rs`
- Modify: `crates/cli/tests/headless.rs`

1. Add a fresh-workspace test asserting doctor creates no `.minimax` paths.
2. Add initialized-runtime cases for valid, busy, and malformed state without
   acquiring a writer lease or repairing files.
3. Implement a bounded read-only inspection result and map it to doctor checks.
4. Confirm byte-for-byte filesystem state remains unchanged after inspection.
5. Run vault and doctor tests.

## Task 6: Correct cutover version authority

**Files:**

- Modify: `crates/compat-harness/src/migration_support.rs`
- Modify: `crates/compat-harness/tests/migration_support.rs`
- Modify: `fixtures/compat/migration/typescript-v1/support-window.v1.json`
- Modify: `README.md`
- Modify: `docs/release/cutover.md`
- Modify: `docs/release/install-upgrade-rollback.md`

1. Change regression expectations from `3.0.0` to the published `0.1.0`
   cutover.
2. Run migration-support tests and candidate compatibility verification.
3. Confirm strict hosted freshness fails only because the repaired source no
   longer matches the old CI evidence fingerprint.

## Task 7: Full verification and final review

1. Run `cargo fmt --all -- --check`.
2. Run Clippy and all workspace tests with the available GNU-LLVM Windows
   toolchain when MSVC linking is unavailable.
3. Run provider and retrieval evaluations, candidate compatibility verification,
   and all npm package-contract tests.
4. Build/pack the candidate and install it into a clean npm prefix with
   `--ignore-scripts`; run `--version`, `doctor`, and no-subcommand first-use
   smoke checks without Rust in the consumer path.
5. Inspect `git diff`, secrets, generated files, and hosted-evidence files.
6. Report passing gates and the externally blocked hosted reseal; do not push,
   tag, or publish without a new user instruction.
