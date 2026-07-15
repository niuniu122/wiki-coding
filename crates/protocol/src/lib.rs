//! Stable data contracts shared across the Rust rewrite.
//!
//! This lowest-level crate owns serializable messages and events. It must not
//! depend on orchestration, providers, tools, retrieval, storage, or the UI.

mod event;
mod runtime;
mod session;

pub use event::{
    ProtocolErrorCode, ProviderProtocolKind, SCHEMA_VERSION, SchemaVersion, SessionId, StreamEvent,
    StreamEventV1, TerminalOutcome, ToolCallFragment, ToolCallId, TurnId, Usage,
    parse_stream_event_v1,
};
pub use runtime::{
    DiagnosticCode, MessageRole, ModelId, ModelMessage, OutputSettings, ProviderId, RequestId,
    RuntimeErrorCode, RuntimeEvent, RuntimeEventV1, RuntimeFailure, RuntimeTerminalOutcome,
    TurnReceipt, TurnRequest, parse_runtime_event_v1,
};
pub use session::{
    CompactionId, CompactionPointer, CompactionRecentTurn, CompactionRecord, JournalRecord,
    ModelBinding, RecordId, RecoveryRecord, SessionRecord, SessionRecordV1, SessionStatus,
    TraceCode, TraceEntry, TurnRecord, TurnStatus, VisibleMessage, parse_session_record_v1,
};

/// Human-readable boundary used by architecture checks and documentation.
pub const CRATE_ROLE: &str = "stable serializable contracts with no product-layer dependencies";
