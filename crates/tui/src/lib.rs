//! Terminal presentation for the Rust rewrite.
//!
//! The TUI renders protocol state and sends user intent inward. It does not own
//! runtime policy, provider parsing, tool effects, retrieval, or Vault writes.

/// Human-readable boundary used by architecture checks and documentation.
pub const CRATE_ROLE: &str = "terminal rendering and input translation over protocol state";
