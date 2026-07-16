//! Stable data contracts shared across the Rust rewrite.
//!
//! This lowest-level crate owns serializable messages and events. It must not
//! depend on orchestration, providers, tools, retrieval, storage, or the UI.

mod event;
mod knowledge;
mod retrieval;
mod runtime;
mod session;
mod tool;
mod vault;

pub use event::{
    ProtocolErrorCode, ProviderProtocolKind, SCHEMA_VERSION, SchemaVersion, SessionId, StreamEvent,
    StreamEventV1, TerminalOutcome, ToolCallFragment, ToolCallId, TurnId, Usage,
    parse_stream_event_v1,
};
pub use knowledge::{
    EvidenceId, KnowledgeEvaluationJob, KnowledgeJobId, KnowledgeOperation, KnowledgePage,
    KnowledgePageStatus, KnowledgePatch, KnowledgeReceipt, KnowledgeReceiptOutcome,
    KnowledgeValidationError, MAX_KNOWLEDGE_BODY_BYTES, MAX_KNOWLEDGE_OPERATIONS,
    MAX_KNOWLEDGE_SOURCES, PageId, SourceCitation, TopicId, WikiWorkflowEvent, WikiWorkflowState,
    WikiWorkflowUsage,
};
pub use retrieval::{
    IndexDomain, IndexStatusRecord, RetrievalDegradedReason, RetrievalExplanation,
    RetrievalHitRecord, RetrievalMode, RetrievalResponse,
};
pub use runtime::{
    AgentLimits, AssistantToolCallBatch, ConversationItem, DiagnosticCode, MessageRole, ModelId,
    ModelMessage, OutputSettings, ProviderId, RequestId, RuntimeErrorCode, RuntimeEvent,
    RuntimeEventV1, RuntimeFailure, RuntimeTerminalOutcome, ToolResultMessage, TurnReceipt,
    TurnRequest, parse_runtime_event_v1,
};
pub use session::{
    CompactionId, CompactionPointer, CompactionRecentTurn, CompactionRecord, JournalRecord,
    ModelBinding, RecordId, RecoveryRecord, SessionRecord, SessionRecordV1, SessionStatus,
    ToolInvocationRecord, TraceCode, TraceEntry, TurnRecord, TurnStatus, VisibleMessage,
    parse_session_record_v1,
};
pub use tool::{
    MAX_TOOL_ARGUMENT_BYTES, MAX_TOOL_CODE_BYTES, MAX_TOOL_DESCRIPTION_BYTES, MAX_TOOL_NAME_BYTES,
    MAX_TOOL_RESULT_BYTES, ToolCall, ToolDecision, ToolDecisionKind, ToolDefinition, ToolEffect,
    ToolInvocation, ToolResult, ToolTerminalStatus, ToolValidationError, V1_TOOL_NAMES,
    validate_unique_call_ids,
};
pub use vault::{
    ContentHash, ForgetId, ForgetPlan, ForgetReceipt, GcCandidate, GcClass, GcId, GcPlan,
    GcReceipt, GcReceiptAction, InboxImportReceipt, InboxImportStatus, ProjectId, RawEvidenceKind,
    RawEvidenceManifest, RebuildReceipt, TransactionId, TransactionManifest, TransactionState,
    TransactionTarget, TrashEntry, TrashManifest, TrashState, VaultIssue, VaultIssueCode,
    VaultLintReport, VaultManifest, VaultOwnership, VaultReceipt, VaultRepairReceipt,
    VaultValidationError, validate_vault_relative_path,
};

/// Human-readable boundary used by architecture checks and documentation.
pub const CRATE_ROLE: &str = "stable serializable contracts with no product-layer dependencies";
