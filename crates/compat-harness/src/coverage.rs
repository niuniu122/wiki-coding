use std::collections::{BTreeMap, BTreeSet};
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
    pub evidence_contracts: Vec<CoverageEvidenceContract>,
    pub sources: Vec<CoverageSource>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CoverageEvidenceContract {
    pub id: String,
    pub evidence_class: CoverageEvidenceClass,
    pub category: CoverageCategory,
    pub claim: String,
    pub evidence: Vec<CoverageEvidence>,
    pub responsibility_ids: Vec<String>,
    #[serde(default)]
    pub retirement_review: Option<RetirementReview>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RetirementReview {
    pub sources: Vec<String>,
    pub status: RetirementStatus,
    pub outcome: String,
    pub reason: String,
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

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverageEvidenceClass {
    Behavior,
    Evaluation,
    FixtureContract,
    Migration,
    PackageSmoke,
    ParserRendering,
    ReviewedRetirement,
    StateMachine,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize)]
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetirementStatus {
    Dormant,
    Internal,
    Unshipped,
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
    if matrix.schema_version != 2 {
        return invalid("coverage matrix schemaVersion must be 2");
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
    let mut responsibilities = BTreeMap::new();
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
            if responsibilities
                .insert(
                    responsibility.id.as_str(),
                    (source.source_path.as_str(), responsibility),
                )
                .is_some()
            {
                return invalid(format!(
                    "duplicate responsibility ID: {}",
                    responsibility.id
                ));
            }
            validate_responsibility(root, responsibility, &allowed_javascript)?;
        }
    }
    validate_evidence_contracts(root, matrix, &responsibilities)
}

fn validate_evidence_contracts(
    root: &Path,
    matrix: &CoverageMatrix,
    responsibilities: &BTreeMap<&str, (&str, &CoverageResponsibility)>,
) -> Result<(), CoverageError> {
    if matrix.evidence_contracts.is_empty() {
        return invalid("coverage matrix has no evidenceContracts");
    }

    let mut contract_ids = BTreeSet::new();
    let mut assigned_responsibilities = BTreeSet::new();
    let mut exact_owners =
        BTreeMap::<String, (String, CoverageEvidenceClass, CoverageCategory)>::new();
    for contract in &matrix.evidence_contracts {
        if !valid_responsibility_id(&contract.id) || !contract_ids.insert(contract.id.as_str()) {
            return invalid(format!(
                "invalid or duplicate evidence contract ID: {}",
                contract.id
            ));
        }
        if contract.claim.trim().len() < 32
            || ["todo", "tbd", "placeholder", "generic"]
                .iter()
                .any(|marker| contract.claim.to_ascii_lowercase().contains(marker))
        {
            return invalid(format!(
                "evidence contract lacks a precise behavioral claim: {}",
                contract.id
            ));
        }
        if contract.evidence.is_empty() || contract.responsibility_ids.is_empty() {
            return invalid(format!(
                "evidence contract lacks an exact owner or responsibilities: {}",
                contract.id
            ));
        }

        let mut contract_sources = BTreeSet::new();
        for responsibility_id in &contract.responsibility_ids {
            if !assigned_responsibilities.insert(responsibility_id.as_str()) {
                return invalid(format!(
                    "responsibility belongs to more than one evidence contract: {responsibility_id}"
                ));
            }
            let Some((source_path, responsibility)) =
                responsibilities.get(responsibility_id.as_str())
            else {
                return invalid(format!(
                    "evidence contract names an unknown responsibility: {responsibility_id}"
                ));
            };
            contract_sources.insert(*source_path);
            if responsibility.category != contract.category {
                return invalid(format!(
                    "evidence contract category is incompatible with responsibility: {responsibility_id}"
                ));
            }
            if responsibility.evidence != contract.evidence {
                return invalid(format!(
                    "responsibility evidence differs from its exact contract owner: {responsibility_id}"
                ));
            }
            match responsibility.disposition {
                CoverageDisposition::Retired
                    if contract.evidence_class != CoverageEvidenceClass::ReviewedRetirement =>
                {
                    return invalid(format!(
                        "retired responsibility lacks reviewed-retirement evidence class: {responsibility_id}"
                    ));
                }
                CoverageDisposition::PackageSmoke
                    if contract.evidence_class != CoverageEvidenceClass::PackageSmoke =>
                {
                    return invalid(format!(
                        "package-smoke responsibility has an incompatible evidence class: {responsibility_id}"
                    ));
                }
                CoverageDisposition::RustCovered
                    if matches!(
                        contract.evidence_class,
                        CoverageEvidenceClass::PackageSmoke
                            | CoverageEvidenceClass::ReviewedRetirement
                    ) =>
                {
                    return invalid(format!(
                        "Rust-covered responsibility has an incompatible evidence class: {responsibility_id}"
                    ));
                }
                _ => {}
            }
            if responsibility.disposition == CoverageDisposition::Retired
                && retirement_is_forbidden(responsibility_id)
            {
                return invalid(format!(
                    "public or safety responsibility cannot be retired: {responsibility_id}"
                ));
            }
        }

        if contract.evidence_class == CoverageEvidenceClass::ReviewedRetirement {
            let review = contract.retirement_review.as_ref().ok_or_else(|| {
                CoverageError::Invalid(format!(
                    "reviewed retirement lacks retirementReview: {}",
                    contract.id
                ))
            })?;
            let review_sources = review
                .sources
                .iter()
                .map(String::as_str)
                .collect::<BTreeSet<_>>();
            if review_sources != contract_sources
                || review.outcome.trim().len() < 24
                || review.reason.trim().len() < 32
            {
                return invalid(format!(
                    "retirementReview is not exact and source-complete: {}",
                    contract.id
                ));
            }
        } else if contract.retirement_review.is_some() {
            return invalid(format!(
                "non-retirement contract cannot contain retirementReview: {}",
                contract.id
            ));
        }

        if contract.evidence_class != CoverageEvidenceClass::ReviewedRetirement {
            for evidence in &contract.evidence {
                let owner = format!(
                    "{}#{}",
                    evidence.path,
                    evidence.test.as_deref().unwrap_or("")
                );
                let semantic_identity = (
                    contract.id.clone(),
                    contract.evidence_class,
                    contract.category,
                );
                if let Some(previous) =
                    exact_owners.insert(owner.clone(), semantic_identity.clone())
                    && previous != semantic_identity
                {
                    return invalid(format!(
                        "exact evidence owner is reused across incompatible semantic contracts: {owner}"
                    ));
                }
            }
        }
    }

    let expected = responsibilities.keys().copied().collect::<BTreeSet<_>>();
    if assigned_responsibilities != expected {
        let missing = expected
            .difference(&assigned_responsibilities)
            .next()
            .copied()
            .unwrap_or("unknown");
        return invalid(format!(
            "responsibility is missing an evidence contract assignment: {missing}"
        ));
    }

    for contract in &matrix.evidence_contracts {
        for evidence in &contract.evidence {
            validate_evidence_owner(root, evidence, contract.id.as_str())?;
        }
    }
    Ok(())
}

fn retirement_is_forbidden(id: &str) -> bool {
    matches!(
        id,
        "ts-test-agent-budget-test-ts"
            | "ts-test-agent-item-storage-test-ts"
            | "ts-test-agent-route-cutover-test-ts"
            | "ts-test-agent-run-engine-test-ts"
            | "ts-test-agent-run-recovery-test-ts"
            | "ts-test-application-kernel-test-ts"
            | "ts-test-credential-consent-test-ts"
            | "ts-test-feature-flags-test-ts"
            | "ts-test-model-profile-registry-test-ts"
            | "ts-test-model-profile-test-ts"
            | "ts-test-model-selection-persistence-test-ts"
            | "ts-test-model-selection-service-test-ts"
            | "ts-test-model-state-store-test-ts"
            | "ts-test-secret-store-test-ts"
            | "ts-test-summary-generator-test-ts"
            | "ts-test-user-profile-store-test-ts"
            | "ts-command-retry-continue-outcomes"
    )
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
            validate_evidence_owner(root, evidence, &responsibility.id)?;
        } else if evidence.test.is_some() {
            return invalid(format!(
                "package orchestration evidence cannot claim a Rust test name: {}",
                responsibility.id
            ));
        }
    }
    Ok(())
}

fn validate_evidence_owner(
    root: &Path,
    evidence: &CoverageEvidence,
    context: &str,
) -> Result<(), CoverageError> {
    let extension = Path::new(&evidence.path)
        .extension()
        .and_then(|value| value.to_str());
    if extension != Some("rs") {
        return Ok(());
    }
    let test = evidence
        .test
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            CoverageError::Invalid(format!("Rust evidence lacks an exact test name: {context}"))
        })?;
    let source = fs::read_to_string(root.join(&evidence.path)).map_err(|_| {
        CoverageError::Invalid(format!("cannot read coverage evidence: {}", evidence.path))
    })?;
    if !source.contains(&format!("fn {test}")) {
        return invalid(format!(
            "Rust evidence test name was not found in {}: {test}",
            evidence.path
        ));
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
        let evidence_classes = [
            CoverageEvidenceClass::Behavior,
            CoverageEvidenceClass::Evaluation,
            CoverageEvidenceClass::FixtureContract,
            CoverageEvidenceClass::Migration,
            CoverageEvidenceClass::PackageSmoke,
            CoverageEvidenceClass::ParserRendering,
            CoverageEvidenceClass::ReviewedRetirement,
            CoverageEvidenceClass::StateMachine,
        ];
        assert_eq!(evidence_classes.len(), 8);
        assert_ne!(
            CoverageDisposition::RustCovered,
            CoverageDisposition::Retired
        );
    }
}
