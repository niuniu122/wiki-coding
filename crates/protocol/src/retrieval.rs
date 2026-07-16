use serde::{Deserialize, Serialize};

use crate::SchemaVersion;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IndexDomain {
    Capability,
    Project,
    Wiki,
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
}
