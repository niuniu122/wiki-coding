use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Mutex;

use minimax_protocol::{
    CapabilityKind, CapabilityReadiness, RetrievalDegradedReason, RetrievalMode, SchemaVersion,
};
use minimax_retrieval::{
    CandidateVector, CapabilityCard, CapabilityCatalog, CapabilityCatalogError,
    CapabilityInventory, CapabilityWorkspace, CapabilityWorkspaceCatalog, EMBEDDING_HELPER_ABI,
    EmbeddingOutput, EmbeddingRequest, EmbeddingResourceManifest, EmbeddingRunner,
    EmbeddingSelection, GRANITE_EMBEDDING_MODEL_ID, GRANITE_RESOURCE_PACKAGE_ID,
    VerifiedEmbeddingResource,
};
use serde_json::{Value, json};

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CapabilityEvalFixture {
    schema_version: SchemaVersion,
    cases: Vec<CapabilityEvalCase>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CapabilityEvalCase {
    id: String,
    query: String,
    kind: CapabilityKind,
    expected_top: Option<String>,
}

fn hash_json(value: &Value) -> String {
    let cards: Vec<CapabilityCard> =
        serde_json::from_value(value.clone()).expect("typed capability cards");
    CapabilityCatalog::fingerprint_for_cards(&cards)
}

fn catalog(kind: &str, cards: Vec<Value>) -> Vec<u8> {
    let cards = Value::Array(cards);
    serde_json::to_vec(&json!({
        "schemaVersion": 1,
        "kind": kind,
        "sourceUrl": format!("https://catalog.example/{kind}"),
        "generatedAt": "2026-07-17T00:00:00Z",
        "fingerprint": hash_json(&cards),
        "cards": cards
    }))
    .expect("catalog")
}

fn card(id: &str, kind: &str, name: &str, description: &str) -> Value {
    json!({
        "id": id,
        "kind": kind,
        "name": name,
        "aliases": [],
        "description": description,
        "intents": [description],
        "sourceUrl": format!("https://catalog.example/cards/{id}"),
        "installKind": "external",
        "installGuidance": "Review the source and confirm installation in a separate workflow."
    })
}

fn catalogs() -> CapabilityWorkspaceCatalog {
    CapabilityWorkspaceCatalog::from_slices(
        &catalog(
            "project",
            vec![card(
                "project:example/search",
                "project",
                "Search Project",
                "fast command line file search 文件 搜索",
            )],
        ),
        &catalog(
            "skill",
            vec![card(
                "skill:example/docs",
                "skill",
                "Docs Skill",
                "find official documentation 文档",
            )],
        ),
        &catalog(
            "mcp",
            vec![json!({
                "id": "mcp:example/github",
                "kind": "mcp",
                "name": "GitHub MCP",
                "aliases": ["github server"],
                "description": "work with repositories issues and pull requests 仓库",
                "intents": ["repository issues pull requests", "仓库 issue"],
                "sourceUrl": "https://catalog.example/cards/mcp-github",
                "repositoryUrl": "https://github.com/example/github-mcp",
                "license": "MIT",
                "installKind": "external",
                "installGuidance": "Review the source and confirm installation in a separate workflow.",
                "authorizations": ["github_personal_access_token"],
                "permissions": ["github_api_access"]
            })],
        ),
    )
    .expect("workspace catalogs")
}

fn bundled_catalogs() -> CapabilityWorkspaceCatalog {
    CapabilityWorkspaceCatalog::from_slices(
        include_bytes!("../../../capabilities/catalogs/projects.v1.json"),
        include_bytes!("../../../capabilities/catalogs/skills.v1.json"),
        include_bytes!("../../../capabilities/catalogs/mcp.v1.json"),
    )
    .expect("bundled workspace catalogs")
}

#[test]
fn strict_catalogs_reject_cross_kind_unknown_executable_duplicate_and_drifted_cards() {
    let valid = catalog(
        "skill",
        vec![card(
            "skill:example/docs",
            "skill",
            "Docs Skill",
            "official documentation",
        )],
    );

    let mut cross_kind: Value = serde_json::from_slice(&valid).expect("JSON");
    cross_kind["cards"][0]["kind"] = Value::String("mcp".into());
    let cards = cross_kind["cards"].clone();
    cross_kind["fingerprint"] = Value::String(hash_json(&cards));
    assert_eq!(
        CapabilityWorkspaceCatalog::from_slices(
            &catalog("project", vec![]),
            &serde_json::to_vec(&cross_kind).expect("json"),
            &catalog("mcp", vec![]),
        ),
        Err(CapabilityCatalogError::KindMismatch)
    );

    let mut unknown: Value = serde_json::from_slice(&valid).expect("JSON");
    unknown["cards"][0]["surprise"] = Value::Bool(true);
    assert_eq!(
        CapabilityWorkspaceCatalog::from_slices(
            &catalog("project", vec![]),
            &serde_json::to_vec(&unknown).expect("json"),
            &catalog("mcp", vec![]),
        ),
        Err(CapabilityCatalogError::InvalidJson)
    );

    let mut executable: Value = serde_json::from_slice(&valid).expect("JSON");
    executable["cards"][0]["installGuidance"] = Value::String("npm install unsafe".into());
    let cards = executable["cards"].clone();
    executable["fingerprint"] = Value::String(hash_json(&cards));
    assert_eq!(
        CapabilityWorkspaceCatalog::from_slices(
            &catalog("project", vec![]),
            &serde_json::to_vec(&executable).expect("json"),
            &catalog("mcp", vec![]),
        ),
        Err(CapabilityCatalogError::UnsafeGuidance)
    );

    let duplicate = card(
        "project:example/search",
        "project",
        "Duplicate",
        "duplicate",
    );
    assert_eq!(
        CapabilityWorkspaceCatalog::from_slices(
            &catalog("project", vec![duplicate.clone()]),
            &catalog("skill", vec![]),
            &catalog(
                "mcp",
                vec![json!({
                    "id": "project:example/search",
                    "kind": "mcp",
                    "name": "Wrong duplicate",
                    "description": "duplicate",
                    "intents": ["duplicate"],
                    "sourceUrl": "https://catalog.example/duplicate",
                    "installKind": "external",
                    "installGuidance": "Review the source in a separate workflow."
                })],
            ),
        ),
        Err(CapabilityCatalogError::InvalidCard)
    );

    let mut drift: Value = serde_json::from_slice(&valid).expect("JSON");
    drift["cards"][0]["description"] = Value::String("drifted".into());
    assert_eq!(
        CapabilityWorkspaceCatalog::from_slices(
            &catalog("project", vec![]),
            &serde_json::to_vec(&drift).expect("json"),
            &catalog("mcp", vec![]),
        ),
        Err(CapabilityCatalogError::InvalidFingerprint)
    );
}

#[tokio::test]
async fn isolated_kinds_search_offline_and_readiness_precedence_is_truthful() {
    let workspace = CapabilityWorkspace::new(catalogs());
    let empty = CapabilityInventory::default();
    let project = workspace
        .discover(
            "快速 文件 搜索",
            Some(CapabilityKind::Project),
            5,
            &empty,
            EmbeddingSelection::Unavailable(RetrievalDegradedReason::EmbeddingMissing),
        )
        .await;
    assert_eq!(project.mode, RetrievalMode::Bm25);
    assert_eq!(project.hits.len(), 1);
    assert_eq!(project.hits[0].card.kind, CapabilityKind::Project);
    assert_eq!(project.hits[0].readiness, CapabilityReadiness::NeedsInstall);

    let installed = CapabilityInventory::new(
        ["mcp:example/github".to_owned()],
        std::iter::empty::<String>(),
    )
    .expect("inventory");
    let mcp = workspace
        .discover(
            "管理 github 仓库 issue",
            Some(CapabilityKind::Mcp),
            5,
            &installed,
            EmbeddingSelection::Unavailable(RetrievalDegradedReason::EmbeddingMissing),
        )
        .await;
    assert_eq!(mcp.hits[0].readiness, CapabilityReadiness::NeedsAccess);

    let ready = CapabilityInventory::new(
        ["mcp:example/github".to_owned()],
        ["mcp:example/github".to_owned()],
    )
    .expect("inventory");
    let mcp = workspace
        .discover(
            "管理 github 仓库 issue",
            Some(CapabilityKind::Mcp),
            5,
            &ready,
            EmbeddingSelection::Unavailable(RetrievalDegradedReason::EmbeddingMissing),
        )
        .await;
    assert_eq!(mcp.hits[0].readiness, CapabilityReadiness::Ready);

    let no_skill = workspace
        .discover(
            "管理 github 仓库 issue",
            Some(CapabilityKind::Skill),
            5,
            &empty,
            EmbeddingSelection::Unavailable(RetrievalDegradedReason::EmbeddingMissing),
        )
        .await;
    assert!(no_skill.hits.is_empty());
}

#[tokio::test]
async fn bundled_mixed_language_fixture_preserves_kind_isolation_and_truthful_no_match() {
    let fixture: CapabilityEvalFixture = serde_json::from_slice(include_bytes!(
        "../../../fixtures/compat/retrieval/capability-workspace.v1.json"
    ))
    .expect("capability eval fixture");
    let _ = fixture.schema_version;
    assert_eq!(fixture.cases.len(), 15);
    let workspace = CapabilityWorkspace::new(bundled_catalogs());
    let inventory = CapabilityInventory::default();
    for case in fixture.cases {
        let result = workspace
            .discover(
                &case.query,
                Some(case.kind),
                5,
                &inventory,
                EmbeddingSelection::Unavailable(RetrievalDegradedReason::EmbeddingMissing),
            )
            .await;
        assert_eq!(
            result.hits.first().map(|hit| hit.card.id.as_str()),
            case.expected_top.as_deref(),
            "fixture case {}",
            case.id
        );
        assert!(result.hits.iter().all(|hit| hit.card.kind == case.kind));
        assert!(
            result
                .hits
                .iter()
                .all(|hit| hit.readiness == CapabilityReadiness::NeedsInstall)
        );
    }
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

struct RecordingRunner {
    requests: Mutex<Vec<EmbeddingRequest>>,
    outsider: bool,
}

impl EmbeddingRunner for RecordingRunner {
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
            let mut candidates = request
                .candidates
                .iter()
                .enumerate()
                .map(|(index, candidate)| CandidateVector {
                    id: candidate.id.clone(),
                    vector: if index == 0 {
                        vec![1.0, 0.0, 0.0]
                    } else {
                        vec![0.0, 1.0, 0.0]
                    },
                })
                .collect::<Vec<_>>();
            if self.outsider {
                candidates[0].id = "mcp:outside/candidate".into();
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

#[tokio::test]
async fn embedding_sees_only_lexical_union_and_outsiders_preserve_bm25() {
    let workspace = CapabilityWorkspace::new(catalogs());
    let inventory = CapabilityInventory::default();
    let baseline = workspace
        .discover(
            "repository documentation search",
            None,
            5,
            &inventory,
            EmbeddingSelection::Unavailable(RetrievalDegradedReason::EmbeddingMissing),
        )
        .await;
    let baseline_ids = baseline
        .hits
        .iter()
        .map(|hit| hit.card.id.clone())
        .collect::<Vec<_>>();
    assert!(!baseline_ids.is_empty());

    let runner = RecordingRunner {
        requests: Mutex::new(Vec::new()),
        outsider: false,
    };
    let resource = resource(workspace.fingerprint());
    let hybrid = workspace
        .discover(
            "repository documentation search",
            None,
            5,
            &inventory,
            EmbeddingSelection::Verified {
                resource: &resource,
                runner: &runner,
            },
        )
        .await;
    assert_eq!(hybrid.mode, RetrievalMode::HybridVerified);
    let request_ids = runner.requests.lock().expect("requests")[0]
        .candidates
        .iter()
        .map(|candidate| candidate.id.clone())
        .collect::<Vec<_>>();
    assert_eq!(request_ids, baseline_ids);

    let outsider = RecordingRunner {
        requests: Mutex::new(Vec::new()),
        outsider: true,
    };
    let degraded = workspace
        .discover(
            "repository documentation search",
            None,
            5,
            &inventory,
            EmbeddingSelection::Verified {
                resource: &resource,
                runner: &outsider,
            },
        )
        .await;
    assert_eq!(degraded.mode, RetrievalMode::Bm25);
    assert_eq!(
        degraded.degraded_reason,
        Some(RetrievalDegradedReason::MalformedVector)
    );
    assert_eq!(
        degraded
            .hits
            .iter()
            .map(|hit| hit.card.id.clone())
            .collect::<Vec<_>>(),
        baseline_ids
    );
}
