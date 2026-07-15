//! Stable data contracts shared across the Rust rewrite.
//!
//! This lowest-level crate owns serializable messages and events. It must not
//! depend on orchestration, providers, tools, retrieval, storage, or the UI.

mod event;

pub use event::{
    ProtocolErrorCode, ProviderProtocolKind, SCHEMA_VERSION, SchemaVersion, SessionId, StreamEvent,
    StreamEventV1, TerminalOutcome, ToolCallFragment, ToolCallId, TurnId, Usage,
    parse_stream_event_v1,
};

/// Human-readable boundary used by architecture checks and documentation.
pub const CRATE_ROLE: &str = "stable serializable contracts with no product-layer dependencies";
