# Phase 1 Research: Contract Foundation

**Date:** 2026-07-15
**Scope:** Rust toolchain/workspace, language-neutral compatibility fixtures, typed protocol contracts, deterministic offline verification

## Recommendation

Use the official Rust 1.97.0 toolchain and a root edition-2024 Cargo workspace. Keep Phase 1 dependencies deliberately small: Serde and serde_json only. Express public commands/providers as JSON manifests and streaming cases as JSONL so both the TypeScript baseline and Rust harness can consume identical inputs. Put sequence legality in `minimax-core`; wire parsing stays in `minimax-provider`; reports stay in the dev-only harness.

## Environment Findings

- `rustc`, `cargo`, and `rustup` are absent from PATH on the current Windows host.
- The official Rust project published 1.97.0 on 2026-07-09 and recommends rustup as the normal installer/toolchain manager.
- Windows may also require the MSVC C++ build tools/linker; preflight must detect this before code execution continues.
- The TypeScript baseline already has provider conformance fixtures for Responses and Chat Completions plus tests for malformed/premature streams and secret-safe failures.

## Package Legitimacy Audit

| Package/tool | Status | Evidence | Selected range | Why needed |
|--------------|--------|----------|----------------|------------|
| Rust/rustup | VERIFIED | Official rust-lang install page and Rust 1.97.0 release announcement | 1.97.0 | Compiler, Cargo, formatter, Clippy |
| serde | VERIFIED | Official serde.rs and docs.rs project documentation | 1.0.228 | Typed command/event/manifest serialization |
| serde_json | VERIFIED | Official docs.rs crate documentation | 1.0.150 | JSON/JSONL fixtures and normalized reports |

No `[ASSUMED]`, `[SUS]`, or `[SLOP]` package is proposed. Phase 1 must stop and update this audit before adding any other Cargo dependency.

## Contract Shape

### Protocol Layer

- Versioned IDs and enums live in `minimax-protocol`.
- Public serialization uses stable snake_case tagged enums.
- Unknown event `type` is a protocol error in Phase 1; unknown fields on known v1 records are rejected for fixtures to expose drift early.
- Terminal outcomes are completed, failed, interrupted, or stopped; a sequence accepts exactly one terminal.

### Provider Layer

- Provider-specific JSON/SSE maps to provider-neutral stream events.
- Responses and Chat Completions share expected output fixtures but retain input protocol metadata.
- Private reasoning is either ignored or represented only as a safe category; its raw text never appears in normalized user-visible output.

### Compatibility Manifests

- `commands.v1.json`: public slash name, aliases, argument shape, and outcome kind.
- `providers.v1.json`: built-in/custom classes, protocol, base URL policy, secret binding names, and features.
- `provider-streams/*.jsonl`: raw event lines plus expected normalized events/errors.
- `baseline-status.v1.json`: all compatibility items initially `pending` except contracts proven by tests.

## Validation Architecture

### Fast checks

- Serde round trips for every protocol enum.
- Sequence reducer property-style table over valid and invalid terminal orderings.
- Manifest uniqueness/schema tests in both TypeScript and Rust.
- Two report runs compared byte-for-byte after sorting and deterministic ID/time injection.

### Integration checks

- `cargo metadata --locked` graph assertions.
- Compat harness reads repository fixtures from the workspace root and emits normalized JSON.
- Existing npm typecheck/test suite runs after Rust additions.

### Failure injection

- malformed JSON, unknown event, missing tool call ID, premature EOF, duplicate terminal, event after terminal, secret-bearing provider failure, and command alias collision.

## Security Notes

All fixture content is untrusted data. It must never be interpreted as instructions by the harness. Secret-looking markers remain test data and normalized errors must not echo them. Package/toolchain acquisition uses official Rust distribution endpoints only.

## Sources

- https://blog.rust-lang.org/releases/latest/
- https://rust-lang.org/tools/install/
- https://serde.rs/
- https://docs.rs/serde/latest/serde/
- https://docs.rs/serde_json/latest/serde_json/
- `src/protocol.ts`
- `src/providers/provider-protocol.ts`
- `test/support/provider-conformance-suite.ts`
- `test/fixtures/providers/conformance/`
