# Post-Merge Audit Remediation Design

**Status:** Approved in conversation on 2026-07-21

**Product:** `minimax-codex`

## Problem

The first public npm release proves that supported users can install the CLI
without Rust, but the post-merge audit found several gaps between the shipped
product and the approved coding-agent behavior:

- MiniMax Chat Completions can return reasoning inside `<think>` tags in the
  ordinary `content` stream, which is currently rendered and persisted;
- `/trace` changes an internal flag without rendering the safe work trace;
- `/model` creates a session with the requested model while leaving shell and
  provider state bound to the old model;
- first launch and missing-credential errors do not guide an installed user
  into a usable chat session;
- `/vault`, `doctor`, and migration documentation contain command, mutation,
  and release-version inconsistencies.

## Goal

Repair the audited defects without weakening the release gates or expanding
the supported platform and capability scope. A supported npm user should be
able to install the product, set one documented credential, start chatting,
switch models consistently, inspect a safe trace, and run diagnostics without
leaking reasoning or mutating an otherwise fresh workspace.

## Design

### Provider Reasoning Boundary

Chat Completions visible text passes through a stateful streaming filter before
it becomes a runtime event. The filter recognizes `<think>...</think>` even
when an opening or closing tag is split across SSE frames. Text inside the
block, the tags themselves, and incomplete reasoning fragments are fail-closed:
they are never rendered, journaled, or written to Vault. Ordinary visible text
before and after a reasoning block retains its order.

The Responses protocol keeps its existing explicit reasoning classification.
Both protocols report filtered reasoning through the existing safe lifecycle
signal rather than exposing raw reasoning content.

### Trace and Model State

Safe trace entries are derived only from allowlisted runtime lifecycle events
and persisted through the existing trace/session records. The default view is
folded. `/trace` toggles the current view and immediately renders the active
session trace; no provider reasoning payload is used as trace content.

The interactive shell owns one mutable active model binding. A successful
model switch creates the new session and then atomically updates shell status,
provider binding, subsequent requests, and Wiki synthesis validation. If any
step fails, the old binding remains active. `/models` reports the active value.

### First-Use and Diagnostics Experience

Running `minimax-codex` without a subcommand starts the normal chat flow. When
credentials are absent, the error names `MINIMAX_API_KEY`, gives safe shell
examples, and points to `minimax-codex doctor`. `/api` exposes the same guidance
without accepting or echoing a secret in the chat transcript.

`/vault` prints commands using the installed `minimax-codex` executable.
`doctor` uses a read-only runtime inspection path: it may report that runtime
state has not been initialized, but it does not create directories, locks,
journals, indexes, or sessions.

### Release Contract Accuracy

Migration support metadata and release documentation use the actual public
cutover version, `0.1.0`, rather than the stale planned `3.0.0` value. Product
changes intentionally invalidate the previous hosted source fingerprint. The
repair will not edit hosted evidence to manufacture freshness; a new authorized
CI/release evidence cycle must reseal the changed commit.

## Compatibility and Scope

The repair preserves the npm package name, CLI executable, on-disk schema,
provider endpoints, Windows x64 and Linux x64 support, and current capability
catalog. It does not add macOS or Arm binaries, a keyring-writing login command,
new paid API calls, or broader process-tool support on Windows.

## Verification

Each defect receives a failing regression test before implementation. Coverage
includes split-frame reasoning tags, persistence leak checks, trace toggling,
transactional model switching, no-subcommand startup, credential guidance,
read-only doctor behavior, installed command text, and `0.1.0` migration
metadata.

After focused tests pass, verification runs formatting, Clippy, the full Rust
workspace tests, provider and retrieval evaluations, compatibility checks, npm
package tests, and a local packed-package installation with lifecycle scripts
disabled. Strict hosted-freshness checks are expected to remain blocked until
CI produces new evidence for the repaired source fingerprint.

## Acceptance Criteria

- no `<think>` reasoning content reaches terminal output, session journals, or
  Vault, including when tags span stream frames;
- `/trace` renders only safe trace entries and accurately toggles folded and
  expanded views;
- `/model` and `/models` agree, and downstream provider/Wiki operations use the
  selected binding;
- a no-argument launch enters chat and missing credentials produce actionable,
  secret-safe setup guidance;
- `doctor` leaves a fresh workspace unchanged;
- user-facing commands reference `minimax-codex` and migration materials match
  the public `0.1.0` cutover;
- all non-hosted verification gates pass, while stale hosted evidence remains
  visibly stale until it is regenerated by authorized CI.
