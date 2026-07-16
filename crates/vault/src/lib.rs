//! Obsidian-compatible Wiki Vault storage for the Rust rewrite.
//!
//! This crate is the only writer of durable project knowledge. Model workflows
//! propose summaries through validated contracts; they do not write files.

mod bootstrap;
mod forget;
mod gc;
mod inbox;
mod lint;
mod page;
mod path;
mod raw;
mod rebuild;
pub mod runtime;
mod transaction;
mod workflow;

pub use bootstrap::{ProjectVault, VaultError, VaultWarning, classify_vault_path};
pub use forget::{apply_forget_plan, forget_confirmation, plan_forget};
pub use gc::{
    apply_gc_plan, gc_apply_confirmation, gc_purge_confirmation, gc_report, purge_gc_plan,
    read_gc_trash_manifest, undo_gc_plan,
};
pub use inbox::{complete_inbox_import, import_inbox_file};
pub use lint::{lint_vault, repair_vault};
pub use page::{normalize_wiki_slug, parse_wiki_page, render_wiki_page};
pub use path::content_hash as hash_vault_bytes;
pub use raw::{FinalizedSessionEvidence, finalize_runtime_session};
pub use rebuild::rebuild_compiled_wiki;
pub use runtime::{RuntimeStore, RuntimeStoreError};
pub use transaction::{
    PreparedWikiTransaction, TransactionFaultPoint, WikiChange, recover_wiki_transaction,
    wiki_transaction_exists,
};
pub use workflow::{
    KnowledgeWorkflowHistory, KnowledgeWorkflowStore, StoredGeneration, ensure_knowledge_job,
    find_evaluation_missing, knowledge_job_for_session, synthesized_knowledge_patches,
};

/// Human-readable boundary used by architecture checks and documentation.
pub const CRATE_ROLE: &str = "sole durable writer for the local Obsidian-compatible Wiki Vault";
