# AI-SPEC - Phase 5: Retrieval and Project Discovery

> Generated inline through `$gsd-ai-integration-phase` because subagent dispatch was not authorized. The phase uses optional local embedding inference; deterministic lexical retrieval remains the authority and fallback.

## 1. System Classification

**System Type:** Hybrid retrieval / local semantic reranking

**Description:** A local developer-tooling search system ranks capabilities, open-source projects, and current Wiki pages. Good behavior means BM25 remains independently useful, semantic inference is visibly secondary and bounded to candidates, and every status/result can be audited from local facts.

**Critical Failure Modes:**

1. Embedding runs before BM25 or introduces an un-recalled project.
2. A missing/corrupt/stale model or vector resource is reported as healthy hybrid.
3. Result explanations invent license, maintenance, source, or compatibility facts.
4. One index returns another domain's document schema.
5. Optional inference failure removes otherwise useful BM25 results.

## 1b. Domain Context

**Industry Vertical:** Developer tooling and open-source software discovery

**User Population:** Primarily non-programmers describing desired outcomes, plus maintainers auditing retrieval behavior.

**Stakes Level:** Medium; a poor recommendation wastes time or exposes users to unsuitable software, while execution still requires a separate explicit workflow.

**Output Consequence:** Users inspect a ranked project list and may later choose to install or use one; this phase never executes it.

### What Domain Experts Evaluate Against

| Dimension | Good | Bad | Stakes |
|-----------|------|-----|--------|
| Need match | Top candidates directly support the stated task | Keyword coincidence dominates | Wasted time |
| Source truth | URL/license/maintenance fields come from catalog evidence | Missing facts are guessed | Trust and compliance |
| Ordering | BM25 candidate evidence is visible before semantic rerank | Vector search silently searches the whole catalog | Product contract violation |
| Degradation | Missing semantics still gives useful lexical candidates and a reason | Empty results or false hybrid status | Reliability |

### Known Failure Modes

- Popular but irrelevant repositories outrank task-specific projects.
- Chinese intent is split poorly and loses the action/object signal.
- Stale vector bundles no longer correspond to catalog text.
- An unknown license is rendered as permissive.
- A semantic helper hangs or returns NaN/wrong-size vectors.

### Regulatory / Compliance Context

No sector-specific regulation is asserted. License/source attribution, secret exclusion, truthful unknowns, and local-only default evaluation are mandatory product constraints.

### Domain Expert Roles for Evaluation

| Role | Responsibility |
|------|---------------|
| Open-source maintainer | Validate catalog facts and explanation usefulness |
| Non-programmer user | Label whether top projects match plain-language needs |
| Security/release maintainer | Audit helper isolation, hashes, license, and failure behavior |

## 2. Framework Decision

**Selected Framework:** Custom dependency-light Rust retrieval engine and fixed local helper protocol

**Version:** Workspace schema v1; tokenizer `mixed-zh-en-v1`; helper ABI v1

**Rationale:** The product needs a small Windows/Linux binary, deterministic BM25, strict domain isolation, no database, no bundled model, and an optional separately installed inference runtime. General RAG frameworks would add Python/JavaScript runtime and persistence abstractions while weakening the current Rust architecture boundaries.

**Alternatives Considered:**

| Framework | Ruled Out Because |
|-----------|------------------|
| LlamaIndex | RAG-focused runtime/dependency overhead and no need for document generation/orchestration |
| Haystack | Python deployment conflicts with the single Rust CLI and base-size goals |
| LangChain/LangGraph | Agent/workflow abstraction is unrelated to deterministic local ranking |
| SQLite/vector database | Conflicts with the selected transparent file Vault and is unnecessary at v1 scale |

**Vendor Lock-In Accepted:** Partial - the resource contract names Granite multilingual v2, but the helper port and retrieval engine are model-independent.

## 3. Framework Quick Reference

### Installation

```bash
# No retrieval framework or model is installed in the base product.
cargo test -p minimax-retrieval --locked
```

### Core Imports

```rust
use minimax_retrieval::{DomainIndex, ProjectDiscovery, RetrievalMode};
```

### Entry Point Pattern

```rust
let lexical = projects.search_lexical(&query, 20);
let result = discovery.rerank_verified(&query, lexical)?; // candidate subset only
assert!(result.items.iter().all(|item| result.bm25_candidate_ids.contains(&item.id)));
```

### Key Abstractions

| Concept | What it is | When used |
|---------|------------|-----------|
| `DomainDocument` | Typed exact keys and lexical text | Building any of three indexes |
| `LexicalIndex<D>` | Exact plus BM25 over one document type | Always available |
| `ProjectEmbeddingPort` | Candidate-bounded semantic operation | Only after BM25 |
| `VerifiedEmbeddingResource` | Hash/ABI/model/vector health proof | Before hybrid mode |

### Common Pitfalls

1. Deduplicating query terms before retaining their contribution evidence incorrectly.
2. Comparing floating scores without a stable ID tie-break or accepting NaN.
3. Computing semantic search over the complete catalog and filtering afterward.

### Recommended Project Structure

```text
crates/retrieval/src/
  normalize.rs exact.rs bm25.rs rrf.rs domain.rs
  capability.rs project.rs wiki.rs embedding.rs status.rs
```

## 4. Implementation Guidance

**Model Configuration:** Granite multilingual 97M r2, qint8, x64-AVX2, separately installed. Model revision, runtime ABI, tokenizer, dimensions, license, all file SHA-256 values, and catalog/vector fingerprint are strict manifest fields.

**Core Pattern:** lexical-first staged retrieval. Exact wins; otherwise BM25 returns candidate IDs and contribution keywords; optional verified helper returns finite vectors for that bounded set; RRF combines only the two candidate orders.

**Tool Use:** Fixed executable path from the verified resource, fixed argv, bounded JSON stdin/stdout, no shell, clean environment, deadline/kill, and no network/credential fields.

**State Management:** Immutable expected-hash JSON snapshots under Vault derived indexes. A failed rebuild retains the last-known-good snapshot; source Wiki/catalog files remain authoritative.

**Context Window Strategy:** No LLM context is used. Explanations are deterministic bounded fields; result count and document text/vector sizes are capped.

## 4b. AI Systems Best Practices

### Structured Outputs with Pydantic

The production implementation uses Rust `serde(deny_unknown_fields)`. This equivalent interop schema documents the helper boundary:

```python
from pydantic import BaseModel, ConfigDict, FiniteFloat

class EmbeddingResult(BaseModel):
    model_config = ConfigDict(extra="forbid")
    model_id: str
    fingerprint: str
    vectors: list[list[FiniteFloat]]
```

Validation has no model retry: malformed/wrong-dimension output immediately degrades to BM25.

### Async-First Design

The helper operation is isolated behind a port. CLI composition owns the deadline and cancellation; lexical retrieval remains synchronous and immediately available.

### Prompt Engineering Discipline

N/A - no generative prompt is used. Query text is normalized locally and never augmented by a model.

### Context Window Management

Candidate count, text bytes, vector dimensions, helper output bytes, and deadline are bounded before inference.

### Cost and Latency Budget

Remote cost is zero. BM25 p95 must remain <= 100 ms at 10k Wiki pages; semantic rerank has a 150 ms default deadline and always falls back lexically.

## 5. Evaluation Strategy

### Dimensions

| Dimension | Rubric | Measurement | Priority |
|-----------|--------|-------------|----------|
| Lexical recall | Recall@5/top1/no-match meet locked fixture gates | Code | Critical |
| Staged ordering | Embedding call occurs only after BM25 and sees candidate subset | Code | Critical |
| Resource truth | Every invalid resource reason degrades; hybrid requires all proofs | Code | Critical |
| Explanation truth | Every fact equals catalog/source data; unknown stays unknown | Code | High |
| Isolation | Cross-domain snapshot/document/result is rejected | Code | Critical |
| Latency | 10k Wiki BM25 p95 <= 100 ms | Code benchmark | High |

### Eval Tooling

**Primary Tool:** Rust integration tests plus the existing TypeScript 175-case offline evaluator. Arize Phoenix is intentionally not added: this local deterministic subsystem already has typed trace/JSONL facts, and adding a service conflicts with zero-dependency offline gates.

**CI/CD Integration:**

```bash
cargo test -p minimax-retrieval --locked
npm run eval:retrieval
```

### Reference Dataset

**Size:** Existing 175 capability queries plus at least 30 project/Wiki cases and a complete embedding failure matrix.

**Composition:** exact, Chinese/English lexical, ambiguous needs, no-match, project candidate ordering, unknown license/maintenance, current/superseded Wiki, and every resource/runtime degradation.

**Labeling:** Existing capability fixture is the compatibility baseline; project cases are reviewed against catalog facts and plain-language user intent.

## 6. Guardrails

### Online

| Guardrail | Trigger | Intervention |
|-----------|---------|--------------|
| Candidate subset | Semantic result ID absent from BM25 candidates | Reject semantic output; return BM25 |
| Resource proof | Any identity/hash/ABI/fingerprint/health failure | Disable hybrid with stable reason |
| Vector validity | NaN/infinite/wrong dimension/oversize | Reject semantic output; return BM25 |
| Fact provenance | Explanation field absent from catalog | Render unknown; never infer |

### Offline

| Metric | Sampling | Action |
|--------|----------|--------|
| Recall/top1/no-match | Every fixture in CI | Block merge/cutover |
| Latency p95 | Recorded 10k benchmark | Block release if >100 ms |
| Degradation matrix | Every CI run | Block hybrid activation regression |

## 7. Production Monitoring

**Tracing Tool:** Existing bounded local typed trace/JSONL output (explicit Arize Phoenix override for offline/small-binary constraints).

**Key Metrics:** actual mode, degraded reason, lexical candidate count, semantic deadline/failure count, result click/selection signal (future non-sensitive aggregation).

**Alert Thresholds:** any non-candidate semantic ID; any hybrid report without matching fingerprint; lexical gate failure; BM25 p95 >100 ms.

**Smart Sampling Strategy:** review no-match, low-overlap hybrid, unknown-license, repeated-query, and user-rejected-result cases first.

## Checklist

- [x] System type and critical failures classified
- [x] Domain context, stakes, expert criteria, and compliance constraints identified
- [x] Custom Rust/helper framework selected with alternatives and lock-in documented
- [x] Installation, imports, entry pattern, abstractions, pitfalls, and structure documented
- [x] Structured helper output, async boundary, context, and latency strategy defined
- [x] Evaluation dimensions and 175+ case reference datasets specified
- [x] Online guardrails and offline release gates defined
- [x] Existing local typed tracing selected with explicit Phoenix override rationale
