//! Provider adapters for the Rust rewrite.
//!
//! This crate translates provider-specific streams into protocol events. It
//! depends inward on core policy and protocol contracts; core never imports it.

/// Human-readable boundary used by architecture checks and documentation.
pub const CRATE_ROLE: &str = "provider-specific translation into stable protocol events";
