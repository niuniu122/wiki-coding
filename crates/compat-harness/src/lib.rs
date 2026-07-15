//! Cross-implementation compatibility checks for the Rust rewrite.
//!
//! This non-published crate may observe all production crates in order to prove
//! behavioral parity with the existing TypeScript implementation.

/// Human-readable boundary used by architecture checks and documentation.
pub const CRATE_ROLE: &str = "non-production parity harness across TypeScript and Rust boundaries";
