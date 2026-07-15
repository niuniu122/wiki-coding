//! Runtime orchestration and policy for the Rust rewrite.
//!
//! Core coordinates state transitions through protocol contracts. Concrete
//! providers, tools, retrieval engines, Vault storage, and UI code live outside
//! this crate so policy remains testable without side effects.

/// Human-readable boundary used by architecture checks and documentation.
pub const CRATE_ROLE: &str = "runtime orchestration and policy without concrete adapters";
