//! Bounded tool adapters and effect boundaries for the Rust runtime.
//!
//! Permission modes never enter the policy API. Both `confirm` and
//! `full-access` therefore cross the same preflight before any effect.

mod adapter;
mod error;
mod git;
mod npm;
mod path;
mod policy;
mod process;
mod read;
mod write;

pub use error::{ToolDenial, ToolDenialCode};
pub use git::{GitDiffTool, GitStatusTool};
pub use minimax_core::CancellationPort as CancellationSignal;
pub use npm::NpmDiagnosticTool;
pub use path::{ResolvedToolPath, WorkspaceRoot};
pub use policy::{NeverCancelled, Preflight, ToolRegistry, ToolSpec};
pub use process::{
    BoundedProcess, ChildEvent, ChildEventFuture, ChildStopFuture, DirectChild, ProcessCompletion,
    ProcessLauncher, ProcessLimits, ProcessRequest, RunDiagnosticTool, TokioProcessLauncher,
};
pub use read::{ListDirectoryTool, ReadFileTool};
pub use write::{ApplyPatchTool, WriteFileTool};

/// Human-readable boundary used by architecture checks and documentation.
pub const CRATE_ROLE: &str = "permission-aware tool adapters and external effects";
pub use adapter::BuiltinToolPort;
