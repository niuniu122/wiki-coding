use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::manifest::{CompatManifests, ManifestError, ParityStatus};

const REPORT_SCHEMA_VERSION: u16 = 1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompatReport {
    pub schema_version: u16,
    pub baseline_commit: String,
    pub entries: Vec<ReportEntry>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReportEntry {
    pub id: String,
    pub status: ParityStatus,
    pub evidence: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub difference: Option<String>,
}

#[must_use]
pub fn build_report(manifests: &CompatManifests) -> CompatReport {
    let mut entries = Vec::new();
    for item in &manifests.baseline.items {
        let expanded_ids = match item.id.as_str() {
            "typescript.public_commands" | "rust.public_commands" => {
                let implementation = item.id.split('.').next().unwrap_or_default();
                manifests
                    .commands
                    .commands
                    .iter()
                    .flat_map(|command| {
                        std::iter::once(command.name.as_str())
                            .chain(command.aliases.iter().map(String::as_str))
                    })
                    .map(|command| format!("{implementation}.command.{command}"))
                    .collect::<Vec<_>>()
            }
            "typescript.provider_profiles" | "rust.provider_profiles" => {
                let implementation = item.id.split('.').next().unwrap_or_default();
                manifests
                    .providers
                    .profile_classes
                    .iter()
                    .map(|profile| format!("{implementation}.provider_profile.{}", profile.id))
                    .collect()
            }
            "typescript.provider_protocols" | "rust.provider_protocols" => {
                let implementation = item.id.split('.').next().unwrap_or_default();
                manifests
                    .providers
                    .protocols
                    .iter()
                    .map(|protocol| format!("{implementation}.provider_protocol.{protocol}"))
                    .collect()
            }
            _ => vec![item.id.clone()],
        };
        let mut evidence = item.evidence.clone();
        evidence.sort();
        entries.extend(expanded_ids.into_iter().map(|id| ReportEntry {
            id,
            status: item.status.clone(),
            evidence: evidence.clone(),
            difference: item.difference.clone(),
        }));
    }
    entries.sort_by(|left, right| left.id.cmp(&right.id));
    CompatReport {
        schema_version: REPORT_SCHEMA_VERSION,
        baseline_commit: manifests.baseline.baseline_commit.clone(),
        entries,
    }
}

pub fn validate_report(report: &CompatReport, root: &Path) -> Result<(), ManifestError> {
    if report.schema_version != REPORT_SCHEMA_VERSION {
        return Err(ManifestError::Validation(
            "compatibility report schema version must be 1".to_owned(),
        ));
    }
    if report.baseline_commit != "84784f5" {
        return Err(ManifestError::Validation(
            "compatibility report baseline commit must remain 84784f5".to_owned(),
        ));
    }

    let mut ids = BTreeSet::new();
    for entry in &report.entries {
        if !ids.insert(entry.id.as_str()) {
            return Err(ManifestError::Validation(format!(
                "duplicate compatibility report item: {}",
                entry.id
            )));
        }
        match entry.status {
            ParityStatus::Matched => {
                if entry.evidence.is_empty() {
                    return Err(ManifestError::Validation(format!(
                        "matched item requires evidence: {}",
                        entry.id
                    )));
                }
                for evidence in &entry.evidence {
                    if evidence.trim().is_empty() || !root.join(evidence).is_file() {
                        return Err(ManifestError::Validation(format!(
                            "matched item references missing evidence: {} -> {}",
                            entry.id, evidence
                        )));
                    }
                }
                if entry.difference.is_some() {
                    return Err(ManifestError::Validation(format!(
                        "matched item cannot carry a difference: {}",
                        entry.id
                    )));
                }
            }
            ParityStatus::Pending => {
                if !entry.evidence.is_empty() || entry.difference.is_some() {
                    return Err(ManifestError::Validation(format!(
                        "pending item cannot claim evidence or a difference: {}",
                        entry.id
                    )));
                }
            }
            ParityStatus::ApprovedDifference => {
                if entry
                    .difference
                    .as_deref()
                    .is_none_or(|difference| difference.trim().is_empty())
                {
                    return Err(ManifestError::Validation(format!(
                        "approved difference requires rationale: {}",
                        entry.id
                    )));
                }
            }
        }
    }
    Ok(())
}

pub fn report_json(report: &CompatReport) -> Result<String, ManifestError> {
    serde_json::to_string_pretty(report)
        .map(|json| format!("{json}\n"))
        .map_err(|_| ManifestError::Validation("cannot serialize compatibility report".to_owned()))
}
