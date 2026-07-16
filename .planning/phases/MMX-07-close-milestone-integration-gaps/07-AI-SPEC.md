# Phase 7 AI Design Contract

## AI Boundary

The only new AI call is the already-designed `MainModelWikiWorkflow` generation step. It uses the exact `ModelBinding` stored in finalized session evidence. Retrieval, durability classification, validation, Vault writes, recovery, migration, packaging, and project discovery remain deterministic local code.

## Input Contract

- One validated `KnowledgeEvaluationJob` derived from finalized evidence.
- Bounded visible user/assistant evidence with source ID and source hash; no raw reasoning or credential material.
- Bounded excerpts of current Wiki pages with current hashes.
- A schema-versioned instruction requiring one JSON `KnowledgePatch` and source citations.
- Maximum evidence: 256 KiB. Maximum output: 2,048 tokens. No tools.

## Output Contract

The Provider may return only visible text and usage followed by one terminal outcome. Text must parse as strict `KnowledgePatch` JSON. Core then enforces job identity, source hashes, paths, sizes, ownership, expected hashes, current-truth uniqueness, supersession, secrets, and injection markers before the Vault writer sees it.

## Failure and Recovery

- Wrong Provider/model/protocol binding: `model_binding_mismatch`, no call/write.
- Network/provider unavailable: durable pending receipt, no Wiki mutation.
- Invalid JSON/schema: one schema-repair request, then failed receipt.
- Unsafe/stale patch: failed receipt, no Wiki mutation.
- Crash after generation/commit: append-synced workflow facts make restart idempotent; unknown generation is never replayed automatically.

## Evaluation

- Scripted no-op path proves zero Provider calls.
- Scripted valid patch proves separate usage, validation, transaction, and retrieval.
- Wrong binding, malformed output, fabricated source, stale hash, crash boundaries, and repeat execution fail closed or converge once.
- Live Provider spend is explicitly outside automated verification.
