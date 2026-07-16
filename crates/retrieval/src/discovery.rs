use std::collections::{BTreeMap, BTreeSet};

use minimax_protocol::{RetrievalDegradedReason, RetrievalMode, SchemaVersion};

use crate::{
    CandidateVector, EmbeddingCandidate, EmbeddingOutput, EmbeddingRequest, EmbeddingRunner,
    ProjectCatalog, ProjectCatalogEntry, ProjectIndex, SearchDocument, VerifiedEmbeddingResource,
    cosine_similarity, reciprocal_rank_fusion,
};

const BM25_CANDIDATE_LIMIT: usize = 20;

pub enum EmbeddingSelection<'a> {
    Unavailable(RetrievalDegradedReason),
    Verified {
        resource: &'a VerifiedEmbeddingResource,
        runner: &'a dyn EmbeddingRunner,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProjectDiscoveryHit {
    pub project: ProjectCatalogEntry,
    pub matched_terms: Vec<String>,
    pub lexical_rank: usize,
    pub semantic_rank: Option<usize>,
    pub lexical_score: f64,
    pub fused_score: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProjectDiscoveryResult {
    pub query: String,
    pub keywords: Vec<String>,
    pub mode: RetrievalMode,
    pub degraded_reason: Option<RetrievalDegradedReason>,
    pub hits: Vec<ProjectDiscoveryHit>,
}

#[derive(Clone, Debug)]
pub struct ProjectDiscovery {
    catalog: ProjectCatalog,
    index: ProjectIndex,
    by_id: BTreeMap<String, ProjectCatalogEntry>,
}

impl ProjectDiscovery {
    #[must_use]
    pub fn new(catalog: ProjectCatalog) -> Self {
        let by_id = catalog
            .projects
            .iter()
            .map(|project| (project.id.clone(), project.clone()))
            .collect();
        let index = ProjectIndex::new(
            catalog
                .projects
                .iter()
                .map(ProjectCatalogEntry::document)
                .collect(),
        );
        Self {
            catalog,
            index,
            by_id,
        }
    }

    #[must_use]
    pub fn catalog(&self) -> &ProjectCatalog {
        &self.catalog
    }

    pub async fn discover(
        &self,
        query: &str,
        limit: usize,
        embedding: EmbeddingSelection<'_>,
    ) -> ProjectDiscoveryResult {
        let lexical = self.index.search(query, BM25_CANDIDATE_LIMIT);
        let mode = lexical.first().map_or(RetrievalMode::Bm25, |hit| hit.mode);
        let keywords = lexical
            .iter()
            .flat_map(|hit| hit.contributions.iter())
            .filter(|contribution| contribution.term.chars().count() > 1)
            .map(|contribution| contribution.term.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .take(8)
            .collect::<Vec<_>>();
        let baseline = lexical
            .iter()
            .enumerate()
            .filter_map(|(rank, hit)| {
                self.by_id
                    .get(hit.document.id())
                    .cloned()
                    .map(|project| ProjectDiscoveryHit {
                        project,
                        matched_terms: hit
                            .contributions
                            .iter()
                            .map(|item| item.term.clone())
                            .take(8)
                            .collect(),
                        lexical_rank: rank + 1,
                        semantic_rank: None,
                        lexical_score: hit.score,
                        fused_score: None,
                    })
            })
            .collect::<Vec<_>>();
        if mode == RetrievalMode::Exact {
            return ProjectDiscoveryResult {
                query: query.to_owned(),
                keywords,
                mode,
                degraded_reason: None,
                hits: baseline.into_iter().take(limit).collect(),
            };
        }
        if baseline.is_empty() {
            let degraded_reason = match embedding {
                EmbeddingSelection::Unavailable(reason) => Some(reason),
                EmbeddingSelection::Verified { .. } => None,
            };
            return ProjectDiscoveryResult {
                query: query.to_owned(),
                keywords,
                mode,
                degraded_reason,
                hits: Vec::new(),
            };
        }

        let (resource, runner) = match embedding {
            EmbeddingSelection::Unavailable(reason) => {
                return degraded(query, keywords, baseline, limit, reason);
            }
            EmbeddingSelection::Verified { resource, runner } => (resource, runner),
        };
        let request = EmbeddingRequest {
            schema_version: SchemaVersion,
            query: query.to_owned(),
            catalog_fingerprint: self.catalog.fingerprint.clone(),
            vector_fingerprint: resource.manifest.vector_fingerprint.clone(),
            candidates: baseline
                .iter()
                .map(|hit| EmbeddingCandidate {
                    id: hit.project.id.clone(),
                    text: hit.project.document().search_text(),
                })
                .collect(),
        };
        let output = match runner.embed(resource, &request).await {
            Ok(output) => output,
            Err(reason) => return degraded(query, keywords, baseline, limit, reason),
        };
        let semantic = match validate_output(resource, &request, &output) {
            Ok(semantic) => semantic,
            Err(reason) => return degraded(query, keywords, baseline, limit, reason),
        };
        let lexical_ids = baseline
            .iter()
            .map(|hit| hit.project.id.clone())
            .collect::<Vec<_>>();
        let semantic_ids = semantic
            .iter()
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        let fused = reciprocal_rank_fusion(&[lexical_ids, semantic_ids.clone()], 60);
        let semantic_ranks = semantic_ids
            .iter()
            .enumerate()
            .map(|(rank, id)| (id.as_str(), rank + 1))
            .collect::<BTreeMap<_, _>>();
        let mut baseline_by_id = baseline
            .into_iter()
            .map(|hit| (hit.project.id.clone(), hit))
            .collect::<BTreeMap<_, _>>();
        let hits = fused
            .into_iter()
            .filter_map(|ranked| {
                baseline_by_id.remove(&ranked.id).map(|mut hit| {
                    hit.semantic_rank = semantic_ranks.get(ranked.id.as_str()).copied();
                    hit.fused_score = Some(ranked.score);
                    hit
                })
            })
            .take(limit)
            .collect();
        ProjectDiscoveryResult {
            query: query.to_owned(),
            keywords,
            mode: RetrievalMode::HybridVerified,
            degraded_reason: None,
            hits,
        }
    }
}

fn degraded(
    query: &str,
    keywords: Vec<String>,
    baseline: Vec<ProjectDiscoveryHit>,
    limit: usize,
    reason: RetrievalDegradedReason,
) -> ProjectDiscoveryResult {
    ProjectDiscoveryResult {
        query: query.to_owned(),
        keywords,
        mode: RetrievalMode::Bm25,
        degraded_reason: Some(reason),
        hits: baseline.into_iter().take(limit).collect(),
    }
}

fn validate_output(
    resource: &VerifiedEmbeddingResource,
    request: &EmbeddingRequest,
    output: &EmbeddingOutput,
) -> Result<Vec<(String, f64)>, RetrievalDegradedReason> {
    if output.model_id != resource.manifest.model_id
        || output.runtime_abi != resource.manifest.runtime_abi
        || output.catalog_fingerprint != request.catalog_fingerprint
        || output.vector_fingerprint != request.vector_fingerprint
    {
        return Err(RetrievalDegradedReason::FingerprintMismatch);
    }
    if output.dimensions != resource.manifest.dimensions
        || output.query_vector.len() != output.dimensions
        || output
            .candidates
            .iter()
            .any(|candidate| candidate.vector.len() != output.dimensions)
    {
        return Err(RetrievalDegradedReason::WrongDimension);
    }
    if !finite(&output.query_vector)
        || output
            .candidates
            .iter()
            .any(|candidate| !finite(&candidate.vector))
    {
        return Err(RetrievalDegradedReason::NonFiniteVector);
    }
    let requested = request
        .candidates
        .iter()
        .map(|candidate| candidate.id.as_str())
        .collect::<BTreeSet<_>>();
    let returned = output
        .candidates
        .iter()
        .map(|candidate| candidate.id.as_str())
        .collect::<BTreeSet<_>>();
    if requested != returned || returned.len() != output.candidates.len() {
        return Err(RetrievalDegradedReason::MalformedVector);
    }
    let mut semantic = output
        .candidates
        .iter()
        .filter_map(|candidate: &CandidateVector| {
            cosine_similarity(&output.query_vector, &candidate.vector)
                .map(|score| (candidate.id.clone(), score))
        })
        .collect::<Vec<_>>();
    if semantic.len() != output.candidates.len() {
        return Err(RetrievalDegradedReason::MalformedVector);
    }
    semantic.sort_by(|left, right| {
        right
            .1
            .total_cmp(&left.1)
            .then_with(|| left.0.cmp(&right.0))
    });
    Ok(semantic)
}

fn finite(vector: &[f32]) -> bool {
    !vector.is_empty() && vector.iter().all(|value| value.is_finite())
}
