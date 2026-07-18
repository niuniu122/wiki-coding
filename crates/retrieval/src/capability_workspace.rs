use std::collections::{BTreeMap, BTreeSet};

use minimax_protocol::{
    CapabilityKind, CapabilityReadiness, RetrievalDegradedReason, RetrievalMode, SchemaVersion,
};
use serde::{Deserialize, Serialize};

use crate::catalog::hash_json;
use crate::discovery::validate_embedding_output;
use crate::{
    EmbeddingCandidate, EmbeddingRequest, EmbeddingSelection, McpDocument, McpIndex,
    ProjectDocument, ProjectIndex, SearchDocument, SkillDocument, SkillIndex,
    reciprocal_rank_fusion,
};

const MAX_CARDS_PER_CATALOG: usize = 10_000;
const MAX_TEXT_BYTES: usize = 8 * 1024;
const BM25_CANDIDATE_LIMIT: usize = 20;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityInstallKind {
    Bundled,
    External,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityMaintenance {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recent_commits: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_issue_triage: Option<bool>,
}

impl CapabilityMaintenance {
    fn is_unknown(&self) -> bool {
        self.archived.is_none()
            && self.recent_commits.is_none()
            && self.active_issue_triage.is_none()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityCard {
    pub id: String,
    pub kind: CapabilityKind,
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub description: String,
    pub intents: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platforms: Option<Vec<String>>,
    pub source_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    pub install_kind: CapabilityInstallKind,
    pub install_guidance: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authorizations: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permissions: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "CapabilityMaintenance::is_unknown")]
    pub maintenance: CapabilityMaintenance,
}

impl CapabilityCard {
    fn search_text(&self) -> String {
        std::iter::once(self.name.as_str())
            .chain(std::iter::once(self.description.as_str()))
            .chain(self.aliases.iter().map(String::as_str))
            .chain(self.intents.iter().map(String::as_str))
            .chain(self.platforms.iter().flatten().map(String::as_str))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn project_document(&self) -> ProjectDocument {
        ProjectDocument {
            id: self.id.clone(),
            name: self.name.clone(),
            aliases: self.aliases.clone(),
            description: self.description.clone(),
            topics: self.intents.clone(),
            platforms: self.platforms.clone().unwrap_or_default(),
        }
    }

    fn skill_document(&self) -> SkillDocument {
        SkillDocument {
            id: self.id.clone(),
            name: self.name.clone(),
            aliases: self.aliases.clone(),
            description: self.description.clone(),
            intents: self.intents.clone(),
            platforms: self.platforms.clone().unwrap_or_default(),
        }
    }

    fn mcp_document(&self) -> McpDocument {
        McpDocument {
            id: self.id.clone(),
            name: self.name.clone(),
            aliases: self.aliases.clone(),
            description: self.description.clone(),
            intents: self.intents.clone(),
            platforms: self.platforms.clone().unwrap_or_default(),
        }
    }

    #[must_use]
    pub fn confidence_penalty(&self) -> u8 {
        u8::from(self.license.is_none())
            + u8::from(self.platforms.is_none())
            + u8::from(self.permissions.is_none())
            + u8::from(self.maintenance.is_unknown())
    }

    #[must_use]
    pub fn maintenance_facts(&self) -> Vec<String> {
        let mut facts = Vec::new();
        if let Some(value) = self.maintenance.archived {
            facts.push(format!("archived={value}"));
        }
        if let Some(value) = self.maintenance.recent_commits {
            facts.push(format!("recent_commits={value}"));
        }
        if let Some(value) = self.maintenance.active_issue_triage {
            facts.push(format!("active_issue_triage={value}"));
        }
        facts
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityCatalog {
    pub schema_version: SchemaVersion,
    pub kind: CapabilityKind,
    pub source_url: String,
    pub generated_at: String,
    pub fingerprint: String,
    pub cards: Vec<CapabilityCard>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapabilityCatalogError {
    InvalidJson,
    InvalidSource,
    InvalidFingerprint,
    TooManyCards,
    InvalidCard,
    DuplicateCard,
    InvalidUrl,
    InvalidFact,
    KindMismatch,
    UnsafeGuidance,
    InvalidInventory,
}

impl std::fmt::Display for CapabilityCatalogError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let code = match self {
            Self::InvalidJson => "invalid_json",
            Self::InvalidSource => "invalid_source",
            Self::InvalidFingerprint => "invalid_fingerprint",
            Self::TooManyCards => "too_many_cards",
            Self::InvalidCard => "invalid_card",
            Self::DuplicateCard => "duplicate_card",
            Self::InvalidUrl => "invalid_url",
            Self::InvalidFact => "invalid_fact",
            Self::KindMismatch => "kind_mismatch",
            Self::UnsafeGuidance => "unsafe_guidance",
            Self::InvalidInventory => "invalid_inventory",
        };
        formatter.write_str(code)
    }
}

impl std::error::Error for CapabilityCatalogError {}

impl CapabilityCatalog {
    #[must_use]
    pub fn fingerprint_for_cards(cards: &[CapabilityCard]) -> String {
        format!("sha256:{}", hash_json(cards))
    }

    pub fn from_slice(bytes: &[u8]) -> Result<Self, CapabilityCatalogError> {
        let catalog: Self =
            serde_json::from_slice(bytes).map_err(|_| CapabilityCatalogError::InvalidJson)?;
        catalog.validate()
    }

    fn validate(self) -> Result<Self, CapabilityCatalogError> {
        if !valid_https_url(&self.source_url) || !valid_fact(&self.generated_at) {
            return Err(CapabilityCatalogError::InvalidSource);
        }
        if self.cards.len() > MAX_CARDS_PER_CATALOG {
            return Err(CapabilityCatalogError::TooManyCards);
        }
        let mut ids = BTreeSet::new();
        for card in &self.cards {
            if card.kind != self.kind {
                return Err(CapabilityCatalogError::KindMismatch);
            }
            if !valid_card_id(card.kind, &card.id)
                || !valid_fact(&card.name)
                || !valid_fact(&card.description)
                || card.intents.is_empty()
                || card.aliases.len() > 32
                || card.intents.len() > 64
                || card
                    .platforms
                    .as_ref()
                    .is_some_and(|values| values.len() > 32)
                || card
                    .permissions
                    .as_ref()
                    .is_some_and(|values| values.len() > 64)
                || card
                    .authorizations
                    .as_ref()
                    .is_some_and(|values| values.len() > 32)
            {
                return Err(CapabilityCatalogError::InvalidCard);
            }
            if !ids.insert(card.id.as_str()) {
                return Err(CapabilityCatalogError::DuplicateCard);
            }
            if !valid_https_url(&card.source_url)
                || card
                    .repository_url
                    .as_deref()
                    .is_some_and(|url| !valid_https_url(url))
            {
                return Err(CapabilityCatalogError::InvalidUrl);
            }
            if !safe_guidance(&card.install_guidance) {
                return Err(CapabilityCatalogError::UnsafeGuidance);
            }
            if card
                .aliases
                .iter()
                .chain(&card.intents)
                .any(|value| !valid_fact(value))
                || card
                    .platforms
                    .iter()
                    .flatten()
                    .any(|value| !valid_fact(value))
                || card
                    .license
                    .as_deref()
                    .is_some_and(|value| !valid_fact(value))
                || card
                    .permissions
                    .iter()
                    .flatten()
                    .chain(card.authorizations.iter().flatten())
                    .any(|value| !valid_token(value))
            {
                return Err(CapabilityCatalogError::InvalidFact);
            }
        }
        if self.fingerprint != Self::fingerprint_for_cards(&self.cards) {
            return Err(CapabilityCatalogError::InvalidFingerprint);
        }
        Ok(self)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapabilityWorkspaceCatalog {
    projects: CapabilityCatalog,
    skills: CapabilityCatalog,
    mcp: CapabilityCatalog,
    fingerprint: String,
}

impl CapabilityWorkspaceCatalog {
    pub fn from_slices(
        projects: &[u8],
        skills: &[u8],
        mcp: &[u8],
    ) -> Result<Self, CapabilityCatalogError> {
        let projects = CapabilityCatalog::from_slice(projects)?;
        let skills = CapabilityCatalog::from_slice(skills)?;
        let mcp = CapabilityCatalog::from_slice(mcp)?;
        if projects.kind != CapabilityKind::Project
            || skills.kind != CapabilityKind::Skill
            || mcp.kind != CapabilityKind::Mcp
        {
            return Err(CapabilityCatalogError::KindMismatch);
        }
        let mut ids = BTreeSet::new();
        for card in projects.cards.iter().chain(&skills.cards).chain(&mcp.cards) {
            if !ids.insert(card.id.as_str()) {
                return Err(CapabilityCatalogError::DuplicateCard);
            }
        }
        let fingerprint = format!(
            "sha256:{}",
            hash_json(&[
                projects.fingerprint.as_str(),
                skills.fingerprint.as_str(),
                mcp.fingerprint.as_str(),
            ])
        );
        Ok(Self {
            projects,
            skills,
            mcp,
            fingerprint,
        })
    }

    #[must_use]
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    #[must_use]
    pub fn catalog(&self, kind: CapabilityKind) -> &CapabilityCatalog {
        match kind {
            CapabilityKind::Project => &self.projects,
            CapabilityKind::Skill => &self.skills,
            CapabilityKind::Mcp => &self.mcp,
        }
    }

    fn cards(&self) -> impl Iterator<Item = &CapabilityCard> {
        self.projects
            .cards
            .iter()
            .chain(&self.skills.cards)
            .chain(&self.mcp.cards)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CapabilityInventory {
    installed: BTreeSet<String>,
    authorized: BTreeSet<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CapabilityInventoryFile {
    schema_version: SchemaVersion,
    #[serde(default)]
    installed: Vec<String>,
    #[serde(default)]
    authorized: Vec<String>,
}

impl CapabilityInventory {
    pub fn from_slice(bytes: &[u8]) -> Result<Self, CapabilityCatalogError> {
        let file: CapabilityInventoryFile =
            serde_json::from_slice(bytes).map_err(|_| CapabilityCatalogError::InvalidInventory)?;
        let _ = file.schema_version;
        Self::new(file.installed, file.authorized)
    }

    pub fn new(
        installed: impl IntoIterator<Item = String>,
        authorized: impl IntoIterator<Item = String>,
    ) -> Result<Self, CapabilityCatalogError> {
        let installed = collect_inventory_ids(installed)?;
        let authorized = collect_inventory_ids(authorized)?;
        Ok(Self {
            installed,
            authorized,
        })
    }

    #[must_use]
    pub fn readiness(&self, card: &CapabilityCard) -> (CapabilityReadiness, String, String) {
        let installed = card.install_kind == CapabilityInstallKind::Bundled
            || self.installed.contains(&card.id);
        if !installed {
            return (
                CapabilityReadiness::NeedsInstall,
                format!("{} is not installed in the local capability runtime.", card.name),
                "Review the source and installation guidance, then confirm installation in a separate workflow.".to_owned(),
            );
        }
        let requires_authorization = card
            .authorizations
            .as_ref()
            .is_some_and(|values| !values.is_empty());
        if requires_authorization && !self.authorized.contains(&card.id) {
            return (
                CapabilityReadiness::NeedsAccess,
                format!(
                    "{} is installed but its declared authorization is not present.",
                    card.name
                ),
                "Review the requested access and grant authorization in a separate workflow."
                    .to_owned(),
            );
        }
        (
            CapabilityReadiness::Ready,
            format!(
                "{} is installed and has no unmet declared authorization.",
                card.name
            ),
            "Review the capability and explicitly confirm use in the calling workflow.".to_owned(),
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CapabilityWorkspaceHit {
    pub card: CapabilityCard,
    pub readiness: CapabilityReadiness,
    pub readiness_reason: String,
    pub next_action: String,
    pub matched_terms: Vec<String>,
    pub lexical_rank: usize,
    pub semantic_rank: Option<usize>,
    pub lexical_score: f64,
    pub fused_score: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CapabilityWorkspaceResult {
    pub query: String,
    pub selected_kind: Option<CapabilityKind>,
    pub keywords: Vec<String>,
    pub mode: RetrievalMode,
    pub degraded_reason: Option<RetrievalDegradedReason>,
    pub hits: Vec<CapabilityWorkspaceHit>,
}

#[derive(Clone, Debug)]
pub struct CapabilityWorkspace {
    catalogs: CapabilityWorkspaceCatalog,
    projects: ProjectIndex,
    skills: SkillIndex,
    mcp: McpIndex,
    by_id: BTreeMap<String, CapabilityCard>,
}

#[derive(Clone, Debug)]
struct RecalledCard {
    card: CapabilityCard,
    matched_terms: Vec<String>,
    kind_rank: usize,
    lexical_score: f64,
    mode: RetrievalMode,
}

impl CapabilityWorkspace {
    #[must_use]
    pub fn new(catalogs: CapabilityWorkspaceCatalog) -> Self {
        let projects = ProjectIndex::new(
            catalogs
                .projects
                .cards
                .iter()
                .map(CapabilityCard::project_document)
                .collect(),
        );
        let skills = SkillIndex::new(
            catalogs
                .skills
                .cards
                .iter()
                .map(CapabilityCard::skill_document)
                .collect(),
        );
        let mcp = McpIndex::new(
            catalogs
                .mcp
                .cards
                .iter()
                .map(CapabilityCard::mcp_document)
                .collect(),
        );
        let by_id = catalogs
            .cards()
            .map(|card| (card.id.clone(), card.clone()))
            .collect();
        Self {
            catalogs,
            projects,
            skills,
            mcp,
            by_id,
        }
    }

    #[must_use]
    pub fn catalogs(&self) -> &CapabilityWorkspaceCatalog {
        &self.catalogs
    }

    #[must_use]
    pub fn fingerprint(&self) -> &str {
        self.catalogs.fingerprint()
    }

    pub fn validate_inventory(
        &self,
        inventory: &CapabilityInventory,
    ) -> Result<(), CapabilityCatalogError> {
        if inventory
            .installed
            .iter()
            .chain(&inventory.authorized)
            .any(|id| !self.by_id.contains_key(id))
        {
            return Err(CapabilityCatalogError::InvalidInventory);
        }
        Ok(())
    }

    pub async fn discover(
        &self,
        query: &str,
        selected_kind: Option<CapabilityKind>,
        limit: usize,
        inventory: &CapabilityInventory,
        embedding: EmbeddingSelection<'_>,
    ) -> CapabilityWorkspaceResult {
        let mut recalled = self.recall(query, selected_kind);
        let exact = recalled
            .iter()
            .any(|candidate| candidate.mode == RetrievalMode::Exact);
        if exact {
            recalled.retain(|candidate| candidate.mode == RetrievalMode::Exact);
        }
        recalled.sort_by(|left, right| {
            if exact {
                left.card
                    .kind
                    .cmp(&right.card.kind)
                    .then_with(|| left.card.id.cmp(&right.card.id))
            } else {
                right
                    .lexical_score
                    .total_cmp(&left.lexical_score)
                    .then_with(|| left.kind_rank.cmp(&right.kind_rank))
                    .then_with(|| left.card.kind.cmp(&right.card.kind))
                    .then_with(|| left.card.id.cmp(&right.card.id))
            }
        });
        recalled.truncate(BM25_CANDIDATE_LIMIT);
        let keywords = recalled
            .iter()
            .flat_map(|candidate| candidate.matched_terms.iter())
            .filter(|term| term.chars().count() > 1)
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .take(8)
            .collect::<Vec<_>>();
        let baseline = recalled
            .into_iter()
            .enumerate()
            .map(|(rank, candidate)| {
                let (readiness, readiness_reason, next_action) =
                    inventory.readiness(&candidate.card);
                CapabilityWorkspaceHit {
                    card: candidate.card,
                    readiness,
                    readiness_reason,
                    next_action,
                    matched_terms: candidate.matched_terms,
                    lexical_rank: rank + 1,
                    semantic_rank: None,
                    lexical_score: candidate.lexical_score,
                    fused_score: None,
                }
            })
            .collect::<Vec<_>>();
        if exact {
            return result(
                query,
                selected_kind,
                keywords,
                RetrievalMode::Exact,
                None,
                baseline,
                limit,
            );
        }
        if baseline.is_empty() {
            let degraded_reason = match embedding {
                EmbeddingSelection::Unavailable(reason) => Some(reason),
                EmbeddingSelection::Verified { .. } => None,
            };
            return result(
                query,
                selected_kind,
                keywords,
                RetrievalMode::Bm25,
                degraded_reason,
                baseline,
                limit,
            );
        }
        let (resource, runner) = match embedding {
            EmbeddingSelection::Unavailable(reason) => {
                return result(
                    query,
                    selected_kind,
                    keywords,
                    RetrievalMode::Bm25,
                    Some(reason),
                    baseline,
                    limit,
                );
            }
            EmbeddingSelection::Verified { resource, runner } => (resource, runner),
        };
        if resource.manifest.catalog_fingerprint != self.fingerprint() {
            return result(
                query,
                selected_kind,
                keywords,
                RetrievalMode::Bm25,
                Some(RetrievalDegradedReason::FingerprintMismatch),
                baseline,
                limit,
            );
        }
        let request = EmbeddingRequest {
            schema_version: SchemaVersion,
            query: query.to_owned(),
            catalog_fingerprint: self.fingerprint().to_owned(),
            vector_fingerprint: resource.manifest.vector_fingerprint.clone(),
            candidates: baseline
                .iter()
                .map(|hit| EmbeddingCandidate {
                    id: hit.card.id.clone(),
                    text: hit.card.search_text(),
                })
                .collect(),
        };
        let output = match runner.embed(resource, &request).await {
            Ok(output) => output,
            Err(reason) => {
                return result(
                    query,
                    selected_kind,
                    keywords,
                    RetrievalMode::Bm25,
                    Some(reason),
                    baseline,
                    limit,
                );
            }
        };
        let semantic = match validate_embedding_output(resource, &request, &output) {
            Ok(semantic) => semantic,
            Err(reason) => {
                return result(
                    query,
                    selected_kind,
                    keywords,
                    RetrievalMode::Bm25,
                    Some(reason),
                    baseline,
                    limit,
                );
            }
        };
        let lexical_ids = baseline
            .iter()
            .map(|hit| hit.card.id.clone())
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
            .map(|hit| (hit.card.id.clone(), hit))
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
            .collect();
        result(
            query,
            selected_kind,
            keywords,
            RetrievalMode::HybridVerified,
            None,
            hits,
            limit,
        )
    }

    fn recall(&self, query: &str, selected_kind: Option<CapabilityKind>) -> Vec<RecalledCard> {
        let mut recalled = Vec::new();
        if selected_kind.is_none() || selected_kind == Some(CapabilityKind::Project) {
            recalled.extend(
                self.projects
                    .search(query, BM25_CANDIDATE_LIMIT)
                    .into_iter()
                    .enumerate()
                    .filter_map(|(rank, hit)| {
                        self.recalled(
                            hit.document.id(),
                            hit.mode,
                            hit.score,
                            rank,
                            hit.contributions
                                .into_iter()
                                .map(|item| item.term)
                                .collect(),
                        )
                    }),
            );
        }
        if selected_kind.is_none() || selected_kind == Some(CapabilityKind::Skill) {
            recalled.extend(
                self.skills
                    .search(query, BM25_CANDIDATE_LIMIT)
                    .into_iter()
                    .enumerate()
                    .filter_map(|(rank, hit)| {
                        self.recalled(
                            hit.document.id(),
                            hit.mode,
                            hit.score,
                            rank,
                            hit.contributions
                                .into_iter()
                                .map(|item| item.term)
                                .collect(),
                        )
                    }),
            );
        }
        if selected_kind.is_none() || selected_kind == Some(CapabilityKind::Mcp) {
            recalled.extend(
                self.mcp
                    .search(query, BM25_CANDIDATE_LIMIT)
                    .into_iter()
                    .enumerate()
                    .filter_map(|(rank, hit)| {
                        self.recalled(
                            hit.document.id(),
                            hit.mode,
                            hit.score,
                            rank,
                            hit.contributions
                                .into_iter()
                                .map(|item| item.term)
                                .collect(),
                        )
                    }),
            );
        }
        recalled
    }

    fn recalled(
        &self,
        id: &str,
        mode: RetrievalMode,
        raw_score: f64,
        zero_based_rank: usize,
        matched_terms: Vec<String>,
    ) -> Option<RecalledCard> {
        let card = self.by_id.get(id)?.clone();
        let lexical_score = if mode == RetrievalMode::Exact {
            1.0
        } else {
            let rank_score = 1.0 / (60 + zero_based_rank + 1) as f64;
            rank_score + raw_score.max(0.0) * 1e-9
        };
        Some(RecalledCard {
            card,
            matched_terms,
            kind_rank: zero_based_rank + 1,
            lexical_score,
            mode,
        })
    }
}

fn result(
    query: &str,
    selected_kind: Option<CapabilityKind>,
    keywords: Vec<String>,
    mode: RetrievalMode,
    degraded_reason: Option<RetrievalDegradedReason>,
    hits: Vec<CapabilityWorkspaceHit>,
    limit: usize,
) -> CapabilityWorkspaceResult {
    CapabilityWorkspaceResult {
        query: query.to_owned(),
        selected_kind,
        keywords,
        mode,
        degraded_reason,
        hits: hits.into_iter().take(limit).collect(),
    }
}

fn collect_inventory_ids(
    values: impl IntoIterator<Item = String>,
) -> Result<BTreeSet<String>, CapabilityCatalogError> {
    let values = values.into_iter().collect::<Vec<_>>();
    if values.len() > MAX_CARDS_PER_CATALOG || values.iter().any(|value| !valid_any_card_id(value))
    {
        return Err(CapabilityCatalogError::InvalidInventory);
    }
    let unique = values.iter().collect::<BTreeSet<_>>();
    if unique.len() != values.len() {
        return Err(CapabilityCatalogError::InvalidInventory);
    }
    Ok(values.into_iter().collect())
}

fn valid_any_card_id(value: &str) -> bool {
    [
        CapabilityKind::Project,
        CapabilityKind::Skill,
        CapabilityKind::Mcp,
    ]
    .into_iter()
    .any(|kind| valid_card_id(kind, value))
}

fn valid_card_id(kind: CapabilityKind, value: &str) -> bool {
    value.strip_prefix(kind.id_prefix()).is_some_and(|suffix| {
        !suffix.is_empty()
            && suffix.len() <= 128
            && suffix.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'-' | b'_' | b'.' | b'/')
            })
    })
}

fn valid_token(value: &str) -> bool {
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

fn safe_guidance(value: &str) -> bool {
    if !valid_fact(value) {
        return false;
    }
    let lower = value.to_ascii_lowercase();
    ![
        "npm install",
        "cargo install",
        "pip install",
        "curl ",
        "wget ",
        "powershell",
        "cmd.exe",
        "bash ",
        "sh ",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
        && !value.contains([';', '|', '&', '`'])
        && !value.contains("$(")
}
