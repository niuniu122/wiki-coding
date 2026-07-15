# Ingested Constraints

- protocol: Cargo workspace dependencies are one-way; core depends on ports, never adapters.
- schema: Vault Markdown carries stable IDs, project binding, provenance, status, and supersession metadata.
- nfr: cold start <= 500 ms excluding recovery/model load; idle RSS <= 150 MB; compressed base artifact <= 50 MB; BM25 p95 <= 100 ms at 10k Wiki pages.
- security: credentials never enter Vault/config/trace; environment variables take precedence over OS keyring; destructive migration and external-cost actions remain gated.
- compatibility: preserve public slash commands, Responses and Chat Completions behavior, built-in/custom OpenAI-compatible providers, and explicit idempotent migration.
- platform: Windows and Linux are the v1 support matrix.
- source: docs/superpowers/specs/2026-07-15-rust-vault-rewrite-design.md
