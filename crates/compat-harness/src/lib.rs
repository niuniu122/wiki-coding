//! Public-contract compatibility checks for the Rust product.
//!
//! This non-published crate observes production crates and immutable fixtures;
//! it never needs a second executable product implementation.

pub mod architecture;
pub mod baseline;
pub mod coverage;
pub mod manifest;
pub mod migration_support;
pub mod provider_eval;
pub mod report;
pub mod retrieval_eval;
pub mod source_authority;

pub use architecture::{
    ArchitectureError, ArchitectureGraph, ArchitecturePackage, load_cargo_architecture,
    validate_architecture, validate_cli_tui_markdown_boundary, validate_core_source_boundary,
    validate_core_source_directory, validate_core_source_text, validate_migration_source_boundary,
    validate_migration_source_text, validate_retrieval_source_boundary,
    validate_retrieval_source_text, validate_ui_source_text, validate_vault_source_boundary,
    validate_vault_source_text,
};
pub use baseline::{
    BaselineError, compute_product_fingerprint, validate_cutover_candidate,
    validate_cutover_evidence, validate_cutover_strict_precondition,
    validate_hosted_candidate_gate, validate_hosted_candidate_gate_document,
    validate_hosted_release_gate, validate_hosted_release_gate_document, validate_product_entry,
    validate_rust_command_surface, validate_rust_provider_profiles,
    validate_rust_retrieval_evidence, validate_rust_shell_evidence, validate_rust_tool_evidence,
    validate_rust_vault_evidence,
};
pub use coverage::{
    CoverageCategory, CoverageDisposition, CoverageError, CoverageEvidence, CoverageMatrix,
    CoverageResponsibility, CoverageSource, load_coverage_matrix, validate_coverage_matrix,
};
pub use manifest::{
    CommandManifest, CompatManifests, ManifestError, ParityStatus, ProviderManifest,
    PublicContractManifest, StatusItem, load_compat_manifests, repository_root,
};
pub use migration_support::{
    MigrationSupportError, MigrationSupportWindowStatus, validate_migration_fixture_manifest,
    validate_migration_support_window,
};
pub use provider_eval::{
    PROVIDER_EVALUATION_GOLDEN, PROVIDER_EVALUATION_MANIFEST, ProviderCheckReport,
    ProviderEvaluationError, ProviderEvaluationReport, ProviderEvaluationTotals,
    ProviderProtocolReport, provider_evaluation_authorizes_release, provider_report_json,
    run_provider_evaluation, verify_provider_evaluation,
};
pub use report::{
    ApprovedDifference, CompatReport, ReportEntry, build_report, report_json,
    validate_compatibility_source_boundary, validate_report, verify_fixture_compatibility,
    verify_fixture_compatibility_strict_precondition,
};
pub use retrieval_eval::{
    CandidateBoundaryReport, CorpusReport, DegradationReport, DisabledPathReport, ProjectReport,
    RETRIEVAL_EVALUATION_GOLDEN, RETRIEVAL_EVALUATION_MANIFEST, RetrievalEvaluationError,
    RetrievalEvaluationReport, RetrievalMetrics, RetrievalThresholds, WorkspaceReport,
    retrieval_report_json, run_retrieval_evaluation, verify_retrieval_evaluation,
};
pub use source_authority::{
    SourceAuthorityError, SourceAuthorityManifest, load_source_authority,
    validate_javascript_source_text, validate_source_authority,
};

/// Human-readable boundary used by architecture checks and documentation.
pub const CRATE_ROLE: &str = "non-production public-contract and Rust compatibility harness";
