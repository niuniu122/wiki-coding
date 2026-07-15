# Phase 1 Context: Contract Foundation

## Locked Implementation Decisions

- Keep one repository root containing both `package.json` and root `Cargo.toml`; Rust crates live under `crates/`.
- Pin Rust `1.97.0` in `rust-toolchain.toml` and use edition 2024.
- Name production packages `minimax-protocol`, `minimax-core`, `minimax-provider`, `minimax-tools`, `minimax-retrieval`, `minimax-vault`, `minimax-tui`, and `minimax-cli`; `minimax-compat-harness` is dev-only.
- Define compatibility fixtures as language-neutral JSON/JSONL under `fixtures/compat/`, not Rust test constants.
- Serialize public protocol types with Serde using explicit tagged enums and a schema version.
- Start with only the types/ports required by Phase 1 fixtures; stub crates stay intentionally thin.
- Run architecture validation from the compat harness over `cargo metadata`, so core cannot accidentally import adapters.
- Keep the existing npm bin and TypeScript tests unchanged; additive scripts may call Rust only after the toolchain exists.

## Patterns

- `protocol` owns IDs, commands, events, terminal outcomes, provider-neutral content/tool types, and serialization errors.
- `core` owns the sequence validator/state reducer and abstract clock/ID/provider ports.
- Provider wire fixtures are parsed by a provider crate adapter into protocol events; provider-specific fields never enter core.
- Compatibility status is one of `matched`, `pending`, or `approved_difference`, with evidence links.
- Deterministic reports sort entries and normalize time/IDs before comparison.

## Deferred Choices

- Async runtime and HTTP client details wait for Phase 2.
- TUI framework implementation details wait for Phase 2 (architecture assumes Ratatui/Crossterm style only).
- Tool execution, Vault libraries, BM25 library, embedding backend, and packaging tool wait for their owning phases.

## Safety and Verification

- No live network or credential tests.
- Initial dependency download/toolchain installation is environment setup; subsequent verification uses `--locked`.
- A phase-local summary records exact commands and any environmental prerequisite not available in CI.

<decisions>
- **D-01:** Rust 1.97.0 and edition 2024 are locked for this milestone baseline.
- **D-02:** Root workspace plus `crates/` is locked.
- **D-03:** TypeScript remains the product entry through Phase 5.
- **D-04:** Compatibility manifests and fixtures are language-neutral.
</decisions>
