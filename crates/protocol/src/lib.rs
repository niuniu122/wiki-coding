//! Stable data contracts shared across the Rust rewrite.
//!
//! This lowest-level crate owns serializable messages and events. It must not
//! depend on orchestration, providers, tools, retrieval, storage, or the UI.

/// Human-readable boundary used by architecture checks and documentation.
pub const CRATE_ROLE: &str = "stable serializable contracts with no product-layer dependencies";
