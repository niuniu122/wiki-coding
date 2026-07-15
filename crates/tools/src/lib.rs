//! Bounded tool adapters and effect boundaries for the Rust runtime.
//!
//! Permission modes never enter the policy API. Both `confirm` and
//! `full-access` therefore cross the same preflight before any effect.

mod error;
mod path;
mod policy;
mod read;
mod write;

pub use error::{ToolDenial, ToolDenialCode};
pub use path::{ResolvedToolPath, WorkspaceRoot};
pub use policy::{CancellationSignal, NeverCancelled, Preflight, ToolRegistry, ToolSpec};
pub use read::{ListDirectoryTool, ReadFileTool};
pub use write::{ApplyPatchTool, WriteFileTool};

/// Human-readable boundary used by architecture checks and documentation.
pub const CRATE_ROLE: &str = "permission-aware tool adapters and external effects";
