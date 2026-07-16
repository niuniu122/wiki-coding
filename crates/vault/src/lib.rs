//! Obsidian-compatible Wiki Vault storage for the Rust rewrite.
//!
//! This crate is the only writer of durable project knowledge. Model workflows
//! propose summaries through validated contracts; they do not write files.

mod bootstrap;
mod inbox;
mod page;
mod path;
mod raw;
pub mod runtime;
mod transaction;

pub use bootstrap::{ProjectVault, VaultError, VaultWarning, classify_vault_path};
pub use inbox::{complete_inbox_import, import_inbox_file};
pub use page::{normalize_wiki_slug, parse_wiki_page, render_wiki_page};
pub use path::content_hash as hash_vault_bytes;
pub use raw::{FinalizedSessionEvidence, finalize_runtime_session};
pub use runtime::{RuntimeStore, RuntimeStoreError};
pub use transaction::{
    PreparedWikiTransaction, TransactionFaultPoint, WikiChange, recover_wiki_transaction,
};

/// Human-readable boundary used by architecture checks and documentation.
pub const CRATE_ROLE: &str = "sole durable writer for the local Obsidian-compatible Wiki Vault";
