use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Mutex;

use minimax_protocol::{RetrievalDegradedReason, RetrievalMode, SchemaVersion};
use minimax_retrieval::{
    CandidateVector, CatalogError, EMBEDDING_HELPER_ABI, EmbeddingOutput, EmbeddingRequest,
    EmbeddingResourceManifest, EmbeddingRunner, EmbeddingSelection, GRANITE_EMBEDDING_MODEL_ID,
    GRANITE_RESOURCE_PACKAGE_ID, ProjectCatalog, ProjectDiscovery, VerifiedEmbeddingResource,
};

fn catalog() -> ProjectCatalog {
    ProjectCatalog::from_slice(include_bytes!(
        "../../../fixtures/compat/retrieval/projects.v1.json"
    ))
    .expect("valid project catalog")
}

fn resource(catalog_fingerprint: &str) -> VerifiedEmbeddingResource {
    VerifiedEmbeddingResource {
        directory: PathBuf::from("fixture-resource"),
        helper_path: PathBuf::from("fixture-resource/helper"),
        manifest: EmbeddingResourceManifest {
            schema_version: SchemaVersion,
            package_id: GRANITE_RESOURCE_PACKAGE_ID.into(),
            model_id: GRANITE_EMBEDDING_MODEL_ID.into(),
            model_revision: "fixture-revision".into(),
            runtime_abi: EMBEDDING_HELPER_ABI.into(),
            architecture: "x64-avx2".into(),
            quantization: "qint8".into(),
            license: "Apache-2.0".into(),
            tokenizer_version: "fixture-tokenizer".into(),
            dimensions: 3,
            catalog_fingerprint: catalog_fingerprint.into(),
            vector_fingerprint: format!("sha256:{}", "2".repeat(64)),
            helper_relative_path: "helper".into(),
            platform_health: "verified".into(),
            files: vec![],
        },
    }
}

struct ScriptedRunner {
    requests: Mutex<Vec<EmbeddingRequest>>,
    failure: Option<RetrievalDegradedReason>,
    outsider: bool,
}

impl ScriptedRunner {
    fn success() -> Self {
        Self {
            requests: Mutex::new(Vec::new()),
            failure: None,
            outsider: false,
        }
    }
}

impl EmbeddingRunner for ScriptedRunner {
    fn embed<'a>(
        &'a self,
        resource: &'a VerifiedEmbeddingResource,
        request: &'a EmbeddingRequest,
    ) -> Pin<Box<dyn Future<Output = Result<EmbeddingOutput, RetrievalDegradedReason>> + Send + 'a>>
    {
        Box::pin(async move {
            self.requests
                .lock()
                .expect("requests")
                .push(request.clone());
            if let Some(reason) = self.failure {
                return Err(reason);
            }
            let mut candidates = request
                .candidates
                .iter()
                .enumerate()
                .map(|(index, candidate)| CandidateVector {
                    id: candidate.id.clone(),
                    vector: match index % 3 {
                        0 => vec![1.0, 0.0, 0.0],
                        1 => vec![0.0, 1.0, 0.0],
                        _ => vec![0.0, 0.0, 1.0],
                    },
                })
                .collect::<Vec<_>>();
            if self.outsider {
                candidates[0].id = "outside/bm25".into();
            }
            Ok(EmbeddingOutput {
                schema_version: SchemaVersion,
                model_id: resource.manifest.model_id.clone(),
                runtime_abi: resource.manifest.runtime_abi.clone(),
                catalog_fingerprint: request.catalog_fingerprint.clone(),
                vector_fingerprint: request.vector_fingerprint.clone(),
                dimensions: 3,
                query_vector: vec![0.0, 1.0, 0.0],
                candidates,
            })
        })
    }
}

#[test]
fn strict_catalog_rejects_unknown_duplicate_unsafe_and_drifted_data() {
    let original: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../../fixtures/compat/retrieval/projects.v1.json"
    ))
    .expect("fixture");
    let loaded = catalog();
    assert!(
        loaded
            .projects
            .iter()
            .all(|project| project.license.is_none())
    );

    let mut unknown = original.clone();
    unknown["surprise"] = serde_json::Value::Bool(true);
    assert_eq!(
        ProjectCatalog::from_slice(&serde_json::to_vec(&unknown).expect("json")),
        Err(CatalogError::InvalidJson)
    );

    let mut duplicate = original.clone();
    let first = duplicate["projects"][0].clone();
    duplicate["projects"]
        .as_array_mut()
        .expect("projects")
        .push(first);
    assert_eq!(
        ProjectCatalog::from_slice(&serde_json::to_vec(&duplicate).expect("json")),
        Err(CatalogError::DuplicateProject)
    );

    let mut unsafe_url = original.clone();
    unsafe_url["projects"][0]["repositoryUrl"] =
        serde_json::Value::String("http://unsafe.test/repo".into());
    assert_eq!(
        ProjectCatalog::from_slice(&serde_json::to_vec(&unsafe_url).expect("json")),
        Err(CatalogError::InvalidUrl)
    );

    let mut drift = original;
    drift["projects"][0]["description"] = serde_json::Value::String("drifted".into());
    assert_eq!(
        ProjectCatalog::from_slice(&serde_json::to_vec(&drift).expect("json")),
        Err(CatalogError::InvalidFingerprint)
    );
}

#[tokio::test]
async fn bm25_runs_first_and_embedding_receives_only_its_candidates() {
    let catalog = catalog();
    let discovery = ProjectDiscovery::new(catalog.clone());
    let lexical = discovery
        .discover(
            "fast command line file search",
            5,
            EmbeddingSelection::Unavailable(RetrievalDegradedReason::EmbeddingMissing),
        )
        .await;
    assert_eq!(lexical.mode, RetrievalMode::Bm25);
    assert_eq!(
        lexical.degraded_reason,
        Some(RetrievalDegradedReason::EmbeddingMissing)
    );
    assert!(!lexical.hits.is_empty());
    assert!(!lexical.keywords.is_empty());

    let runner = ScriptedRunner::success();
    let resource = resource(&catalog.fingerprint);
    let hybrid = discovery
        .discover(
            "fast command line file search",
            5,
            EmbeddingSelection::Verified {
                resource: &resource,
                runner: &runner,
            },
        )
        .await;
    assert_eq!(hybrid.mode, RetrievalMode::HybridVerified);
    assert_eq!(hybrid.degraded_reason, None);
    let requests = runner.requests.lock().expect("requests");
    assert_eq!(requests.len(), 1);
    let lexical_ids = lexical
        .hits
        .iter()
        .map(|hit| hit.project.id.as_str())
        .collect::<Vec<_>>();
    let embedded_ids = requests[0]
        .candidates
        .iter()
        .map(|candidate| candidate.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(embedded_ids, lexical_ids);
    assert!(
        hybrid
            .hits
            .iter()
            .all(|hit| lexical_ids.contains(&hit.project.id.as_str()))
    );
}

#[tokio::test]
async fn every_semantic_failure_preserves_the_bm25_order() {
    let catalog = catalog();
    let discovery = ProjectDiscovery::new(catalog.clone());
    let baseline = discovery
        .discover(
            "coding assistant terminal project",
            5,
            EmbeddingSelection::Unavailable(RetrievalDegradedReason::EmbeddingMissing),
        )
        .await;
    let baseline_ids = baseline
        .hits
        .iter()
        .map(|hit| hit.project.id.clone())
        .collect::<Vec<_>>();
    let resource = resource(&catalog.fingerprint);
    for reason in [
        RetrievalDegradedReason::HelperUnavailable,
        RetrievalDegradedReason::HelperTimeout,
        RetrievalDegradedReason::HelperCrashed,
        RetrievalDegradedReason::NonFiniteVector,
        RetrievalDegradedReason::WrongDimension,
    ] {
        let runner = ScriptedRunner {
            requests: Mutex::new(Vec::new()),
            failure: Some(reason),
            outsider: false,
        };
        let result = discovery
            .discover(
                "coding assistant terminal project",
                5,
                EmbeddingSelection::Verified {
                    resource: &resource,
                    runner: &runner,
                },
            )
            .await;
        assert_eq!(result.mode, RetrievalMode::Bm25);
        assert_eq!(result.degraded_reason, Some(reason));
        assert_eq!(
            result
                .hits
                .iter()
                .map(|hit| hit.project.id.clone())
                .collect::<Vec<_>>(),
            baseline_ids
        );
    }

    let outsider = ScriptedRunner {
        requests: Mutex::new(Vec::new()),
        failure: None,
        outsider: true,
    };
    let result = discovery
        .discover(
            "coding assistant terminal project",
            5,
            EmbeddingSelection::Verified {
                resource: &resource,
                runner: &outsider,
            },
        )
        .await;
    assert_eq!(
        result.degraded_reason,
        Some(RetrievalDegradedReason::MalformedVector)
    );
    assert_eq!(
        result
            .hits
            .iter()
            .map(|hit| hit.project.id.clone())
            .collect::<Vec<_>>(),
        baseline_ids
    );
}
