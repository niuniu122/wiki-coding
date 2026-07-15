//! Tool adapters and effect boundaries for the Rust rewrite.
//!
//! Permission enforcement and filesystem or process effects belong here. The
//! public permission vocabulary remains `confirm` and `full-access`.

/// Human-readable boundary used by architecture checks and documentation.
pub const CRATE_ROLE: &str = "permission-aware tool adapters and external effects";
