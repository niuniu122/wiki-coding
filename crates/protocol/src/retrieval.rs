use serde::{Deserialize, Serialize};

use crate::SchemaVersion;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IndexDomain {
    Capability,
    Project,
    Skill,
    Mcp,
    Wiki,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityKind {
    Project,
    Skill,
    Mcp,
}

impl CapabilityKind {
    #[must_use]
    pub const fn index_domain(self) -> IndexDomain {
        match self {
            Self::Project => IndexDomain::Project,
            Self::Skill => IndexDomain::Skill,
            Self::Mcp => IndexDomain::Mcp,
        }
    }

    #[must_use]
    pub const fn id_prefix(self) -> &'static str {
        match self {
            Self::Project => "project:",
            Self::Skill => "skill:",
            Self::Mcp => "mcp:",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityReadiness {
    Ready,
    NeedsInstall,
    #[serde(rename = "needs_authorization")]
    NeedsAccess,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RetrievalMode {
    Exact,
    Bm25,
    HybridVerified,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RetrievalDegradedReason {
    EmbeddingMissing,
    IncompatibleCpu,
    InvalidManifest,
    HashMismatch,
    RuntimeAbiMismatch,
    FingerprintMismatch,
    HelperUnavailable,
    HelperTimeout,
    HelperCrashed,
    MalformedVector,
    NonFiniteVector,
    WrongDimension,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RetrievalExplanation {
    pub matched_terms: Vec<String>,
    pub lexical_rank: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semantic_rank: Option<u32>,
    pub lexical_score: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fused_score: Option<f64>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RetrievalHitRecord {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub platforms: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_activity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_release: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub maintenance: Vec<String>,
    #[serde(default)]
    pub confidence_penalty: u8,
    pub explanation: RetrievalExplanation,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RetrievalResponse {
    pub schema_version: SchemaVersion,
    pub domain: IndexDomain,
    pub query: String,
    pub keywords: Vec<String>,
    pub mode: RetrievalMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub degraded_reason: Option<RetrievalDegradedReason>,
    pub results: Vec<RetrievalHitRecord>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityWorkspaceHitRecord {
    pub id: String,
    pub kind: CapabilityKind,
    pub title: String,
    pub description: String,
    pub readiness: CapabilityReadiness,
    pub readiness_reason: String,
    pub next_action: String,
    pub source_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platforms: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permissions: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authorizations: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub maintenance: Vec<String>,
    #[serde(default)]
    pub confidence_penalty: u8,
    pub explanation: RetrievalExplanation,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityWorkspaceResponse {
    pub schema_version: SchemaVersion,
    pub query: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_kind: Option<CapabilityKind>,
    pub keywords: Vec<String>,
    pub mode: RetrievalMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub degraded_reason: Option<RetrievalDegradedReason>,
    pub results: Vec<CapabilityWorkspaceHitRecord>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityWorkspaceStatusRecord {
    pub schema_version: SchemaVersion,
    pub catalogs: Vec<IndexStatusRecord>,
    pub workspace_fingerprint: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IndexStatusRecord {
    pub schema_version: SchemaVersion,
    pub domain: IndexDomain,
    pub documents: u64,
    pub mode: RetrievalMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub degraded_reason: Option<RetrievalDegradedReason>,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unknown_result_fields() {
        let value = serde_json::json!({
            "schemaVersion": 1,
            "domain": "project",
            "query": "terminal search",
            "keywords": ["terminal", "search"],
            "mode": "bm25",
            "results": [],
            "surprise": true
        });

        assert!(serde_json::from_value::<RetrievalResponse>(value).is_err());
    }

    #[test]
    fn capability_workspace_records_round_trip_strictly() {
        let response = CapabilityWorkspaceResponse {
            schema_version: SchemaVersion,
            query: "find docs".into(),
            selected_kind: Some(CapabilityKind::Skill),
            keywords: vec!["docs".into()],
            mode: RetrievalMode::Bm25,
            degraded_reason: Some(RetrievalDegradedReason::EmbeddingMissing),
            results: vec![CapabilityWorkspaceHitRecord {
                id: "skill:openai/openai-docs".into(),
                kind: CapabilityKind::Skill,
                title: "OpenAI Docs".into(),
                description: "Find official documentation".into(),
                readiness: CapabilityReadiness::NeedsInstall,
                readiness_reason: "This Skill is not installed.".into(),
                next_action: "Review the source, then confirm installation separately.".into(),
                source_url: "https://github.com/openai/skills".into(),
                repository_url: Some("https://github.com/openai/skills".into()),
                license: None,
                platforms: None,
                permissions: None,
                authorizations: None,
                maintenance: Vec::new(),
                confidence_penalty: 3,
                explanation: RetrievalExplanation {
                    matched_terms: vec!["docs".into()],
                    lexical_rank: 1,
                    semantic_rank: None,
                    lexical_score: 1.0,
                    fused_score: None,
                },
            }],
        };
        let encoded = serde_json::to_value(&response).expect("serialize");
        assert_eq!(
            serde_json::from_value::<CapabilityWorkspaceResponse>(encoded).expect("strict parse"),
            response
        );
    }
}
