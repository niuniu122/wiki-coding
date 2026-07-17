# AI-SPEC - Phase 9: Capability Workspace and Non-Programmer Harness

> Generated inline through `$gsd-ai-integration-phase` because repository policy does not authorize subagent dispatch. This phase extends local retrieval and optional embedding; deterministic catalog and BM25 facts remain authoritative.

## 1. System Classification

**System Type:** Hybrid retrieval / external capability recommendation

**Description:** A local harness turns a non-programmer's plain-language need into explainable open-source project, Skill, and MCP candidates. Good behavior preserves typed category isolation, useful offline BM25 results, candidate-only semantic reranking, and a readiness explanation that never grants install or execution authority.

**Critical Failure Modes:**

1. A recommendation is automatically installed, authorized, or started.
2. Embedding searches the full catalog or returns a non-BM25 candidate.
3. Project, Skill, and MCP schemas or mutable runtime facts contaminate one another.
4. A missing permission, license, source, install, or authorization fact is invented.
5. Optional semantic failure removes useful lexical results or falsely reports hybrid mode.

## 1b. Domain Context

**Industry Vertical:** Developer tooling and agent capability discovery  
**User Population:** Non-programmers describing outcomes, plus maintainers curating audited metadata  
**Stakes Level:** Medium  
**Output Consequence:** A user may later choose to install or authorize third-party code, but this phase only supplies evidence and a safe next action.

| Dimension | Good | Bad | Stakes |
|-----------|------|-----|--------|
| Need match | Candidate directly supports the user's outcome | Keyword coincidence dominates | Wasted time and confusion |
| Readiness truth | Local inventory and declared requirements produce one accurate state | Catalog marketing text is treated as installed or authorized | Failed or unsafe setup |
| Authority | Search stops at explanation | Recommendation silently downloads, asks for secrets, or starts a process | Security and trust |
| Source truth | Unknown facts remain unknown and provenance is visible | License, permissions, maintenance, or compatibility is guessed | Compliance and safety |
| Retrieval order | BM25 candidates are visible before optional rerank | Vector search expands the candidate set | Contract violation |

**Known Failure Modes:** vague user language maps to popular but irrelevant tools; a Skill is mistaken for an MCP server; installation is treated as authorization; vector/catalog fingerprints drift; empty new catalogs crash unified search; unsafe metadata smuggles shell syntax into a suggested action.

**Regulatory / Compliance Context:** No sector-specific regulation is asserted. Source/license provenance, secret exclusion, least authority, local-only default evaluation, and truthful unknowns are mandatory product constraints.

| Role | Responsibility |
|------|----------------|
| Non-programmer reviewer | Label intent match and whether readiness text is understandable |
| Open-source maintainer | Validate catalog provenance and unknown handling |
| Security maintainer | Audit metadata parsing, authority boundary, and candidate-only inference |

## 2. Framework Decision

**Selected Framework:** Existing dependency-light Rust exact/BM25/RRF engine with the verified local embedding-helper ABI  
**Version:** Workspace schema v1; tokenizer `mixed-zh-en-v1`; helper ABI v1

**Rationale:** The repository already has deterministic typed retrieval, strict Serde contracts, a 20-candidate embedding boundary, and offline release gates. Extending those types preserves the small Rust binary and safety model. A general RAG framework would add an unrelated runtime and obscure the exact authority boundary.

| Framework | Ruled Out Because |
|-----------|------------------|
| LlamaIndex | Python/TypeScript runtime and document-QA abstractions are unnecessary for local typed card ranking |
| LangChain/LangGraph | Agent orchestration does not solve schema isolation or installation authority |
| Haystack | Python pipeline deployment conflicts with the single Rust CLI and offline base artifact |
| Vector database | The bounded catalogs do not justify a database and the product intentionally avoids one |

**Vendor Lock-In Accepted:** Partial - the verified resource currently names Granite multilingual, while the retrieval and helper ports remain model-independent.

## 3. Framework Quick Reference

### Installation

```bash
# No new framework or model is installed.
cargo test -p minimax-retrieval -p minimax-cli --locked
```

### Core Imports

```rust
use minimax_retrieval::{CapabilityWorkspace, CapabilityKind, EmbeddingSelection};
```

### Entry Point Pattern

```rust
let lexical = workspace.recall(&query, selected_kinds, 20);
let result = workspace.rerank(&query, lexical, embedding).await;
assert!(result.hits.iter().all(|hit| result.lexical_ids.contains(&hit.card.id)));
```

### Key Abstractions

| Concept | What It Is | When Used |
|---------|------------|-----------|
| `CapabilityCard` | Strict source facts shared by projects, Skills, and MCP servers | Catalog load and result rendering |
| Typed marker/index | Compile-time category boundary | Every lexical search |
| `CapabilityInventory` | Immutable local installed/authorized overlay | Readiness derivation only |
| `EmbeddingSelection` | Verified helper or stable degraded reason | After BM25 candidate recall |

### Common Pitfalls

1. Storing mutable `installed=true` in a source catalog.
2. Comparing raw BM25 scores from separate corpora without a stable merge rule.
3. Rendering an install hint as a shell command that a model could execute automatically.

### Recommended Project Structure

```text
capabilities/catalogs/
crates/retrieval/src/capability_workspace.rs
crates/cli/src/index.rs
crates/protocol/src/retrieval.rs
```

## 4. Implementation Guidance

**Model Configuration:** Optional verified Granite multilingual resource, qint8 x64-AVX2, with fixed model/revision/runtime/tokenizer/dimension/license/file/catalog/vector fingerprints.  
**Core Pattern:** Search selected typed indexes, merge bounded lexical ranks deterministically, optionally embed only that union, validate exact returned IDs and dimensions, then fuse lexical/semantic rankings.  
**Tool Use:** None. Catalog parsing and search perform file reads only when an explicit path is supplied.  
**State Management:** Source catalogs are immutable input. Installed and authorized ID sets are a separate immutable overlay; derived indexes are rebuildable.  
**Context Window Strategy:** No LLM context is needed for ranking. Prompt augmentation serializes only bounded result facts and a read-only authority notice.

## 4b. AI Systems Best Practices

The production schema uses Rust `serde(deny_unknown_fields)`. This interop example documents equivalent strict structured output:

```python
from typing import Literal
from pydantic import BaseModel, ConfigDict

class CapabilityResult(BaseModel):
    model_config = ConfigDict(extra="forbid")
    kind: Literal["project", "skill", "mcp"]
    readiness: Literal["ready", "needs_install", "needs_authorization"]
    id: str
```

Malformed semantic output is never retried with broader authority; it immediately falls back to BM25. Async is limited to the bounded helper process. No prompt engineering or generation occurs. Candidate count, catalog bytes, fields, vector dimensions, helper output, and deadline are capped. Remote inference cost is zero.

## 5. Evaluation Strategy

| Dimension | Rubric | Measurement | Priority |
|-----------|--------|-------------|----------|
| Kind isolation | Cross-kind document/catalog construction is rejected | Compile/unit tests | Critical |
| Lexical usefulness | Labeled ordinary needs return an applicable top result or truthful no-match | Fixture recall/top-k | Critical |
| Candidate boundary | Embedded and returned IDs exactly equal the bounded lexical union | Recording helper tests | Critical |
| Readiness truth | Installation precedes authorization and all three states are reachable | Table-driven tests | Critical |
| Authority | Search produces zero download/install/auth/process side effects | CLI/integration tests | Critical |
| Explanation truth | Every rendered fact equals catalog or inventory evidence | Round-trip/render tests | High |

**Primary Tool:** Rust tests and existing offline retrieval evaluation. Arize Phoenix is intentionally not added because this deterministic local subsystem already emits bounded typed facts and cannot justify a service dependency.

```bash
cargo test -p minimax-retrieval -p minimax-protocol -p minimax-tui -p minimax-cli --locked
npm run eval:retrieval
```

**Reference Dataset:** Start with 15 high-quality mixed Chinese/English project/Skill/MCP needs covering exact names, ordinary outcomes, ambiguous type, empty type, no-match, install, authorization, ready, unsafe metadata, and every semantic degradation class. Labels are reviewed by a non-programmer for clarity and by a maintainer for source truth.

## 6. Guardrails

| Guardrail | Trigger | Intervention |
|-----------|---------|--------------|
| Candidate subset | Semantic ID is absent, duplicated, or outside lexical union | Reject semantic output; preserve BM25 |
| Metadata safety | Unknown field, unsafe URL, executable command, control text, duplicate/cross-kind ID | Reject catalog |
| Readiness evidence | No installed/authorized inventory proof | Render the earlier unmet state |
| Authority | Search input requests install/execute semantics | Return evidence only; no side effect path exists |

Offline gates block regressions in fixture recall, strict schema, readiness, rendering parity, or no-side-effect behavior.

## 7. Production Monitoring

**Tracing Tool:** Existing bounded local typed JSON/text output; Phoenix override retained for offline and base-size constraints.  
**Key Metrics:** selected kinds, lexical candidate counts, actual mode, degraded reason, readiness distribution, and user-selected result (future opt-in signal).  
**Alert Thresholds:** any semantic outsider, hybrid without matching fingerprints, cross-kind schema acceptance, or discovery-side process/network call.  
**Smart Sampling Strategy:** Review no-match, repeated query, low-overlap hybrid, unknown-license/permission, and user-rejected-result cases first.

## Checklist

- [x] System type and five critical failures classified
- [x] Domain stakes, expert criteria, failure modes, and compliance constraints identified
- [x] Existing Rust/helper framework selected with alternatives documented
- [x] Entry pattern, abstractions, pitfalls, and structure documented
- [x] Strict structured output, async boundary, context, and latency strategy defined
- [x] Evaluation dimensions and 15-case starting dataset specified
- [x] Online guardrails and offline gates defined
- [x] Existing local tracing selected with explicit Phoenix override rationale
