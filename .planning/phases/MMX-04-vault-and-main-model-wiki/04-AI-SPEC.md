# AI-SPEC — Phase 4: Vault and Main-Model Wiki

> AI design contract for the `MainModelWikiWorkflow`. The production runtime remains native Rust; this contract prevents the model from owning persistence or changing its identity silently.

---

## 1. System Classification

**System Type:** Hybrid autonomous-agent subworkflow plus structured knowledge extraction

**Description:**
A finalized MiniMax Codex session is locally classified for durable value. When durable, the same Provider/model pinned to that session receives bounded raw evidence and relevant current Wiki context, then proposes a structured `KnowledgePatch`. Core validates it and the Vault adapter commits ordinary Markdown through a recoverable transaction. Good output is minimal, source-grounded, non-secret, idempotent, and leaves one current truth per topic.

**Critical Failure Modes:**
1. The model invents a durable claim or cites a raw source that does not support it.
2. The workflow silently switches to another model, repeats a paid call, or hides its separate usage.
3. Model output writes directly to the Vault, escapes Agent-owned paths, or overwrites an externally modified page.
4. A crash creates duplicate pages, loses finalized raw evidence, or leaves two current truths.
5. Credentials, private raw reasoning, or untrusted embedded instructions enter Wiki content.

---

## 1b. Domain Context

**Industry Vertical:** Developer tooling and local knowledge management

**User Population:** Non-programmers and developers using a local coding/task agent whose session history becomes long-term project knowledge

**Stakes Level:** High

**Output Consequence:** Future model context, project decisions, debugging guidance, and user actions can depend on the compiled Wiki. A plausible unsupported summary can repeatedly misdirect later work.

### What Domain Experts Evaluate Against

| Dimension | Good — expert accepts | Bad — expert flags | Stakes |
|-----------|------------------------|--------------------|--------|
| Provenance | Every durable claim cites one or more existing raw source IDs that support it | Missing, fabricated, circular, or irrelevant source | Critical |
| Minimal delta | Patch changes only knowledge made necessary by this session | Rewrites unrelated pages or copies full conversation | High |
| Current truth | A replacement has explicit supersession and exactly one current page | Conflicting current conclusions or hidden history deletion | Critical |
| Fidelity | Decisions, diagnostics, and constraints retain qualifiers and uncertainty | Summary turns tentative evidence into certainty | High |
| Privacy | No credential, secret, private raw reasoning, or unsafe attachment body enters Wiki | Secret or sensitive transient data is persisted | Critical |
| Recoverability | Reapplying the same patch is a no-op and crash recovery reaches the expected hash | Duplicate page/log entry or unrecoverable half-write | Critical |

### Known Failure Modes in This Domain

- Summary drift: repeated Wiki-to-Wiki paraphrase loses the original evidence and becomes self-referential.
- Authority laundering: a model-generated sentence is later treated as user-confirmed fact because provenance is absent.
- Context poisoning: instructions embedded inside imported/raw text are followed instead of summarized as untrusted data.
- Over-crystallization: ordinary chat creates noisy pages that crowd out useful retrieval and spend quota.
- Stale truth: a new decision is appended while the old conclusion remains current.
- Model identity drift: a retry uses a different model and produces an unacknowledged change in summarization policy.

### Regulatory / Compliance Context

No domain-specific regulation is assumed. General privacy, credential handling, source attribution, user-requested deletion, and local filesystem protection constraints apply. `vault forget` must remove affected compiled claims before deleting referenced evidence and leave a non-secret tombstone.

### Domain Expert Roles for Evaluation

| Role | Responsibility |
|------|---------------|
| Product owner/user | Confirms whether durable decisions and preferences are represented faithfully |
| Agent-runtime maintainer | Labels protocol, idempotency, model-pinning, and recovery fixtures |
| Security reviewer | Reviews secret, path ownership, prompt-injection, and deletion cases |

---

## 2. Framework Decision

**Selected Framework:** Native Rust core workflow; no external agent/RAG orchestration framework

**Version:** Workspace contract v1, dependencies pinned by `Cargo.lock`

**Rationale:**
The workflow is a small, explicit state machine inside an existing Rust agent runtime, not a general-purpose RAG application or multi-agent graph. Its load-bearing requirements are typed events, model identity pinning, file transaction recovery, and strict port ownership. A Python/TypeScript framework would introduce a second runtime, duplicate persistence, and weaken the chosen core/Provider/Vault boundaries. Rust `serde` supplies typed serialization, `schemars` can derive a JSON Schema, and a reusable JSON Schema validator plus semantic validation can reject malformed model output before persistence.

**Alternatives Considered:**

| Framework | Ruled Out Because |
|-----------|------------------|
| LangGraph | Adds a second language/runtime and persistence graph for a linear, already-specified Rust state machine |
| LlamaIndex | Excellent for document RAG, but this phase needs evidence-bound patch generation and file transaction ownership, not a RAG framework |
| OpenAI Agents SDK | Provider-specific and duplicates the project's own provider/tool/session runtime |
| LangChain | Broad abstraction surface without solving the Rust transaction and ownership requirements |

**Vendor Lock-In Accepted:** No framework lock-in; the user intentionally pins the session's selected Provider/model for each Wiki job

---

## 3. Framework Quick Reference

### Installation

```toml
[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
schemars = "1"
jsonschema = "0.47"
```

Versions are finalized and locked during the implementing phase after the supported Rust toolchain is verified. The AI contract depends on their public behavior, not an unpinned network fetch.

### Core Imports

```rust
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
```

### Entry Point Pattern

```rust
pub async fn run_wiki_job(
    ports: &impl WikiWorkflowPorts,
    job: PendingWikiJob,
) -> Result<WikiReceipt, WikiWorkflowError> {
    ports.assert_pinned_model(&job.model_binding)?;
    let evidence = ports.load_bounded_evidence(&job.source_ids).await?;
    let raw = ports.generate_patch(&job.model_binding, evidence).await?;
    let patch = ports.validate_patch(raw, &job).await?;
    ports.commit_patch(patch, &job.expected_state).await
}
```

### Key Abstractions

| Concept | What It Is | When You Use It |
|---------|------------|-----------------|
| `PendingWikiJob` | Durable session/model/source binding and expected state | Before any model call or retry |
| `KnowledgePatch` | Deny-unknown-fields structured proposal | Only model output accepted by core |
| `PatchValidator` | Schema, provenance, policy, secret, and semantic checks | Before Vault transaction creation |
| `WikiReceipt` | No-op/synthesized/pending/failed durable outcome with usage | Recovery, audit, and duplicate suppression |
| `KnowledgePort` | Read current Wiki and submit validated patch | Core-to-Vault boundary |

### Common Pitfalls

1. Treating successful JSON deserialization as sufficient; provenance, ownership, supersession, and expected hashes still require semantic validation.
2. Deriving a schema without pinning/fixture-testing its emitted form; Schemars documents that generated schema shape may change across versions.
3. Retrying a timed-out model call without a stable job ID and call receipt, which can double-spend and create duplicate patches.
4. Feeding the entire Wiki or session to the model instead of bounded evidence, which increases drift, cost, and prompt-injection exposure.

### Recommended Project Structure

```text
crates/core/src/wiki_workflow/
  state.rs
  durability_gate.rs
  prompt.rs
  validate.rs
  workflow.rs
crates/protocol/src/knowledge_patch.rs
crates/vault/src/wiki_transaction.rs
crates/compat-harness/fixtures/wiki/
```

---

## 4. Implementation Guidance

**Model Configuration:** Reuse the session's immutable `ProviderId + ModelId + protocol + relevant settings` binding. Default temperature is the session setting; the prompt requests concise evidence-bound output. The response token cap and evidence budget are explicit job fields and are recorded in the receipt.

**Core Pattern:** Durable state machine: `evaluation_pending -> no_op | synthesis_pending -> generating -> validating -> committing -> synthesized`, with `failed_retryable` and `failed_terminal` outcomes. Raw finalization precedes evaluation creation. Every transition is journaled before the next external action.

**Tool Use:** The Wiki workflow exposes no shell or filesystem tools to the model. It supplies raw evidence and current-page excerpts as untrusted data. The only model output is `KnowledgePatch`; core and Vault perform validation and persistence.

**State Management:** Stable evaluation/job IDs derive from source session identity. Receipts record model binding, prompt/schema version, input source hashes, usage, patch hash, transaction ID, and outcome. Vault transactions use expected hashes and idempotent roll-forward.

**Context Window Strategy:** Local DurabilityGate avoids unnecessary calls. Selected raw events are capped and source-labeled; only directly relevant current Wiki pages are included. The model never receives the entire Vault, GC trash, credentials, or private raw reasoning.

---

## 4b. AI Systems Best Practices

### Structured Outputs with Rust and a Pydantic Eval Mirror

Production Rust is authoritative:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KnowledgePatch {
    pub schema_version: u16,
    pub source_ids: Vec<String>,
    pub operations: Vec<KnowledgeOperation>,
}
```

A Pydantic mirror is allowed only in offline evaluator/fixture tooling when cross-language schema conformance is useful; it is not a production runtime dependency:

```python
from pydantic import BaseModel, ConfigDict, Field

class KnowledgePatchFixture(BaseModel):
    model_config = ConfigDict(extra="forbid")
    schema_version: int = Field(ge=1)
    source_ids: list[str] = Field(min_length=1)
    operations: list[dict] = Field(max_length=32)
```

Validation order is JSON parse -> generated schema -> typed deserialize -> source/path/secret/supersession semantic checks -> expected-hash transaction check. A schema failure may request one bounded repair attempt under the same model binding; semantic or secret failures do not auto-repair with broader context.

### Async-First Design

Provider generation and Vault I/O are awaited through ports; no lock is held across network I/O. Cancellation records a retryable receipt before returning. Streaming model text is not exposed as a partially trusted patch; only the complete validated object advances.

### Prompt Engineering Discipline

System instructions define the patch contract and prohibitions. Raw evidence and Wiki excerpts are wrapped as untrusted, source-labeled data. Few-shot examples cover create/update/no-op/supersede and invalid-source refusal. Prompt and schema versions are stored with the job.

### Context Window Management

Use deterministic source selection and hard byte/token budgets. Prefer typed outcome, changed files, decisions, diagnostics, and explicit todo events. Existing Wiki is context only and cannot become a new source ID.

### Cost and Latency Budget

At most one normal generation plus one schema-repair attempt per job. No-op gate calls cost zero model tokens. UI reports input/output usage separately from the main session. Tests use mock Providers; real cost thresholds are configured only after explicit quota authorization.

---

## 5. Evaluation Strategy

### Dimensions

| Dimension | Rubric | Measurement Approach | Priority |
|-----------|--------|----------------------|----------|
| Schema validity | Pass only if exact schema and deny-unknown-fields checks succeed | Code | Critical |
| Provenance faithfulness | Pass only if every claim maps to supporting raw source IDs; 5 = exact, 1 = unsupported | Code + calibrated human review | Critical |
| Minimality | Pass when unrelated pages and full transcript text are absent | Code + human | High |
| Current-truth correctness | Pass when one current page remains and supersession is complete | Code | Critical |
| Privacy and injection resistance | Pass when secrets/instructions in evidence never become commands or persisted secrets | Code + security review | Critical |
| Idempotency/recovery | Pass when repeated apply and every injected crash point converge to one expected hash | Code | Critical |
| Gate precision | Pass target: all durable fixtures synthesize and all trivial fixtures no-op | Code | High |
| Model identity/cost visibility | Pass when binding and separate usage are present and retries never silently change model | Code | Critical |

### Eval Tooling

**Primary Tool:** Native Rust fixture runner (`cargo test`) with deterministic mock Provider and fault injection

**Observability Override:** Arize Phoenix is intentionally not a v1 default. Requiring a Python tracing service would violate the single-binary/local-first boundary. Safe local DomainEvents and receipts provide the required trace; a later optional exporter may target OpenTelemetry/Phoenix without changing core.

**Setup:**

```bash
cargo test -p minimax-core wiki_workflow
cargo test -p minimax-vault wiki_transaction
cargo test -p minimax-compat-harness wiki_eval
```

**CI/CD Integration:**

```bash
cargo test --workspace --locked
```

### Reference Dataset

**Size:** 20 hand-labeled offline fixtures initially

**Composition:** 4 no-op sessions; 3 new-decision creates; 3 updates/supersessions; 2 diagnostics/lessons; 2 invalid or missing sources; 2 secret/prompt-injection cases; 2 crash/retry cases; 1 external-edit conflict; 1 unavailable-pinned-model case.

**Labeling:** The user/product owner labels semantic durability and fidelity; runtime maintainer labels protocol/recovery; security reviewer labels secret/injection cases. No LLM judge is trusted until calibrated against those labels.

---

## 6. Guardrails

### Online (Real-Time)

| Guardrail | Trigger | Intervention |
|-----------|---------|--------------|
| Durability gate | Session lacks durable typed signals | Write no-op receipt; do not call model |
| Model binding | Current Provider/model differs from job binding | Keep pending and require explicit rebind |
| Schema/size | Malformed, unknown fields, too many operations, or oversize output | Reject; at most one bounded schema repair |
| Provenance | Missing/nonexistent/unsupported source ID | Reject patch and preserve pending evidence |
| Ownership/path | Operation targets non-Wiki or externally changed file | Fail closed; never overwrite |
| Secret/injection | Secret pattern or instruction-following artifact detected | Reject, redact diagnostic, flag security event |
| Expected hash | Current state differs from transaction precondition | Abort commit and re-evaluate from current state |

### Offline (Flywheel)

| Metric | Sampling Strategy | Action on Degradation |
|--------|------------------|-----------------------|
| Unsupported-claim rate | Review every validation failure plus 10% of successful patches | Tighten source selection/prompt; expand fixtures |
| Over-crystallization rate | Sample no-op/synthesis boundary weekly | Adjust typed DurabilityGate rules, not a hidden model classifier |
| Duplicate/supersession defects | Run full fault matrix on every Vault change | Block merge and repair transaction/state logic |
| Usage drift | Compare per-job input/output distribution by prompt version | Reduce context budget or investigate selection regression |

---

## 7. Production Monitoring

**Tracing Tool:** Built-in safe local DomainEvent/receipt trace; optional external exporter deferred

**Key Metrics to Track:** no-op vs synthesis counts, schema/provenance rejection counts, job latency, separate model usage, pending age, transaction recovery count, external-edit conflicts.

**Alert Thresholds:** Any secret persistence, unsupported committed claim, duplicate current page, silent model rebind, or unrecoverable transaction is release-blocking. Pending jobs older than the configured maintenance window and repeated retry failures surface actionable doctor warnings.

**Smart Sampling Strategy:** Always review hard-guardrail triggers and model rebind requests; oversample high-operation patches, supersessions, privacy deletions, and jobs with unusual usage/latency; randomly sample 10% of ordinary successful patches during pre-release evaluation.

---

## Checklist

- [x] System type classified
- [x] Critical failure modes identified (>=3)
- [x] Domain context documented
- [x] Regulatory/compliance context identified
- [x] Domain expert roles defined
- [x] Framework selected with rationale
- [x] Alternatives considered and ruled out
- [x] Framework quick reference written
- [x] AI systems best practices written with Rust authority and Pydantic eval mirror
- [x] Evaluation dimensions grounded in domain rubric ingredients
- [x] Each eval dimension has a concrete rubric
- [x] Eval tooling selected with explicit Phoenix override
- [x] Reference dataset has 20 labeled examples
- [x] CI/CD eval integration specified
- [x] Online guardrails defined
- [x] Production monitoring and sampling defined

## Primary References

- https://serde.rs/
- https://docs.rs/schemars/latest/schemars/
- https://docs.rs/jsonschema/latest/jsonschema/
