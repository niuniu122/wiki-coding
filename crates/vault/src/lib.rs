//! Obsidian-compatible Wiki Vault storage for the Rust rewrite.
//!
//! This crate is the only writer of durable project knowledge. Model workflows
//! propose summaries through validated contracts; they do not write files.

mod bootstrap;
mod path;
mod raw;
pub mod runtime;

pub use bootstrap::{ProjectVault, VaultError, VaultWarning, classify_vault_path};
pub use raw::{FinalizedSessionEvidence, finalize_runtime_session};
pub use runtime::{RuntimeStore, RuntimeStoreError};

/// Human-readable boundary used by architecture checks and documentation.
pub const CRATE_ROLE: &str = "sole durable writer for the local Obsidian-compatible Wiki Vault";
