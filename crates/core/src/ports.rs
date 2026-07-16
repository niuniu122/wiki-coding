use std::future::Future;
use std::pin::Pin;

use minimax_protocol::{KnowledgePatch, ToolDecision, ToolInvocation, ToolResult, TransactionId};

use crate::knowledge::{WikiGenerationError, WikiGenerationOutput, WikiGenerationRequest};

pub type ApprovalFuture<'a> = Pin<Box<dyn Future<Output = ToolDecision> + Send + 'a>>;
pub type CancellationFuture<'a> = Pin<Box<dyn Future<Output = ()> + Send + 'a>>;
pub type ToolFuture<'a> = Pin<Box<dyn Future<Output = ToolResult> + Send + 'a>>;
pub type WikiGenerationFuture<'a> =
    Pin<Box<dyn Future<Output = Result<WikiGenerationOutput, WikiGenerationError>> + Send + 'a>>;
pub type KnowledgeCommitFuture<'a> =
    Pin<Box<dyn Future<Output = Result<TransactionId, KnowledgeCommitError>> + Send + 'a>>;

pub trait ApprovalPort: Send + Sync {
    fn decide<'a>(&'a self, invocation: &'a ToolInvocation) -> ApprovalFuture<'a>;
}

/// Run-scoped cancellation visible to effect adapters without coupling core to
/// a particular async runtime.
pub trait CancellationPort: Send + Sync {
    fn is_cancelled(&self) -> bool;
    fn cancelled<'a>(&'a self) -> CancellationFuture<'a>;
}

pub trait ToolPort: Send + Sync {
    fn preflight(
        &self,
        invocation: &ToolInvocation,
        cancellation: &dyn CancellationPort,
    ) -> Result<(), ToolResult>;
    fn execute<'a>(
        &'a self,
        invocation: &'a ToolInvocation,
        cancellation: &'a dyn CancellationPort,
    ) -> ToolFuture<'a>;
}

pub trait WikiGenerationPort: Send + Sync {
    fn generate<'a>(&'a self, request: &'a WikiGenerationRequest) -> WikiGenerationFuture<'a>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KnowledgeCommitError {
    Conflict,
    Unavailable,
    Invalid,
}

pub trait KnowledgePort: Send + Sync {
    fn commit<'a>(&'a self, patch: &'a KnowledgePatch) -> KnowledgeCommitFuture<'a>;
}

/// Supplies time to core workflows without consulting the system clock directly.
pub trait Clock {
    fn now_unix_ms(&self) -> u64;
}

/// Supplies stable identifiers without coupling core to an operating-system RNG.
pub trait IdGenerator {
    fn next_id(&self, prefix: &str) -> String;
}

/// Deterministic clock for fixtures and replay tests.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FixedClock {
    unix_ms: u64,
}

impl FixedClock {
    #[must_use]
    pub const fn new(unix_ms: u64) -> Self {
        Self { unix_ms }
    }
}

impl Clock for FixedClock {
    fn now_unix_ms(&self) -> u64 {
        self.unix_ms
    }
}

/// Deterministic ID generator for fixtures and replay tests.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FixedIdGenerator {
    suffix: String,
}

impl FixedIdGenerator {
    #[must_use]
    pub fn new(suffix: impl Into<String>) -> Self {
        Self {
            suffix: suffix.into(),
        }
    }
}

impl IdGenerator for FixedIdGenerator {
    fn next_id(&self, prefix: &str) -> String {
        format!("{prefix}_{}", self.suffix)
    }
}
