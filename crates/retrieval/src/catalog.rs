use std::collections::BTreeSet;

use minimax_protocol::SchemaVersion;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::ProjectDocument;

const MAX_PROJECTS: usize = 10_000;
const MAX_TEXT_BYTES: usize = 8 * 1024;

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MaintenanceSignals {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recent_commits: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_issue_triage: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectRelease {
    pub version: String,
    pub published_at: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectCatalogEntry {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub description: String,
    #[serde(default)]
    pub topics: Vec<String>,
    #[serde(default)]
    pub platforms: Vec<String>,
    pub source_url: String,
    pub repository_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_activity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_release: Option<ProjectRelease>,
    #[serde(default)]
    pub maintenance: MaintenanceSignals,
}

impl ProjectCatalogEntry {
    #[must_use]
    pub fn document(&self) -> ProjectDocument {
        ProjectDocument {
            id: self.id.clone(),
            name: self.name.clone(),
            aliases: self.aliases.clone(),
            description: self.description.clone(),
            topics: self.topics.clone(),
            platforms: self.platforms.clone(),
        }
    }

    #[must_use]
    pub fn confidence_penalty(&self) -> u8 {
        u8::from(self.license.is_none())
            + u8::from(self.last_activity.is_none())
            + u8::from(
                self.maintenance.archived.is_none()
                    || self.maintenance.recent_commits.is_none()
                    || self.maintenance.active_issue_triage.is_none(),
            )
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectCatalog {
    pub schema_version: SchemaVersion,
    pub source_url: String,
    pub generated_at: String,
    pub fingerprint: String,
    pub projects: Vec<ProjectCatalogEntry>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogError {
    InvalidJson,
    InvalidSource,
    InvalidFingerprint,
    EmptyCatalog,
    TooManyProjects,
    InvalidProject,
    DuplicateProject,
    InvalidUrl,
    InvalidFact,
}

impl std::fmt::Display for CatalogError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let code = match self {
            Self::InvalidJson => "invalid_json",
            Self::InvalidSource => "invalid_source",
            Self::InvalidFingerprint => "invalid_fingerprint",
            Self::EmptyCatalog => "empty_catalog",
            Self::TooManyProjects => "too_many_projects",
            Self::InvalidProject => "invalid_project",
            Self::DuplicateProject => "duplicate_project",
            Self::InvalidUrl => "invalid_url",
            Self::InvalidFact => "invalid_fact",
        };
        formatter.write_str(code)
    }
}

impl std::error::Error for CatalogError {}

impl ProjectCatalog {
    pub fn from_slice(bytes: &[u8]) -> Result<Self, CatalogError> {
        let catalog: Self = serde_json::from_slice(bytes).map_err(|_| CatalogError::InvalidJson)?;
        catalog.validate()
    }

    pub fn validate(self) -> Result<Self, CatalogError> {
        if !valid_https_url(&self.source_url) || !valid_fact(&self.generated_at) {
            return Err(CatalogError::InvalidSource);
        }
        if self.projects.is_empty() {
            return Err(CatalogError::EmptyCatalog);
        }
        if self.projects.len() > MAX_PROJECTS {
            return Err(CatalogError::TooManyProjects);
        }
        let mut ids = BTreeSet::new();
        for project in &self.projects {
            if !valid_id(&project.id)
                || !valid_fact(&project.name)
                || !valid_fact(&project.description)
                || project.aliases.len() > 32
                || project.topics.len() > 64
                || project.platforms.len() > 32
            {
                return Err(CatalogError::InvalidProject);
            }
            if !ids.insert(project.id.as_str()) {
                return Err(CatalogError::DuplicateProject);
            }
            if !valid_https_url(&project.source_url) || !valid_https_url(&project.repository_url) {
                return Err(CatalogError::InvalidUrl);
            }
            if project
                .aliases
                .iter()
                .chain(&project.topics)
                .chain(&project.platforms)
                .any(|value| !valid_fact(value))
                || project
                    .license
                    .as_deref()
                    .is_some_and(|value| !valid_fact(value))
                || project
                    .last_activity
                    .as_deref()
                    .is_some_and(|value| !valid_fact(value))
                || project.latest_release.as_ref().is_some_and(|release| {
                    !valid_fact(&release.version) || !valid_fact(&release.published_at)
                })
            {
                return Err(CatalogError::InvalidFact);
            }
        }
        let expected = format!("sha256:{}", hash_json(&self.projects));
        if self.fingerprint != expected {
            return Err(CatalogError::InvalidFingerprint);
        }
        Ok(self)
    }
}

#[must_use]
pub(crate) fn hash_json<T: Serialize>(value: &T) -> String {
    let bytes = serde_json::to_vec(value).unwrap_or_default();
    encode_digest(Sha256::digest(bytes))
}

pub(crate) fn encode_digest(digest: impl IntoIterator<Item = u8>) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let iterator = digest.into_iter();
    let (lower, _) = iterator.size_hint();
    let mut encoded = String::with_capacity(lower * 2);
    for byte in iterator {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'-' | b'_' | b'.' | b'/')
        })
}

fn valid_fact(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= MAX_TEXT_BYTES
        && !value.chars().any(char::is_control)
}

fn valid_https_url(value: &str) -> bool {
    value.starts_with("https://")
        && value.len() <= 2_048
        && value.len() > "https://".len()
        && !value.chars().any(char::is_whitespace)
        && !value.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_facts_lower_confidence_without_becoming_claims() {
        let project = ProjectCatalogEntry {
            id: "example".into(),
            name: "Example".into(),
            aliases: vec![],
            description: "Example project".into(),
            topics: vec![],
            platforms: vec![],
            source_url: "https://example.test/catalog".into(),
            repository_url: "https://example.test/repo".into(),
            license: None,
            last_activity: None,
            latest_release: None,
            maintenance: MaintenanceSignals::default(),
        };
        assert_eq!(project.confidence_penalty(), 3);
    }
}
