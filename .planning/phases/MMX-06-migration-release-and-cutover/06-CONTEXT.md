# Phase 6 Context: Migration, Release, and Cutover

## User outcome

Existing TypeScript users can inspect exactly what will move, migrate only safe durable data without changing the source, verify or roll back the result, and then use a small Windows or Linux Rust release as the normal `minimax-codex` command. The TypeScript command remains explicitly available during the support window.

## Locked decisions

- **D-601:** Migration is a separate explicit workflow. `inventory` and `dry-run` are read-only; `apply` requires a saved plan plus an exact plan-hash confirmation; no startup path auto-migrates.
- **D-602:** The default TypeScript source is the project-local `.mini-codex` tree and the Rust target is the selected project root. Both roots are canonicalized, bounded, and reported.
- **D-603:** Inventory is deterministic and records source hashes, planned target schema/paths/hashes, excluded paths/records, symlinks, unsupported records, and target collisions before any write.
- **D-604:** The import allowlist is safe project configuration, threads/sessions, turns, visible user/assistant messages, bounded tool request/result facts, and capability metadata.
- **D-605:** Credentials, credential-bearing headers, environment values, plaintext secret files, traces/private reasoning, summaries, indexes/caches, locks, databases, repair fragments, and secret-looking record content are excluded rather than copied.
- **D-606:** TypeScript session data is normalized into versioned Rust `SessionRecordV1` creation records; imported IDs are namespaced deterministically and the resulting journal must replay through `SessionMachine` before publication.
- **D-607:** Apply re-inventories the source and rejects drift. Existing identical targets are reused; any different existing target is a blocking collision. Staging and an operation manifest support in-process/crash recovery.
- **D-608:** The source is never written, renamed, or deleted. A receipt lists source fingerprint, plan hash, created/reused targets, and target hashes. Repeating the same apply is idempotent.
- **D-609:** Rollback requires the receipt hash, verifies current target hashes, and removes only files marked created by that migration. Reused or user-modified targets are never removed; the immutable apply receipt remains as audit evidence.
- **D-610:** Release artifacts are versioned, embedding-free base archives for Windows x64 MSVC and Linux x64 GNU, each with SHA-256 and a machine-readable manifest.
- **D-611:** Packaging uses the compiled release binary and a shell-free Node launcher. The default npm `minimax-codex` bin changes to the launcher only after all mandatory local and hosted gates pass; `minimax-codex-legacy` keeps the TypeScript entry during the support window.
- **D-612:** The launcher selects only a fixed platform/architecture binary path and never downloads, shells, reads credentials, or silently falls back. A clear unsupported/missing-artifact error points to the legacy command.
- **D-613:** Embedding weights/runtime remain a separate optional package and are never included in the base artifact, npm package, install, upgrade, or rollback flow.
- **D-614:** Release gates enforce cold start <= 500 ms, idle RSS <= 150 MB, compressed base artifact <= 50 MB, and 10k Wiki BM25 p95 <= 100 ms with recorded environment and samples.
- **D-615:** Offline CI covers formatting, Clippy, Rust/TypeScript tests, contracts, parity, recovery, security, migration, retrieval/provider evaluations, packaging, checksums, licenses, and performance without credentials, Provider calls, downloads, or API spend.
- **D-616:** The supported release matrix is Windows x64 MSVC and Linux x64 GNU. The local Windows GNU-LLVM toolchain is development evidence only; hosted Windows MSVC and Linux gates are mandatory cutover evidence.
- **D-617:** Compatibility status becomes green only from executable evidence. Rust provider profiles, migration, release, and product-entry claims may not be marked matched early.
- **D-618:** No tag, package publication, PR, merge, source cleanup, real-data migration, or TypeScript source deletion belongs to this phase execution without separate authorization.

## Boundaries

In scope: deterministic migration plan/apply/verify/rollback, fixtures and recovery tests, release manifest/checksum archives, launcher and legacy entry, Windows/Linux CI, license/security/performance gates, parity evidence, install/upgrade/rollback/support documentation, and gated default-entry cutover.

Out of scope: migrating the user's real home/project data during development, publishing npm or release assets, downloading embeddings, live Provider calls, macOS packaging, database introduction, deleting TypeScript source, PR, or merge.

## Prior architecture influence

As in the Codex and claw-code repository comparison, the public launcher stays thin, core records remain typed and replayable, platform packaging is an adapter concern, and every irreversible-looking operation is actually represented by an explicit plan and receipt. The Obsidian Vault remains the knowledge authority; migration introduces no SQLite or second database.
