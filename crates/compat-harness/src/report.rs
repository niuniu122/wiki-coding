use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::architecture::{
    load_cargo_architecture, validate_architecture, validate_cli_tui_markdown_boundary,
    validate_core_source_boundary, validate_migration_source_boundary,
    validate_retrieval_source_boundary, validate_vault_source_boundary,
};
use crate::baseline::{
    validate_cutover_candidate, validate_cutover_evidence, validate_product_entry,
    validate_rust_command_surface, validate_rust_provider_profiles,
    validate_rust_retrieval_evidence, validate_rust_tool_evidence, validate_rust_vault_evidence,
};
use crate::manifest::{CompatManifests, ManifestError, ParityStatus};
use crate::provider_eval::verify_provider_evaluation;
use crate::retrieval_eval::verify_retrieval_evaluation;

const REPORT_SCHEMA_VERSION: u16 = 1;
const DIFFERENCE_FIXTURE: &str = "fixtures/compat/command-differences.v1.json";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompatReport {
    pub schema_version: u16,
    pub contract_version: String,
    pub contract_fingerprint: String,
    pub entries: Vec<ReportEntry>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReportEntry {
    pub id: String,
    pub rust_status: ParityStatus,
    pub rust_evidence: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approved_difference: Option<ApprovedDifference>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApprovedDifference {
    pub id: String,
    pub command: String,
    pub locked_outcome: String,
    pub rust_behavior: String,
    pub reason: String,
    pub safety: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CommandDifferenceFixture {
    schema_version: u16,
    differences: Vec<ApprovedDifference>,
}

pub fn build_report(
    manifests: &CompatManifests,
    root: &Path,
) -> Result<CompatReport, ManifestError> {
    let differences = load_approved_differences(root, manifests)?;
    let mut entries = Vec::with_capacity(manifests.public_contract.items.len());
    for item in &manifests.public_contract.items {
        let mut rust_evidence = item.evidence.clone();
        rust_evidence.sort();
        let approved_difference = item
            .approved_difference
            .as_ref()
            .map(|id| {
                differences.get(id).cloned().ok_or_else(|| {
                    ManifestError::Validation(format!(
                        "public contract references unknown approved difference: {id}"
                    ))
                })
            })
            .transpose()?;
        entries.push(ReportEntry {
            id: item.id.clone(),
            rust_status: item.status.clone(),
            rust_evidence,
            approved_difference,
        });
    }
    entries.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(CompatReport {
        schema_version: REPORT_SCHEMA_VERSION,
        contract_version: manifests.public_contract.contract_version.clone(),
        contract_fingerprint: manifests.public_contract.content_fingerprint.clone(),
        entries,
    })
}

pub fn validate_report(
    report: &CompatReport,
    manifests: &CompatManifests,
    root: &Path,
) -> Result<(), ManifestError> {
    if report.schema_version != REPORT_SCHEMA_VERSION {
        return Err(ManifestError::Validation(
            "compatibility report schema version must be 1".to_owned(),
        ));
    }
    if report.contract_version != manifests.public_contract.contract_version {
        return Err(ManifestError::Validation(
            "compatibility report contract version mismatch".to_owned(),
        ));
    }
    if report.contract_fingerprint != manifests.public_contract.content_fingerprint {
        return Err(ManifestError::Validation(
            "compatibility report contract fingerprint mismatch".to_owned(),
        ));
    }

    let contract_items = manifests
        .public_contract
        .items
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect::<BTreeMap<_, _>>();
    let mut ids = BTreeSet::new();
    for entry in &report.entries {
        if !ids.insert(entry.id.as_str()) {
            return Err(ManifestError::Validation(format!(
                "duplicate compatibility report item: {}",
                entry.id
            )));
        }
        let item = contract_items.get(entry.id.as_str()).ok_or_else(|| {
            ManifestError::Validation(format!(
                "compatibility report contains a non-contract item: {}",
                entry.id
            ))
        })?;
        if entry.rust_status != item.status {
            return Err(ManifestError::Validation(format!(
                "compatibility report Rust status drift: {}",
                entry.id
            )));
        }
        if entry.rust_evidence.is_empty() {
            return Err(ManifestError::Validation(format!(
                "matched item requires evidence: {}",
                entry.id
            )));
        }
        for evidence in &entry.rust_evidence {
            if evidence.trim().is_empty() || !root.join(evidence).is_file() {
                return Err(ManifestError::Validation(format!(
                    "matched item references missing evidence: {} -> {}",
                    entry.id, evidence
                )));
            }
        }
        match entry.rust_status {
            ParityStatus::Matched if entry.approved_difference.is_some() => {
                return Err(ManifestError::Validation(format!(
                    "matched item cannot carry an approved difference: {}",
                    entry.id
                )));
            }
            ParityStatus::ApprovedDifference if entry.approved_difference.is_none() => {
                return Err(ManifestError::Validation(format!(
                    "approved difference is missing from report: {}",
                    entry.id
                )));
            }
            ParityStatus::Pending => {
                return Err(ManifestError::Validation(format!(
                    "public contract report cannot contain a pending item: {}",
                    entry.id
                )));
            }
            ParityStatus::Matched | ParityStatus::ApprovedDifference => {}
        }
    }
    if ids.len() != contract_items.len() {
        return Err(ManifestError::Validation(
            "compatibility report must contain every public contract item exactly once".to_owned(),
        ));
    }

    let expected = build_report(manifests, root)?;
    if report != &expected {
        return Err(ManifestError::Validation(
            "compatibility report differs from immutable contract and approved differences"
                .to_owned(),
        ));
    }
    Ok(())
}

pub fn report_json(report: &CompatReport) -> Result<String, ManifestError> {
    serde_json::to_string_pretty(report)
        .map(|json| format!("{json}\n"))
        .map_err(|_| ManifestError::Validation("cannot serialize compatibility report".to_owned()))
}

pub fn validate_compatibility_source_boundary(root: &Path) -> Result<(), ManifestError> {
    let compatibility_sources = [
        "crates/compat-harness/src/manifest.rs",
        "crates/compat-harness/src/report.rs",
        "crates/compat-harness/src/baseline.rs",
        "crates/compat-harness/src/main.rs",
    ];
    let process_patterns = [
        concat!("Command::new(\"", "node", "\")"),
        concat!("Command::new(\"", "npm", "\")"),
        concat!("Command::new(\"", "npx", "\")"),
        concat!("Command::new(\"", "tsc", "\")"),
    ];
    let source_patterns = [
        concat!("src/cli.", "tsx"),
        concat!("dist/cli.", "js"),
        concat!("npm run ", "build"),
        concat!("tsc ", "-p"),
        concat!("import '../", "src/"),
        concat!("import \"../", "src/"),
    ];
    for relative in compatibility_sources {
        let source = fs::read_to_string(root.join(relative)).map_err(|_| ManifestError::Read {
            file: relative.to_owned(),
        })?;
        if process_patterns
            .iter()
            .any(|pattern| source.contains(pattern))
            || (relative != "crates/compat-harness/src/baseline.rs"
                && source_patterns
                    .iter()
                    .any(|pattern| source.contains(pattern)))
        {
            return Err(ManifestError::Validation(format!(
                "compatibility source depends on the transitional TypeScript runtime: {relative}"
            )));
        }
    }
    Ok(())
}

pub fn verify_fixture_compatibility(
    root: &Path,
    require_hosted_evidence: bool,
) -> Result<(), String> {
    validate_compatibility_source_boundary(root).map_err(|error| error.to_string())?;
    verify_provider_evaluation(root).map_err(|error| error.to_string())?;
    verify_retrieval_evaluation(root).map_err(|error| error.to_string())?;

    let manifests =
        crate::manifest::load_compat_manifests(root).map_err(|error| error.to_string())?;
    validate_rust_command_surface(&manifests.commands).map_err(|error| error.to_string())?;
    validate_rust_tool_evidence(root, &manifests.public_contract)
        .map_err(|error| error.to_string())?;
    validate_rust_vault_evidence(root).map_err(|error| error.to_string())?;
    validate_rust_retrieval_evidence(root).map_err(|error| error.to_string())?;
    validate_rust_provider_profiles(&manifests.providers).map_err(|error| error.to_string())?;
    validate_product_entry(root).map_err(|error| error.to_string())?;
    if require_hosted_evidence {
        validate_cutover_evidence(root, &manifests.public_contract)
            .map_err(|error| error.to_string())?;
    } else {
        validate_cutover_candidate(root, &manifests.public_contract)
            .map_err(|error| error.to_string())?;
    }

    let architecture = load_cargo_architecture(root).map_err(|error| error.to_string())?;
    validate_architecture(&architecture).map_err(|error| error.to_string())?;
    validate_core_source_boundary(root).map_err(|error| error.to_string())?;
    validate_vault_source_boundary(root).map_err(|error| error.to_string())?;
    validate_cli_tui_markdown_boundary(root).map_err(|error| error.to_string())?;
    validate_retrieval_source_boundary(root).map_err(|error| error.to_string())?;
    validate_migration_source_boundary(root).map_err(|error| error.to_string())?;

    let first = build_report(&manifests, root).map_err(|error| error.to_string())?;
    validate_report(&first, &manifests, root).map_err(|error| error.to_string())?;
    let first_json = report_json(&first).map_err(|error| error.to_string())?;
    let second_manifests =
        crate::manifest::load_compat_manifests(root).map_err(|error| error.to_string())?;
    let second = build_report(&second_manifests, root).map_err(|error| error.to_string())?;
    validate_report(&second, &second_manifests, root).map_err(|error| error.to_string())?;
    let second_json = report_json(&second).map_err(|error| error.to_string())?;
    if first_json != second_json {
        return Err("compatibility report is not deterministic".to_owned());
    }
    Ok(())
}

fn load_approved_differences(
    root: &Path,
    manifests: &CompatManifests,
) -> Result<BTreeMap<String, ApprovedDifference>, ManifestError> {
    let raw =
        fs::read_to_string(root.join(DIFFERENCE_FIXTURE)).map_err(|_| ManifestError::Read {
            file: DIFFERENCE_FIXTURE.to_owned(),
        })?;
    let fixture: CommandDifferenceFixture =
        serde_json::from_str(&raw).map_err(|_| ManifestError::Parse {
            file: DIFFERENCE_FIXTURE.to_owned(),
        })?;
    if fixture.schema_version != 1 {
        return Err(ManifestError::Validation(
            "unsupported approved-difference schema version".to_owned(),
        ));
    }

    let expected_links = manifests
        .public_contract
        .items
        .iter()
        .filter_map(|item| item.approved_difference.clone())
        .collect::<BTreeSet<_>>();
    let mut by_id = BTreeMap::new();
    let mut commands = BTreeSet::new();
    for difference in fixture.differences {
        if !difference.id.starts_with("difference.command.")
            || !commands.insert(difference.command.clone())
            || difference.locked_outcome.trim().is_empty()
            || difference.rust_behavior.len() < 24
            || difference.reason.len() < 24
            || difference.safety.len() < 24
            || by_id.insert(difference.id.clone(), difference).is_some()
        {
            return Err(ManifestError::Validation(
                "approved command differences must be unique, complete, and stable".to_owned(),
            ));
        }
    }
    if by_id.keys().cloned().collect::<BTreeSet<_>>() != expected_links {
        return Err(ManifestError::Validation(
            "approved command differences must exactly match public-contract links".to_owned(),
        ));
    }
    for (id, difference) in &by_id {
        let contract_id = format!("contract.command.{}", difference.command);
        let item = manifests
            .public_contract
            .items
            .iter()
            .find(|item| item.id == contract_id)
            .ok_or_else(|| {
                ManifestError::Validation(format!(
                    "approved difference has no public command: {id}"
                ))
            })?;
        if item.status != ParityStatus::ApprovedDifference
            || item.approved_difference.as_deref() != Some(id.as_str())
        {
            return Err(ManifestError::Validation(format!(
                "approved difference is not linked by its public command: {id}"
            )));
        }
        let command = manifests
            .commands
            .commands
            .iter()
            .find(|command| command.name == difference.command)
            .ok_or_else(|| {
                ManifestError::Validation(format!(
                    "approved difference command is not canonical: {id}"
                ))
            })?;
        if command.outcome != difference.locked_outcome {
            return Err(ManifestError::Validation(format!(
                "approved difference outcome drift: {id}"
            )));
        }
    }
    Ok(by_id)
}
