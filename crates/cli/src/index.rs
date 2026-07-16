use std::path::Path;

use minimax_protocol::{
    IndexDomain, IndexStatusRecord, KnowledgePageStatus, ProjectId, RetrievalDegradedReason,
    RetrievalExplanation, RetrievalHitRecord, RetrievalMode, RetrievalResponse, SchemaVersion,
};
use minimax_retrieval::{
    CapabilityDocument, CapabilityIndex, CatalogError, EmbeddingHost, EmbeddingSelection,
    ProcessEmbeddingRunner, ProjectCatalog, ProjectDiscovery, SearchDocument, WikiDocument,
    WikiIndex, validate_embedding_resource,
};
use minimax_vault::{ProjectVault, VaultError, read_wiki_pages};

#[derive(Debug)]
pub enum IndexError {
    Read,
    Catalog(CatalogError),
    Vault(VaultError),
}

impl std::fmt::Display for IndexError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Read => formatter.write_str("index source could not be read"),
            Self::Catalog(error) => write!(formatter, "project catalog is invalid: {error}"),
            Self::Vault(error) => write!(formatter, "Vault search failed: {error}"),
        }
    }
}

impl std::error::Error for IndexError {}

impl From<VaultError> for IndexError {
    fn from(value: VaultError) -> Self {
        Self::Vault(value)
    }
}

#[must_use]
pub fn capability_status() -> IndexStatusRecord {
    IndexStatusRecord {
        schema_version: SchemaVersion,
        domain: IndexDomain::Capability,
        documents: u64::try_from(capability_documents().len()).unwrap_or(u64::MAX),
        mode: RetrievalMode::Bm25,
        degraded_reason: None,
        source: "rust_builtin_commands".to_owned(),
        fingerprint: None,
    }
}

#[must_use]
pub fn capability_search(query: &str, limit: usize) -> RetrievalResponse {
    let index = CapabilityIndex::new(capability_documents());
    let hits = index.search(query, limit);
    let mode = hits.first().map_or(RetrievalMode::Bm25, |hit| hit.mode);
    let keywords = keywords(&hits);
    RetrievalResponse {
        schema_version: SchemaVersion,
        domain: IndexDomain::Capability,
        query: query.to_owned(),
        keywords,
        mode,
        degraded_reason: None,
        results: hits
            .into_iter()
            .enumerate()
            .map(|(rank, hit)| RetrievalHitRecord {
                id: hit.document.id,
                title: hit.document.name,
                source_url: None,
                repository_url: None,
                license: None,
                platforms: Vec::new(),
                last_activity: None,
                latest_release: None,
                maintenance: Vec::new(),
                confidence_penalty: 0,
                explanation: RetrievalExplanation {
                    matched_terms: hit
                        .contributions
                        .into_iter()
                        .map(|item| item.term)
                        .take(8)
                        .collect(),
                    lexical_rank: bounded_rank(rank),
                    semantic_rank: None,
                    lexical_score: finite_score(hit.score),
                    fused_score: None,
                },
            })
            .collect(),
    }
}

pub fn project_status(
    catalog_path: &Path,
    embedding_resource: Option<&Path>,
) -> Result<IndexStatusRecord, IndexError> {
    let catalog = load_catalog(catalog_path)?;
    let (mode, degraded_reason) = embedding_status(&catalog, embedding_resource);
    Ok(IndexStatusRecord {
        schema_version: SchemaVersion,
        domain: IndexDomain::Project,
        documents: u64::try_from(catalog.projects.len()).unwrap_or(u64::MAX),
        mode,
        degraded_reason,
        source: catalog.source_url.clone(),
        fingerprint: Some(catalog.fingerprint),
    })
}

pub async fn project_search(
    catalog_path: &Path,
    embedding_resource: Option<&Path>,
    query: &str,
    limit: usize,
) -> Result<RetrievalResponse, IndexError> {
    let catalog = load_catalog(catalog_path)?;
    let discovery = ProjectDiscovery::new(catalog.clone());
    let runner = ProcessEmbeddingRunner::default();
    let verified = embedding_resource.map(|directory| {
        validate_embedding_resource(directory, &EmbeddingHost::detect(), &catalog.fingerprint)
    });
    let selection = match verified.as_ref() {
        Some(Ok(resource)) => EmbeddingSelection::Verified {
            resource,
            runner: &runner,
        },
        Some(Err(reason)) => EmbeddingSelection::Unavailable(*reason),
        None => EmbeddingSelection::Unavailable(RetrievalDegradedReason::EmbeddingMissing),
    };
    let result = discovery.discover(query, limit, selection).await;
    Ok(RetrievalResponse {
        schema_version: SchemaVersion,
        domain: IndexDomain::Project,
        query: result.query,
        keywords: result.keywords,
        mode: result.mode,
        degraded_reason: result.degraded_reason,
        results: result
            .hits
            .into_iter()
            .map(|hit| {
                let maintenance = maintenance_facts(&hit.project);
                let confidence_penalty = hit.project.confidence_penalty();
                RetrievalHitRecord {
                    id: hit.project.id,
                    title: hit.project.name,
                    source_url: Some(hit.project.source_url),
                    repository_url: Some(hit.project.repository_url),
                    license: hit.project.license,
                    platforms: hit.project.platforms,
                    last_activity: hit.project.last_activity,
                    latest_release: hit
                        .project
                        .latest_release
                        .map(|release| format!("{}@{}", release.version, release.published_at)),
                    maintenance,
                    confidence_penalty,
                    explanation: RetrievalExplanation {
                        matched_terms: hit.matched_terms,
                        lexical_rank: u32::try_from(hit.lexical_rank).unwrap_or(u32::MAX),
                        semantic_rank: hit.semantic_rank.and_then(|rank| u32::try_from(rank).ok()),
                        lexical_score: finite_score(hit.lexical_score),
                        fused_score: hit.fused_score,
                    },
                }
            })
            .collect(),
    })
}

pub fn wiki_status(
    project_root: &Path,
    vault_root: &Path,
    project_id: ProjectId,
) -> Result<IndexStatusRecord, IndexError> {
    let vault = ProjectVault::open_read_only(project_root, vault_root, project_id)?;
    let pages = read_wiki_pages(&vault)?;
    let current = pages
        .iter()
        .filter(|(_, page)| page.status == KnowledgePageStatus::Current)
        .count();
    Ok(IndexStatusRecord {
        schema_version: SchemaVersion,
        domain: IndexDomain::Wiki,
        documents: u64::try_from(current).unwrap_or(u64::MAX),
        mode: RetrievalMode::Bm25,
        degraded_reason: None,
        source: vault.root().display().to_string(),
        fingerprint: None,
    })
}

pub fn wiki_search(
    project_root: &Path,
    vault_root: &Path,
    project_id: ProjectId,
    query: &str,
    limit: usize,
) -> Result<RetrievalResponse, IndexError> {
    let vault = ProjectVault::open_read_only(project_root, vault_root, project_id)?;
    let documents = read_wiki_pages(&vault)?
        .into_values()
        .map(|page| WikiDocument {
            id: page.page_id.as_str().to_owned(),
            title: page.title,
            body: page.body,
            aliases: Vec::new(),
            current: page.status == KnowledgePageStatus::Current,
        })
        .collect();
    let hits = WikiIndex::new(documents).search(query, limit);
    let mode = hits.first().map_or(RetrievalMode::Bm25, |hit| hit.mode);
    let keywords = keywords(&hits);
    Ok(RetrievalResponse {
        schema_version: SchemaVersion,
        domain: IndexDomain::Wiki,
        query: query.to_owned(),
        keywords,
        mode,
        degraded_reason: None,
        results: hits
            .into_iter()
            .enumerate()
            .map(|(rank, hit)| RetrievalHitRecord {
                id: hit.document.id,
                title: hit.document.title,
                source_url: None,
                repository_url: None,
                license: None,
                platforms: Vec::new(),
                last_activity: None,
                latest_release: None,
                maintenance: Vec::new(),
                confidence_penalty: 0,
                explanation: RetrievalExplanation {
                    matched_terms: hit
                        .contributions
                        .into_iter()
                        .map(|item| item.term)
                        .take(8)
                        .collect(),
                    lexical_rank: bounded_rank(rank),
                    semantic_rank: None,
                    lexical_score: finite_score(hit.score),
                    fused_score: None,
                },
            })
            .collect(),
    })
}

fn load_catalog(path: &Path) -> Result<ProjectCatalog, IndexError> {
    let bytes = std::fs::read(path).map_err(|_| IndexError::Read)?;
    ProjectCatalog::from_slice(&bytes).map_err(IndexError::Catalog)
}

fn embedding_status(
    catalog: &ProjectCatalog,
    embedding_resource: Option<&Path>,
) -> (RetrievalMode, Option<RetrievalDegradedReason>) {
    let Some(directory) = embedding_resource else {
        return (
            RetrievalMode::Bm25,
            Some(RetrievalDegradedReason::EmbeddingMissing),
        );
    };
    match validate_embedding_resource(directory, &EmbeddingHost::detect(), &catalog.fingerprint) {
        // Resource validation alone is not helper health. Search may claim hybrid only
        // after the helper returns a fully validated candidate-vector response.
        Ok(_) => (RetrievalMode::Bm25, None),
        Err(reason) => (RetrievalMode::Bm25, Some(reason)),
    }
}

fn maintenance_facts(project: &minimax_retrieval::ProjectCatalogEntry) -> Vec<String> {
    let mut facts = Vec::new();
    if let Some(value) = project.maintenance.archived {
        facts.push(format!("archived={value}"));
    }
    if let Some(value) = project.maintenance.recent_commits {
        facts.push(format!("recent_commits={value}"));
    }
    if let Some(value) = project.maintenance.active_issue_triage {
        facts.push(format!("active_issue_triage={value}"));
    }
    facts
}

fn keywords<D: SearchDocument>(hits: &[minimax_retrieval::LexicalHit<D>]) -> Vec<String> {
    let mut keywords = hits
        .iter()
        .flat_map(|hit| hit.contributions.iter())
        .filter(|item| item.term.chars().count() > 1)
        .map(|item| item.term.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    keywords.truncate(8);
    keywords
}

fn finite_score(value: f64) -> f64 {
    if value.is_finite() { value } else { 1.0 }
}

fn bounded_rank(zero_based: usize) -> u32 {
    u32::try_from(zero_based.saturating_add(1)).unwrap_or(u32::MAX)
}

fn capability_documents() -> Vec<CapabilityDocument> {
    [
        (
            "run",
            "Run one turn",
            &["/run"][..],
            "run one model turn headless JSONL 执行 单轮",
        ),
        (
            "chat",
            "Interactive chat",
            &["/chat"][..],
            "interactive terminal chat conversation 交互 对话",
        ),
        (
            "doctor",
            "Diagnose configuration",
            &["/doctor"][..],
            "diagnose inspect provider configuration health 检查 配置",
        ),
        (
            "vault",
            "Maintain project Vault",
            &["/vault"][..],
            "vault wiki lint repair rebuild import garbage collection knowledge 维护 知识",
        ),
        (
            "capabilities",
            "Search capabilities",
            &["/capabilities"][..],
            "search inspect available commands capabilities 搜索 能力 命令",
        ),
        (
            "projects",
            "Find open-source projects",
            &["index projects search"][..],
            "find open source software project BM25 embedding recommendation 查找 开源 项目",
        ),
    ]
    .into_iter()
    .map(|(id, name, aliases, intent)| CapabilityDocument {
        id: format!("command:{id}"),
        name: name.to_owned(),
        description: intent.to_owned(),
        aliases: aliases.iter().map(|value| (*value).to_owned()).collect(),
        commands: aliases.iter().map(|value| (*value).to_owned()).collect(),
        intent_document: format!("{name}\n{intent}"),
        available: true,
    })
    .collect()
}
