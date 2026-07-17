//! Runtime orchestration and policy for the Rust rewrite.
//!
//! Core coordinates state transitions through protocol contracts. Concrete
//! providers, tools, retrieval engines, Vault storage, and UI code live outside
//! this crate so policy remains testable without side effects.

mod compaction;
mod knowledge;
mod ports;
mod runtime;
mod sequence;
mod session;
mod tool;
mod trace;

pub use compaction::{CompactionBudget, CompactionError, LocalCompactor};
pub use knowledge::{
    CurrentWikiPage, DurabilityCode, DurabilityDecision, DurabilityGate, DurabilitySignals,
    KnowledgeEffect, KnowledgeGuardError, KnowledgeInput, KnowledgePatchValidator,
    KnowledgeValidationContext, MainModelWikiWorkflow, WikiCurrentExcerpt, WikiEvidenceChunk,
    WikiGenerationError, WikiGenerationOutput, WikiGenerationRequest, WikiWorkflowError,
};
pub use ports::{
    ApprovalFuture, ApprovalPort, CancellationFuture, CancellationPort, Clock, FixedClock,
    FixedIdGenerator, IdGenerator, KnowledgeCommitError, KnowledgeCommitFuture, KnowledgePort,
    ToolFuture, ToolPort, WikiGenerationFuture, WikiGenerationPort,
};
pub use runtime::{RunEffect, RunInput, RunMachine, RunState};
pub use sequence::{NormalizedReplayRecord, StreamSequence, replay_stream};
pub use session::{SessionCommand, SessionEffect, SessionMachine, SessionSummary};
pub use tool::{
    AgentBudget, AgentBudgetError, BudgetKind, DecisionSnapshot, InvocationEffect, InvocationError,
    InvocationInput, InvocationMachine, InvocationRegistry, InvocationState, PermissionMode,
    ToolSandboxPolicy,
};
pub use trace::{FoldedTrace, SafeTraceFact, SafeTraceRecorder};

/// Human-readable boundary used by architecture checks and documentation.
pub const CRATE_ROLE: &str = "runtime orchestration and policy without concrete adapters";
