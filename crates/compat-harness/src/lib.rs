//! Cross-implementation compatibility checks for the Rust rewrite.
//!
//! This non-published crate may observe all production crates in order to prove
//! behavioral parity with the existing TypeScript implementation.

pub mod architecture;
pub mod baseline;
pub mod manifest;
pub mod report;

pub use architecture::{
    ArchitectureError, ArchitectureGraph, ArchitecturePackage, load_cargo_architecture,
    validate_architecture, validate_cli_tui_markdown_boundary, validate_core_source_boundary,
    validate_core_source_directory, validate_core_source_text, validate_retrieval_source_boundary,
    validate_retrieval_source_text, validate_ui_source_text, validate_vault_source_boundary,
    validate_vault_source_text,
};
pub use baseline::{
    BaselineError, validate_product_entry, validate_rust_command_surface,
    validate_rust_retrieval_evidence, validate_rust_tool_evidence, validate_rust_vault_evidence,
};
pub use manifest::{
    BaselineStatus, CommandManifest, CompatManifests, ManifestError, ParityStatus,
    ProviderManifest, StatusItem, load_compat_manifests, repository_root,
};
pub use report::{CompatReport, ReportEntry, build_report, report_json, validate_report};

/// Human-readable boundary used by architecture checks and documentation.
pub const CRATE_ROLE: &str = "non-production parity harness across TypeScript and Rust boundaries";
