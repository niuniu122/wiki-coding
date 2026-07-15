//! Cross-implementation compatibility checks for the Rust rewrite.
//!
//! This non-published crate may observe all production crates in order to prove
//! behavioral parity with the existing TypeScript implementation.

pub mod manifest;
pub mod report;

pub use manifest::{
    BaselineStatus, CommandManifest, CompatManifests, ManifestError, ParityStatus,
    ProviderManifest, StatusItem, load_compat_manifests, repository_root,
};
pub use report::{CompatReport, ReportEntry, build_report, report_json, validate_report};

/// Human-readable boundary used by architecture checks and documentation.
pub const CRATE_ROLE: &str = "non-production parity harness across TypeScript and Rust boundaries";
