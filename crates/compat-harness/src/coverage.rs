use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::path::{Component, Path};

use serde::Deserialize;

use crate::source_authority::SourceAuthorityManifest;

pub const COVERAGE_MATRIX_PATH: &str =
    "fixtures/compat/verification/typescript-responsibilities.v1.json";
pub const COVERAGE_SOURCE_AUTHORITY: &str = "fixtures/compat/source-authority.v1.json";

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CoverageMatrix {
    pub schema_version: u16,
    pub source_authority: String,
    pub sources: Vec<CoverageSource>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CoverageSource {
    pub source_path: String,
    pub source_sha256: String,
    pub responsibilities: Vec<CoverageResponsibility>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CoverageResponsibility {
    pub id: String,
    pub category: CoverageCategory,
    pub contract_refs: Vec<String>,
    pub disposition: CoverageDisposition,
    pub evidence: Vec<CoverageEvidence>,
    pub rationale: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CoverageEvidence {
    pub path: String,
    pub test: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverageCategory {
    Cli,
    Credential,
    Diagnostic,
    Lifecycle,
    Migration,
    Package,
    Provider,
    Rendering,
    Retrieval,
    Runtime,
    Storage,
    TestHarness,
    Tool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverageDisposition {
    RustCovered,
    PackageSmoke,
    Retired,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CoverageError {
    MatrixRead,
    MatrixParse(String),
    Invalid(String),
}

impl fmt::Display for CoverageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MatrixRead => formatter.write_str("cannot read TypeScript responsibility matrix"),
            Self::MatrixParse(message) => {
                write!(
                    formatter,
                    "invalid TypeScript responsibility matrix JSON: {message}"
                )
            }
            Self::Invalid(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for CoverageError {}

pub fn load_coverage_matrix(root: &Path) -> Result<CoverageMatrix, CoverageError> {
    let contents = fs::read_to_string(root.join(COVERAGE_MATRIX_PATH))
        .map_err(|_| CoverageError::MatrixRead)?;
    serde_json::from_str(&contents).map_err(|error| CoverageError::MatrixParse(error.to_string()))
}

pub fn validate_coverage_matrix(
    root: &Path,
    matrix: &CoverageMatrix,
    authority: &SourceAuthorityManifest,
) -> Result<(), CoverageError> {
    if matrix.schema_version != 1 {
        return invalid("coverage matrix schemaVersion must be 1");
    }
    if matrix.source_authority != COVERAGE_SOURCE_AUTHORITY {
        return invalid("coverage matrix must name the Phase 10 source authority manifest");
    }

    let expected = verification_sources(authority);
    let actual = matrix
        .sources
        .iter()
        .map(|source| (source.source_path.clone(), source.source_sha256.clone()))
        .collect::<BTreeSet<_>>();
    if actual != expected {
        let expected_paths = expected
            .iter()
            .map(|(path, _)| path.as_str())
            .collect::<BTreeSet<_>>();
        let actual_paths = actual
            .iter()
            .map(|(path, _)| path.as_str())
            .collect::<BTreeSet<_>>();
        if let Some(path) = expected_paths.difference(&actual_paths).next() {
            return invalid(format!("coverage matrix is missing source: {path}"));
        }
        if let Some(path) = actual_paths.difference(&expected_paths).next() {
            return invalid(format!("coverage matrix contains unknown source: {path}"));
        }
        return invalid("coverage matrix source hash does not match Phase 10 authority");
    }
    if matrix.sources.len() != expected.len() {
        return invalid("coverage matrix source paths must be unique");
    }

    let allowed_javascript = authority
        .javascript_allowlist
        .iter()
        .map(|entry| entry.path.as_str())
        .collect::<BTreeSet<_>>();
    let mut responsibility_ids = BTreeSet::new();
    let mut previous_source = None;
    for source in &matrix.sources {
        validate_relative_path(&source.source_path)?;
        if previous_source.is_some_and(|previous| previous >= source.source_path.as_str()) {
            return invalid("coverage matrix sources must be sorted and duplicate-free");
        }
        previous_source = Some(source.source_path.as_str());
        if source.responsibilities.is_empty() {
            return invalid(format!(
                "coverage source has no responsibilities: {}",
                source.source_path
            ));
        }
        for responsibility in &source.responsibilities {
            if !valid_responsibility_id(&responsibility.id) {
                return invalid(format!("invalid responsibility ID: {}", responsibility.id));
            }
            if !responsibility_ids.insert(responsibility.id.as_str()) {
                return invalid(format!(
                    "duplicate responsibility ID: {}",
                    responsibility.id
                ));
            }
            validate_responsibility(root, responsibility, &allowed_javascript)?;
        }
    }
    Ok(())
}

fn validate_responsibility(
    root: &Path,
    responsibility: &CoverageResponsibility,
    allowed_javascript: &BTreeSet<&str>,
) -> Result<(), CoverageError> {
    let rationale = responsibility.rationale.trim();
    if rationale.len() < 24
        || ["todo", "tbd", "placeholder", "requires_port"]
            .iter()
            .any(|marker| rationale.to_ascii_lowercase().contains(marker))
    {
        return invalid(format!(
            "responsibility lacks a concrete final rationale: {}",
            responsibility.id
        ));
    }
    if responsibility.evidence.is_empty() {
        return invalid(format!(
            "responsibility has no replacement evidence: {}",
            responsibility.id
        ));
    }

    match responsibility.disposition {
        CoverageDisposition::Retired => {
            if !responsibility.contract_refs.is_empty() {
                return invalid(format!(
                    "retired responsibility cites a locked public contract: {}",
                    responsibility.id
                ));
            }
            let lowercase = rationale.to_ascii_lowercase();
            if !["dormant", "internal", "unshipped"]
                .iter()
                .any(|marker| lowercase.contains(marker))
            {
                return invalid(format!(
                    "retired responsibility lacks a dormant/internal/unshipped rationale: {}",
                    responsibility.id
                ));
            }
        }
        CoverageDisposition::RustCovered | CoverageDisposition::PackageSmoke => {
            if responsibility.contract_refs.is_empty() {
                return invalid(format!(
                    "covered responsibility lacks a public contract reference: {}",
                    responsibility.id
                ));
            }
        }
    }

    for contract_ref in &responsibility.contract_refs {
        let Some((path, anchor)) = contract_ref.split_once('#') else {
            return invalid(format!(
                "contract reference must include a path and anchor: {}",
                responsibility.id
            ));
        };
        if anchor.is_empty() {
            return invalid(format!(
                "contract reference anchor is empty: {}",
                responsibility.id
            ));
        }
        require_regular_file(root, path, "contract reference")?;
    }

    for evidence in &responsibility.evidence {
        require_regular_file(root, &evidence.path, "coverage evidence")?;
        let extension = Path::new(&evidence.path)
            .extension()
            .and_then(|value| value.to_str());
        let is_rust = extension == Some("rs");
        let is_allowed_javascript = allowed_javascript.contains(evidence.path.as_str());
        if !is_rust && !is_allowed_javascript {
            return invalid(format!(
                "coverage evidence is not Rust or allowed package orchestration: {}",
                evidence.path
            ));
        }
        match responsibility.disposition {
            CoverageDisposition::RustCovered | CoverageDisposition::Retired if !is_rust => {
                return invalid(format!(
                    "Rust/retirement evidence must be Rust-owned: {}",
                    responsibility.id
                ));
            }
            CoverageDisposition::PackageSmoke if !is_allowed_javascript => {
                return invalid(format!(
                    "package-smoke evidence must be reviewed JavaScript orchestration: {}",
                    responsibility.id
                ));
            }
            _ => {}
        }
        if is_rust {
            let test = evidence
                .test
                .as_deref()
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    CoverageError::Invalid(format!(
                        "Rust evidence lacks an exact test name: {}",
                        responsibility.id
                    ))
                })?;
            if !valid_test_name(test) {
                return invalid(format!("invalid Rust evidence test name: {test}"));
            }
            let source = fs::read_to_string(root.join(&evidence.path)).map_err(|_| {
                CoverageError::Invalid(format!("cannot read coverage evidence: {}", evidence.path))
            })?;
            if !source.contains(&format!("fn {test}")) {
                return invalid(format!(
                    "Rust evidence test name was not found in {}: {test}",
                    evidence.path
                ));
            }
        } else if evidence.test.is_some() {
            return invalid(format!(
                "package orchestration evidence cannot claim a Rust test name: {}",
                responsibility.id
            ));
        }
    }
    Ok(())
}

fn verification_sources(authority: &SourceAuthorityManifest) -> BTreeSet<(String, String)> {
    let mut sources = authority
        .transitional_type_script
        .entries
        .iter()
        .filter(|entry| {
            entry.path.starts_with("test/")
                || entry.path.starts_with("src/eval/")
                || entry.path.starts_with("src/smoke/")
        })
        .map(|entry| (entry.path.clone(), entry.sha256.clone()))
        .collect::<BTreeSet<_>>();
    sources.extend(
        authority
            .transitional_legacy_test_fixtures
            .entries
            .iter()
            .map(|entry| (entry.path.clone(), entry.sha256.clone())),
    );
    sources
}

fn valid_responsibility_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 120
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn valid_test_name(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn validate_relative_path(path: &str) -> Result<(), CoverageError> {
    let parsed = Path::new(path);
    if path.is_empty()
        || path.contains('\\')
        || path.ends_with('/')
        || parsed.is_absolute()
        || parsed.components().any(|component| {
            matches!(
                component,
                Component::Prefix(_)
                    | Component::RootDir
                    | Component::ParentDir
                    | Component::CurDir
            )
        })
    {
        return invalid(format!("unsafe coverage path: {path}"));
    }
    Ok(())
}

fn require_regular_file(root: &Path, path: &str, class: &str) -> Result<(), CoverageError> {
    validate_relative_path(path)?;
    let metadata = fs::symlink_metadata(root.join(path))
        .map_err(|_| CoverageError::Invalid(format!("missing {class}: {path}")))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return invalid(format!("{class} must be a regular file: {path}"));
    }
    Ok(())
}

fn invalid<T>(message: impl Into<String>) -> Result<T, CoverageError> {
    Err(CoverageError::Invalid(message.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_and_disposition_are_closed_enums() {
        let categories = [
            CoverageCategory::Cli,
            CoverageCategory::Credential,
            CoverageCategory::Diagnostic,
            CoverageCategory::Lifecycle,
            CoverageCategory::Migration,
            CoverageCategory::Package,
            CoverageCategory::Provider,
            CoverageCategory::Rendering,
            CoverageCategory::Retrieval,
            CoverageCategory::Runtime,
            CoverageCategory::Storage,
            CoverageCategory::TestHarness,
            CoverageCategory::Tool,
        ];
        assert_eq!(categories.len(), 13);
        assert_ne!(
            CoverageDisposition::RustCovered,
            CoverageDisposition::Retired
        );
    }
}
