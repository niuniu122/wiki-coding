use std::collections::BTreeSet;
use std::fmt::{self, Write as _};
use std::fs;
use std::future::Future;
use std::path::{Component, Path, PathBuf};
use std::pin::Pin;
use std::sync::Mutex;

use minimax_protocol::{CapabilityKind, RetrievalDegradedReason, RetrievalMode, SchemaVersion};
use minimax_retrieval::{
    CandidateVector, CapabilityDocument, CapabilityIndex, CapabilityInventory, CapabilityWorkspace,
    CapabilityWorkspaceCatalog, EMBEDDING_HELPER_ABI, EmbeddingOutput, EmbeddingRequest,
    EmbeddingResourceManifest, EmbeddingRunner, EmbeddingSelection, GRANITE_EMBEDDING_MODEL_ID,
    GRANITE_RESOURCE_PACKAGE_ID, ProjectCatalog, ProjectDiscovery, VerifiedEmbeddingResource,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub const RETRIEVAL_EVALUATION_MANIFEST: &str = "fixtures/compat/evaluations/retrieval.v1.json";
pub const RETRIEVAL_EVALUATION_GOLDEN: &str =
    "fixtures/compat/evaluations/retrieval-report.expected.json";

const EVALUATION_ID: &str = "retrieval-conformance-v1";
const REQUIRED_DEGRADATIONS: [&str; 6] = [
    "no_resource",
    "damaged_resource",
    "runner_failure",
    "malformed_response",
    "timeout",
    "outsider",
];

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RetrievalEvaluationManifest {
    schema_version: u16,
    evaluation_id: String,
    expanded_corpus: FixtureReference,
    project_catalog: FixtureReference,
    workspace_cases: FixtureReference,
    workspace_catalogs: Vec<FixtureReference>,
    project_cases: Vec<ProjectCase>,
    candidate_query: String,
    degradation_query: String,
    candidate_limit: usize,
    required_degradations: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct FixtureReference {
    path: String,
    sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectCase {
    id: String,
    query: String,
    expected_top: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RetrievalEvaluationReport {
    pub schema_version: u16,
    pub evaluation_id: String,
    pub fixture_fingerprint: String,
    pub thresholds: RetrievalThresholds,
    pub corpus: CorpusReport,
    pub projects: ProjectReport,
    pub workspace: WorkspaceReport,
    pub candidate_boundary: CandidateBoundaryReport,
    pub degradations: Vec<DegradationReport>,
    pub disabled_path: DisabledPathReport,
    pub passed: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RetrievalThresholds {
    pub minimum_cases: usize,
    pub recall_at5: f64,
    pub top1: f64,
    pub mrr: f64,
    pub no_match_precision: f64,
    pub id_validity: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RetrievalMetrics {
    pub cases: usize,
    pub recall_at5: f64,
    pub top1: f64,
    pub mrr: f64,
    pub no_match_precision: f64,
    pub id_validity: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CorpusReport {
    pub id: String,
    pub fingerprint: String,
    pub cases: usize,
    pub positive_cases: usize,
    pub no_match_cases: usize,
    pub chinese_cases: usize,
    pub latin_cases: usize,
    pub exact_cases: usize,
    pub bm25_cases: usize,
    pub metrics: RetrievalMetrics,
    pub exact_metrics: RetrievalMetrics,
    pub bm25_metrics: RetrievalMetrics,
    pub passed: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectReport {
    pub cases: usize,
    pub passed_cases: usize,
    pub exact_cases: usize,
    pub bm25_cases: usize,
    pub hybrid_cases: usize,
    pub baseline_top1: f64,
    pub hybrid_top1: f64,
    pub passed: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceReport {
    pub cases: usize,
    pub passed_cases: usize,
    pub exact_cases: usize,
    pub bm25_cases: usize,
    pub hybrid_cases: usize,
    pub kind_isolation: bool,
    pub no_match_preserved: bool,
    pub passed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateBoundaryReport {
    pub query: String,
    pub lexical_candidate_ids: Vec<String>,
    pub observed_candidate_ids: Vec<String>,
    pub semantic_candidate_ids: Vec<String>,
    pub hybrid_result_ids: Vec<String>,
    pub outsider_attempted_id: String,
    pub outsider_result_ids: Vec<String>,
    pub outsider_rejected: bool,
    pub passed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DegradationReport {
    pub scenario: String,
    pub reason: RetrievalDegradedReason,
    pub mode: RetrievalMode,
    pub result_ids: Vec<String>,
    pub bm25_ids_preserved: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisabledPathReport {
    pub network_requests: u8,
    pub provider_requests: u8,
    pub model_downloads: u8,
    pub model_loads: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RetrievalEvaluationError {
    ManifestRead,
    ManifestParse(String),
    InvalidManifest(String),
    FixtureRead(String),
    FixtureFingerprint(String),
    FixtureParse(String),
    InvalidCorpus(String),
    Runtime,
    ReportSerialization,
    EvaluationFailed,
    GoldenRead,
    GoldenDrift,
}

impl fmt::Display for RetrievalEvaluationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ManifestRead => formatter.write_str("cannot read retrieval evaluation manifest"),
            Self::ManifestParse(message) => {
                write!(
                    formatter,
                    "invalid retrieval evaluation manifest: {message}"
                )
            }
            Self::InvalidManifest(message) => {
                write!(
                    formatter,
                    "invalid retrieval evaluation manifest: {message}"
                )
            }
            Self::FixtureRead(path) => write!(formatter, "cannot read retrieval fixture: {path}"),
            Self::FixtureFingerprint(path) => {
                write!(formatter, "retrieval fixture fingerprint mismatch: {path}")
            }
            Self::FixtureParse(path) => write!(formatter, "invalid retrieval fixture: {path}"),
            Self::InvalidCorpus(message) => {
                write!(formatter, "invalid retrieval corpus: {message}")
            }
            Self::Runtime => formatter.write_str("cannot start retrieval evaluation runtime"),
            Self::ReportSerialization => {
                formatter.write_str("cannot serialize retrieval evaluation report")
            }
            Self::EvaluationFailed => formatter.write_str("retrieval evaluation failed"),
            Self::GoldenRead => formatter.write_str("cannot read retrieval evaluation golden"),
            Self::GoldenDrift => formatter.write_str("retrieval evaluation golden drift"),
        }
    }
}

impl std::error::Error for RetrievalEvaluationError {}

pub fn run_retrieval_evaluation(
    root: &Path,
) -> Result<RetrievalEvaluationReport, RetrievalEvaluationError> {
    let manifest = load_manifest(root)?;
    validate_manifest(&manifest)?;
    let corpus_bytes = read_fingerprinted(root, &manifest.expanded_corpus)?;
    let project_bytes = read_fingerprinted(root, &manifest.project_catalog)?;
    let workspace_case_bytes = read_fingerprinted(root, &manifest.workspace_cases)?;
    let workspace_catalog_bytes = manifest
        .workspace_catalogs
        .iter()
        .map(|reference| read_fingerprinted(root, reference))
        .collect::<Result<Vec<_>, _>>()?;
    let corpus = load_corpus(&corpus_bytes, &manifest.expanded_corpus.path)?;
    let project_catalog = ProjectCatalog::from_slice(&project_bytes).map_err(|_| {
        RetrievalEvaluationError::FixtureParse(manifest.project_catalog.path.clone())
    })?;
    let workspace_cases: WorkspaceCases =
        serde_json::from_slice(&workspace_case_bytes).map_err(|_| {
            RetrievalEvaluationError::FixtureParse(manifest.workspace_cases.path.clone())
        })?;
    validate_workspace_cases(&workspace_cases)?;
    let workspace_catalogs = CapabilityWorkspaceCatalog::from_slices(
        &workspace_catalog_bytes[0],
        &workspace_catalog_bytes[1],
        &workspace_catalog_bytes[2],
    )
    .map_err(|_| {
        RetrievalEvaluationError::FixtureParse("capabilities/catalogs/*.v1.json".into())
    })?;

    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .map_err(|_| RetrievalEvaluationError::Runtime)?;
    let (corpus_report, thresholds) = evaluate_corpus(&corpus)?;
    let discovery = ProjectDiscovery::new(project_catalog);
    let projects = runtime.block_on(evaluate_projects(&discovery, &manifest));
    let workspace = CapabilityWorkspace::new(workspace_catalogs);
    let workspace_report = runtime.block_on(evaluate_workspace(&workspace, &workspace_cases));
    let candidate_boundary = runtime.block_on(evaluate_candidate_boundary(
        &discovery,
        &manifest.candidate_query,
        manifest.candidate_limit,
    ));
    let degradations = runtime.block_on(evaluate_degradations(
        &discovery,
        &manifest.degradation_query,
        manifest.candidate_limit,
    ));
    let degradations_passed = degradations.len() == REQUIRED_DEGRADATIONS.len()
        && degradations
            .iter()
            .all(|scenario| scenario.mode == RetrievalMode::Bm25 && scenario.bm25_ids_preserved);
    let disabled_path = DisabledPathReport {
        network_requests: 0,
        provider_requests: 0,
        model_downloads: 0,
        model_loads: 0,
    };
    let passed = corpus_report.passed
        && projects.passed
        && workspace_report.passed
        && candidate_boundary.passed
        && degradations_passed;
    Ok(RetrievalEvaluationReport {
        schema_version: 1,
        evaluation_id: manifest.evaluation_id.clone(),
        fixture_fingerprint: manifest_fingerprint(&manifest)?,
        thresholds,
        corpus: corpus_report,
        projects,
        workspace: workspace_report,
        candidate_boundary,
        degradations,
        disabled_path,
        passed,
    })
}

pub fn retrieval_report_json(
    report: &RetrievalEvaluationReport,
) -> Result<String, RetrievalEvaluationError> {
    let mut output = serde_json::to_string_pretty(report)
        .map_err(|_| RetrievalEvaluationError::ReportSerialization)?;
    output.push('\n');
    Ok(output)
}

pub fn verify_retrieval_evaluation(
    root: &Path,
) -> Result<RetrievalEvaluationReport, RetrievalEvaluationError> {
    let report = run_retrieval_evaluation(root)?;
    if !report.passed {
        return Err(RetrievalEvaluationError::EvaluationFailed);
    }
    let actual = retrieval_report_json(&report)?;
    let expected = fs::read_to_string(root.join(RETRIEVAL_EVALUATION_GOLDEN))
        .map_err(|_| RetrievalEvaluationError::GoldenRead)?;
    if actual != normalize_newline(&expected) {
        return Err(RetrievalEvaluationError::GoldenDrift);
    }
    Ok(report)
}

fn load_manifest(root: &Path) -> Result<RetrievalEvaluationManifest, RetrievalEvaluationError> {
    let raw = fs::read_to_string(root.join(RETRIEVAL_EVALUATION_MANIFEST))
        .map_err(|_| RetrievalEvaluationError::ManifestRead)?;
    serde_json::from_str(&raw)
        .map_err(|error| RetrievalEvaluationError::ManifestParse(error.to_string()))
}

fn validate_manifest(
    manifest: &RetrievalEvaluationManifest,
) -> Result<(), RetrievalEvaluationError> {
    if manifest.schema_version != 1 || manifest.evaluation_id != EVALUATION_ID {
        return invalid_manifest("schemaVersion or evaluationId is not supported");
    }
    let references = std::iter::once(&manifest.expanded_corpus)
        .chain(std::iter::once(&manifest.project_catalog))
        .chain(std::iter::once(&manifest.workspace_cases))
        .chain(manifest.workspace_catalogs.iter())
        .collect::<Vec<_>>();
    if manifest.workspace_catalogs.len() != 3 {
        return invalid_manifest("exactly three workspace catalogs are required");
    }
    for reference in &references {
        validate_fixture_reference(reference)?;
    }
    if references
        .iter()
        .map(|reference| reference.path.as_str())
        .collect::<BTreeSet<_>>()
        .len()
        != references.len()
    {
        return invalid_manifest("retrieval fixture paths must be unique");
    }
    let expected_catalog_paths = [
        "capabilities/catalogs/projects.v1.json",
        "capabilities/catalogs/skills.v1.json",
        "capabilities/catalogs/mcp.v1.json",
    ];
    if manifest
        .workspace_catalogs
        .iter()
        .map(|reference| reference.path.as_str())
        .ne(expected_catalog_paths)
    {
        return invalid_manifest("workspace catalogs must use stable project, Skill, MCP order");
    }
    let mut case_ids = BTreeSet::new();
    if manifest.project_cases.is_empty()
        || manifest.project_cases.iter().any(|case| {
            case.id.is_empty()
                || case.query.trim().is_empty()
                || case.expected_top.is_empty()
                || !case_ids.insert(case.id.as_str())
        })
    {
        return invalid_manifest("project evaluation cases must be complete and duplicate-free");
    }
    if manifest.candidate_query.trim().is_empty()
        || manifest.degradation_query.trim().is_empty()
        || manifest.candidate_limit != 5
        || manifest
            .required_degradations
            .iter()
            .map(String::as_str)
            .ne(REQUIRED_DEGRADATIONS)
    {
        return invalid_manifest("candidate and degradation contract drifted");
    }
    Ok(())
}

fn validate_fixture_reference(
    reference: &FixtureReference,
) -> Result<(), RetrievalEvaluationError> {
    validate_relative_path(&reference.path)?;
    if reference.sha256.len() != 64
        || !reference
            .sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return invalid_manifest("fixture sha256 must be lowercase hexadecimal");
    }
    Ok(())
}

fn read_fingerprinted(
    root: &Path,
    reference: &FixtureReference,
) -> Result<Vec<u8>, RetrievalEvaluationError> {
    let bytes = fs::read(root.join(&reference.path))
        .map_err(|_| RetrievalEvaluationError::FixtureRead(reference.path.clone()))?;
    if sha256(&bytes) != reference.sha256 {
        return Err(RetrievalEvaluationError::FixtureFingerprint(
            reference.path.clone(),
        ));
    }
    Ok(bytes)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CorpusFixture {
    schema_version: u16,
    corpus_id: String,
    source: CorpusSource,
    corpus_fingerprint: String,
    thresholds: RetrievalThresholdFixture,
    descriptors: Vec<CorpusDescriptor>,
    case_groups: Vec<CorpusCaseGroup>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CorpusSource {
    path: String,
    sha256: String,
    retained_until: String,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RetrievalThresholdFixture {
    minimum_cases: usize,
    recall_at5: f64,
    top1: f64,
    mrr: f64,
    no_match_precision: f64,
    id_validity: f64,
}

impl From<&RetrievalThresholdFixture> for RetrievalThresholds {
    fn from(value: &RetrievalThresholdFixture) -> Self {
        Self {
            minimum_cases: value.minimum_cases,
            recall_at5: value.recall_at5,
            top1: value.top1,
            mrr: value.mrr,
            no_match_precision: value.no_match_precision,
            id_validity: value.id_validity,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CorpusDescriptor {
    schema_version: u16,
    id: String,
    name: String,
    description: String,
    aliases: Vec<String>,
    commands: Vec<String>,
    safety_class: String,
    idempotent: bool,
    execution: CorpusExecution,
    facets: CorpusFacets,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum CorpusExecution {
    WorkspaceRead { operation: String },
    NpmScript { script: String, argv: Vec<String> },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CorpusFacets {
    domain: Vec<String>,
    action: Vec<String>,
    object: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CorpusCaseGroup {
    id: String,
    expected_ids: Vec<String>,
    no_match: bool,
    query_ids: Vec<String>,
    queries: Vec<String>,
}

fn load_corpus(bytes: &[u8], path: &str) -> Result<CorpusFixture, RetrievalEvaluationError> {
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|_| RetrievalEvaluationError::FixtureParse(path.to_owned()))?;
    let corpus: CorpusFixture = serde_json::from_value(value.clone())
        .map_err(|_| RetrievalEvaluationError::FixtureParse(path.to_owned()))?;
    if corpus.schema_version != 1
        || corpus.corpus_id != "capability-retrieval-expanded-v1"
        || corpus.source.path.is_empty()
        || corpus.source.sha256.len() != 64
        || !corpus
            .source
            .sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || corpus.source.retained_until != "14-01"
    {
        return invalid_corpus("immutable source metadata drifted");
    }
    if corpus.thresholds.minimum_cases != 150
        || corpus.thresholds.recall_at5 != 0.95
        || corpus.thresholds.top1 != 0.85
        || corpus.thresholds.mrr != 0.9
        || corpus.thresholds.no_match_precision != 0.95
        || corpus.thresholds.id_validity != 1.0
    {
        return invalid_corpus("locked thresholds drifted");
    }
    let fingerprint_input = json!({
        "caseGroups": value["caseGroups"],
        "descriptors": value["descriptors"],
        "thresholds": value["thresholds"]
    });
    let fingerprint = sha256(
        &serde_json::to_vec(&fingerprint_input)
            .map_err(|_| RetrievalEvaluationError::FixtureParse(path.to_owned()))?,
    );
    if corpus.corpus_fingerprint != fingerprint {
        return invalid_corpus("corpus fingerprint mismatch");
    }

    let mut descriptor_ids = BTreeSet::new();
    for descriptor in &corpus.descriptors {
        if descriptor.schema_version != 1
            || descriptor.id.is_empty()
            || descriptor.safety_class.is_empty()
            || !descriptor_ids.insert(descriptor.id.as_str())
        {
            return invalid_corpus("invalid or duplicate descriptor");
        }
        match &descriptor.execution {
            CorpusExecution::WorkspaceRead { operation } if operation.is_empty() => {
                return invalid_corpus("empty workspace operation");
            }
            CorpusExecution::NpmScript { script, argv }
                if script.is_empty() || !argv.is_empty() =>
            {
                return invalid_corpus("invalid diagnostic declaration");
            }
            _ => {}
        }
        if !descriptor.idempotent
            && !matches!(&descriptor.execution, CorpusExecution::NpmScript { .. })
        {
            return invalid_corpus("invalid idempotence declaration");
        }
    }
    let mut group_ids = BTreeSet::new();
    let mut query_ids = BTreeSet::new();
    let mut count = 0usize;
    for group in &corpus.case_groups {
        if group.id.is_empty()
            || !group_ids.insert(group.id.as_str())
            || group.queries.len() != group.query_ids.len()
            || group.queries.iter().any(|query| query.trim().is_empty())
            || group.no_match != group.expected_ids.is_empty()
            || group
                .expected_ids
                .iter()
                .any(|id| !descriptor_ids.contains(id.as_str()))
        {
            return invalid_corpus("invalid case group");
        }
        for id in &group.query_ids {
            count += 1;
            if id != &format!("capability-case-{count:03}") || !query_ids.insert(id.as_str()) {
                return invalid_corpus("stable query IDs drifted");
            }
        }
    }
    if count != 175 || count < corpus.thresholds.minimum_cases {
        return invalid_corpus("case count drifted");
    }
    Ok(corpus)
}

#[derive(Default)]
struct MetricAccumulator {
    cases: usize,
    positives: usize,
    negatives: usize,
    recalled: usize,
    top1: usize,
    reciprocal_rank: f64,
    no_match_correct: usize,
    returned: usize,
    valid: usize,
}

impl MetricAccumulator {
    fn observe(
        &mut self,
        expected_ids: &[String],
        no_match: bool,
        result_ids: &[String],
        valid_ids: &BTreeSet<String>,
    ) {
        self.cases += 1;
        self.returned += result_ids.len();
        self.valid += result_ids
            .iter()
            .filter(|id| valid_ids.contains(*id))
            .count();
        if no_match {
            self.negatives += 1;
            self.no_match_correct += usize::from(result_ids.is_empty());
            return;
        }
        self.positives += 1;
        if let Some(rank) = result_ids.iter().position(|id| expected_ids.contains(id)) {
            self.recalled += 1;
            self.top1 += usize::from(rank == 0);
            self.reciprocal_rank += 1.0 / (rank + 1) as f64;
        }
    }

    fn finish(self) -> RetrievalMetrics {
        RetrievalMetrics {
            cases: self.cases,
            recall_at5: ratio(self.recalled, self.positives),
            top1: ratio(self.top1, self.positives),
            mrr: if self.positives == 0 {
                1.0
            } else {
                self.reciprocal_rank / self.positives as f64
            },
            no_match_precision: ratio(self.no_match_correct, self.negatives),
            id_validity: ratio(self.valid, self.returned),
        }
    }
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        1.0
    } else {
        numerator as f64 / denominator as f64
    }
}

fn evaluate_corpus(
    corpus: &CorpusFixture,
) -> Result<(CorpusReport, RetrievalThresholds), RetrievalEvaluationError> {
    let documents = corpus
        .descriptors
        .iter()
        .map(|descriptor| {
            let intent_document = std::iter::once(descriptor.name.as_str())
                .chain(std::iter::once(descriptor.description.as_str()))
                .chain(descriptor.aliases.iter().map(String::as_str))
                .chain(descriptor.commands.iter().map(String::as_str))
                .chain(descriptor.facets.domain.iter().map(String::as_str))
                .chain(descriptor.facets.action.iter().map(String::as_str))
                .chain(descriptor.facets.object.iter().map(String::as_str))
                .collect::<Vec<_>>()
                .join("\n");
            CapabilityDocument {
                id: descriptor.id.clone(),
                name: descriptor.name.clone(),
                description: descriptor.description.clone(),
                aliases: descriptor.aliases.clone(),
                commands: descriptor.commands.clone(),
                intent_document,
                available: true,
            }
        })
        .collect::<Vec<_>>();
    let valid_ids = documents
        .iter()
        .map(|document| document.id.clone())
        .collect::<BTreeSet<_>>();
    let index = CapabilityIndex::new(documents);
    let mut all = MetricAccumulator::default();
    let mut exact = MetricAccumulator::default();
    let mut bm25 = MetricAccumulator::default();
    let mut exact_cases = 0usize;
    let mut bm25_cases = 0usize;
    let mut positive_cases = 0usize;
    let mut no_match_cases = 0usize;
    let mut chinese_cases = 0usize;
    let mut latin_cases = 0usize;
    for group in &corpus.case_groups {
        for query in &group.queries {
            chinese_cases += usize::from(query.chars().any(is_cjk));
            latin_cases += usize::from(query.bytes().any(|byte| byte.is_ascii_alphabetic()));
            positive_cases += usize::from(!group.no_match);
            no_match_cases += usize::from(group.no_match);
            let hits = index.search(query, 5);
            let ids = hits
                .iter()
                .map(|hit| hit.document.id.clone())
                .collect::<Vec<_>>();
            all.observe(&group.expected_ids, group.no_match, &ids, &valid_ids);
            if hits
                .first()
                .is_some_and(|hit| hit.mode == RetrievalMode::Exact)
            {
                exact_cases += 1;
                exact.observe(&group.expected_ids, group.no_match, &ids, &valid_ids);
            } else {
                bm25_cases += 1;
                bm25.observe(&group.expected_ids, group.no_match, &ids, &valid_ids);
            }
        }
    }
    let metrics = all.finish();
    let exact_metrics = exact.finish();
    let bm25_metrics = bm25.finish();
    let thresholds = RetrievalThresholds::from(&corpus.thresholds);
    let passed = exact_cases > 0
        && bm25_cases > 0
        && chinese_cases > 0
        && latin_cases > 0
        && meets_thresholds(&metrics, &thresholds);
    Ok((
        CorpusReport {
            id: corpus.corpus_id.clone(),
            fingerprint: corpus.corpus_fingerprint.clone(),
            cases: metrics.cases,
            positive_cases,
            no_match_cases,
            chinese_cases,
            latin_cases,
            exact_cases,
            bm25_cases,
            metrics,
            exact_metrics,
            bm25_metrics,
            passed,
        },
        thresholds,
    ))
}

fn is_cjk(character: char) -> bool {
    matches!(character as u32, 0x3400..=0x4dbf | 0x4e00..=0x9fff)
}

fn meets_thresholds(metrics: &RetrievalMetrics, thresholds: &RetrievalThresholds) -> bool {
    metrics.cases >= thresholds.minimum_cases
        && metrics.recall_at5 >= thresholds.recall_at5
        && metrics.top1 >= thresholds.top1
        && metrics.mrr >= thresholds.mrr
        && metrics.no_match_precision >= thresholds.no_match_precision
        && metrics.id_validity >= thresholds.id_validity
}

async fn evaluate_projects(
    discovery: &ProjectDiscovery,
    manifest: &RetrievalEvaluationManifest,
) -> ProjectReport {
    let resource = resource(&discovery.catalog().fingerprint);
    let mut passed_cases = 0usize;
    let mut exact_cases = 0usize;
    let mut bm25_cases = 0usize;
    let mut hybrid_cases = 0usize;
    let mut baseline_top1 = 0usize;
    let mut hybrid_top1 = 0usize;
    for case in &manifest.project_cases {
        let baseline = discovery
            .discover(
                &case.query,
                manifest.candidate_limit,
                EmbeddingSelection::Unavailable(RetrievalDegradedReason::EmbeddingMissing),
            )
            .await;
        match baseline.mode {
            RetrievalMode::Exact => exact_cases += 1,
            RetrievalMode::Bm25 => bm25_cases += 1,
            RetrievalMode::HybridVerified => {}
        }
        let baseline_ok = baseline
            .hits
            .first()
            .is_some_and(|hit| hit.project.id == case.expected_top);
        baseline_top1 += usize::from(baseline_ok);
        let runner = DeterministicRunner::new(RunnerMode::Success);
        let hybrid = discovery
            .discover(
                &case.query,
                manifest.candidate_limit,
                EmbeddingSelection::Verified {
                    resource: &resource,
                    runner: &runner,
                },
            )
            .await;
        hybrid_cases += usize::from(hybrid.mode == RetrievalMode::HybridVerified);
        let hybrid_ok = hybrid
            .hits
            .first()
            .is_some_and(|hit| hit.project.id == case.expected_top);
        hybrid_top1 += usize::from(hybrid_ok);
        passed_cases += usize::from(baseline_ok && hybrid_ok);
    }
    let cases = manifest.project_cases.len();
    ProjectReport {
        cases,
        passed_cases,
        exact_cases,
        bm25_cases,
        hybrid_cases,
        baseline_top1: ratio(baseline_top1, cases),
        hybrid_top1: ratio(hybrid_top1, cases),
        passed: passed_cases == cases && exact_cases > 0 && bm25_cases > 0 && hybrid_cases > 0,
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WorkspaceCases {
    schema_version: u16,
    cases: Vec<WorkspaceCase>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WorkspaceCase {
    id: String,
    query: String,
    kind: CapabilityKind,
    expected_top: Option<String>,
}

fn validate_workspace_cases(cases: &WorkspaceCases) -> Result<(), RetrievalEvaluationError> {
    let mut ids = BTreeSet::new();
    if cases.schema_version != 1
        || cases.cases.len() != 15
        || cases.cases.iter().any(|case| {
            case.id.is_empty() || case.query.trim().is_empty() || !ids.insert(case.id.as_str())
        })
    {
        return Err(RetrievalEvaluationError::FixtureParse(
            "fixtures/compat/retrieval/capability-workspace.v1.json".into(),
        ));
    }
    Ok(())
}

async fn evaluate_workspace(
    workspace: &CapabilityWorkspace,
    cases: &WorkspaceCases,
) -> WorkspaceReport {
    let inventory = CapabilityInventory::default();
    let resource = resource(workspace.fingerprint());
    let mut passed_cases = 0usize;
    let mut exact_cases = 0usize;
    let mut bm25_cases = 0usize;
    let mut hybrid_cases = 0usize;
    let mut kind_isolation = true;
    let mut no_match_preserved = true;
    for case in &cases.cases {
        let baseline = workspace
            .discover(
                &case.query,
                Some(case.kind),
                5,
                &inventory,
                EmbeddingSelection::Unavailable(RetrievalDegradedReason::EmbeddingMissing),
            )
            .await;
        match baseline.mode {
            RetrievalMode::Exact => exact_cases += 1,
            RetrievalMode::Bm25 => bm25_cases += 1,
            RetrievalMode::HybridVerified => {}
        }
        kind_isolation &= baseline.hits.iter().all(|hit| hit.card.kind == case.kind);
        let baseline_top = baseline.hits.first().map(|hit| hit.card.id.as_str());
        let runner = DeterministicRunner::new(RunnerMode::Success);
        let hybrid = workspace
            .discover(
                &case.query,
                Some(case.kind),
                5,
                &inventory,
                EmbeddingSelection::Verified {
                    resource: &resource,
                    runner: &runner,
                },
            )
            .await;
        hybrid_cases += usize::from(hybrid.mode == RetrievalMode::HybridVerified);
        kind_isolation &= hybrid.hits.iter().all(|hit| hit.card.kind == case.kind);
        let hybrid_top = hybrid.hits.first().map(|hit| hit.card.id.as_str());
        if case.expected_top.is_none() {
            no_match_preserved &= baseline.hits.is_empty() && hybrid.hits.is_empty();
        }
        passed_cases += usize::from(
            baseline_top == case.expected_top.as_deref()
                && hybrid_top == case.expected_top.as_deref(),
        );
    }
    let total = cases.cases.len();
    WorkspaceReport {
        cases: total,
        passed_cases,
        exact_cases,
        bm25_cases,
        hybrid_cases,
        kind_isolation,
        no_match_preserved,
        passed: passed_cases == total
            && exact_cases > 0
            && bm25_cases > 0
            && hybrid_cases > 0
            && kind_isolation
            && no_match_preserved,
    }
}

async fn evaluate_candidate_boundary(
    discovery: &ProjectDiscovery,
    query: &str,
    limit: usize,
) -> CandidateBoundaryReport {
    let baseline = discovery
        .discover(
            query,
            limit,
            EmbeddingSelection::Unavailable(RetrievalDegradedReason::EmbeddingMissing),
        )
        .await;
    let lexical_candidate_ids = project_ids(&baseline);
    let resource = resource(&discovery.catalog().fingerprint);
    let runner = DeterministicRunner::new(RunnerMode::Success);
    let hybrid = discovery
        .discover(
            query,
            limit,
            EmbeddingSelection::Verified {
                resource: &resource,
                runner: &runner,
            },
        )
        .await;
    let observed_candidate_ids = runner.observed_candidate_ids();
    let semantic_candidate_ids = runner.returned_candidate_ids();
    let hybrid_result_ids = project_ids(&hybrid);

    let outsider = DeterministicRunner::new(RunnerMode::Outsider);
    let outsider_result = discovery
        .discover(
            query,
            limit,
            EmbeddingSelection::Verified {
                resource: &resource,
                runner: &outsider,
            },
        )
        .await;
    let outsider_result_ids = project_ids(&outsider_result);
    let outsider_rejected = outsider_result.mode == RetrievalMode::Bm25
        && outsider_result.degraded_reason == Some(RetrievalDegradedReason::MalformedVector)
        && outsider_result_ids == lexical_candidate_ids;
    let passed = !lexical_candidate_ids.is_empty()
        && observed_candidate_ids == lexical_candidate_ids
        && semantic_candidate_ids == lexical_candidate_ids
        && hybrid_result_ids
            .iter()
            .all(|id| lexical_candidate_ids.contains(id))
        && outsider_rejected;
    CandidateBoundaryReport {
        query: query.to_owned(),
        lexical_candidate_ids,
        observed_candidate_ids,
        semantic_candidate_ids,
        hybrid_result_ids,
        outsider_attempted_id: "outside/bm25".into(),
        outsider_result_ids,
        outsider_rejected,
        passed,
    }
}

async fn evaluate_degradations(
    discovery: &ProjectDiscovery,
    query: &str,
    limit: usize,
) -> Vec<DegradationReport> {
    let baseline = discovery
        .discover(
            query,
            limit,
            EmbeddingSelection::Unavailable(RetrievalDegradedReason::EmbeddingMissing),
        )
        .await;
    let baseline_ids = project_ids(&baseline);
    let resource = resource(&discovery.catalog().fingerprint);
    let mut reports = Vec::new();
    for (scenario, selection) in [
        (
            "no_resource",
            ScenarioSelection::Unavailable(RetrievalDegradedReason::EmbeddingMissing),
        ),
        (
            "damaged_resource",
            ScenarioSelection::Unavailable(RetrievalDegradedReason::InvalidManifest),
        ),
        (
            "runner_failure",
            ScenarioSelection::Runner(RunnerMode::Failure(
                RetrievalDegradedReason::HelperUnavailable,
            )),
        ),
        (
            "malformed_response",
            ScenarioSelection::Runner(RunnerMode::Malformed),
        ),
        (
            "timeout",
            ScenarioSelection::Runner(RunnerMode::Failure(RetrievalDegradedReason::HelperTimeout)),
        ),
        ("outsider", ScenarioSelection::Runner(RunnerMode::Outsider)),
    ] {
        let runner;
        let embedding = match selection {
            ScenarioSelection::Unavailable(reason) => EmbeddingSelection::Unavailable(reason),
            ScenarioSelection::Runner(mode) => {
                runner = DeterministicRunner::new(mode);
                EmbeddingSelection::Verified {
                    resource: &resource,
                    runner: &runner,
                }
            }
        };
        let result = discovery.discover(query, limit, embedding).await;
        let ids = project_ids(&result);
        reports.push(DegradationReport {
            scenario: scenario.into(),
            reason: result
                .degraded_reason
                .unwrap_or(RetrievalDegradedReason::MalformedVector),
            mode: result.mode,
            bm25_ids_preserved: ids == baseline_ids,
            result_ids: ids,
        });
    }
    reports
}

enum ScenarioSelection {
    Unavailable(RetrievalDegradedReason),
    Runner(RunnerMode),
}

#[derive(Clone, Copy)]
enum RunnerMode {
    Success,
    Failure(RetrievalDegradedReason),
    Malformed,
    Outsider,
}

struct DeterministicRunner {
    mode: RunnerMode,
    requests: Mutex<Vec<EmbeddingRequest>>,
    returned_ids: Mutex<Vec<String>>,
}

impl DeterministicRunner {
    fn new(mode: RunnerMode) -> Self {
        Self {
            mode,
            requests: Mutex::new(Vec::new()),
            returned_ids: Mutex::new(Vec::new()),
        }
    }

    fn observed_candidate_ids(&self) -> Vec<String> {
        self.requests
            .lock()
            .expect("recording runner requests")
            .first()
            .map(|request| {
                request
                    .candidates
                    .iter()
                    .map(|candidate| candidate.id.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn returned_candidate_ids(&self) -> Vec<String> {
        self.returned_ids
            .lock()
            .expect("recording runner output")
            .clone()
    }
}

impl EmbeddingRunner for DeterministicRunner {
    fn embed<'a>(
        &'a self,
        resource: &'a VerifiedEmbeddingResource,
        request: &'a EmbeddingRequest,
    ) -> Pin<Box<dyn Future<Output = Result<EmbeddingOutput, RetrievalDegradedReason>> + Send + 'a>>
    {
        Box::pin(async move {
            self.requests
                .lock()
                .expect("recording runner requests")
                .push(request.clone());
            if let RunnerMode::Failure(reason) = self.mode {
                return Err(reason);
            }
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
            match self.mode {
                RunnerMode::Malformed if candidates.len() > 1 => {
                    candidates[1].id = candidates[0].id.clone();
                }
                RunnerMode::Outsider if !candidates.is_empty() => {
                    candidates[0].id = "outside/bm25".into();
                }
                _ => {}
            }
            *self.returned_ids.lock().expect("recording runner output") = candidates
                .iter()
                .map(|candidate| candidate.id.clone())
                .collect();
            Ok(EmbeddingOutput {
                schema_version: SchemaVersion,
                model_id: resource.manifest.model_id.clone(),
                runtime_abi: resource.manifest.runtime_abi.clone(),
                catalog_fingerprint: request.catalog_fingerprint.clone(),
                vector_fingerprint: request.vector_fingerprint.clone(),
                dimensions: 3,
                query_vector: vec![1.0, 0.0, 0.0],
                candidates,
            })
        })
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

fn project_ids(result: &minimax_retrieval::ProjectDiscoveryResult) -> Vec<String> {
    result
        .hits
        .iter()
        .map(|hit| hit.project.id.clone())
        .collect()
}

fn manifest_fingerprint(
    manifest: &RetrievalEvaluationManifest,
) -> Result<String, RetrievalEvaluationError> {
    serde_json::to_vec(manifest)
        .map(|bytes| sha256(&bytes))
        .map_err(|_| RetrievalEvaluationError::ReportSerialization)
}

fn validate_relative_path(path: &str) -> Result<(), RetrievalEvaluationError> {
    let parsed = Path::new(path);
    if path.is_empty()
        || path.contains('\\')
        || parsed.is_absolute()
        || parsed.components().any(|component| {
            matches!(
                component,
                Component::Prefix(_)
                    | Component::RootDir
                    | Component::ParentDir
                    | Component::CurDir
            )
        })
    {
        return invalid_manifest("fixture paths must be safe repository-relative paths");
    }
    Ok(())
}

fn sha256(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(&mut output, "{byte:02x}").expect("writing to a String cannot fail");
    }
    output
}

fn normalize_newline(value: &str) -> String {
    value.trim_end_matches(['\r', '\n']).to_owned() + "\n"
}

fn invalid_manifest<T>(message: impl Into<String>) -> Result<T, RetrievalEvaluationError> {
    Err(RetrievalEvaluationError::InvalidManifest(message.into()))
}

fn invalid_corpus<T>(message: impl Into<String>) -> Result<T, RetrievalEvaluationError> {
    Err(RetrievalEvaluationError::InvalidCorpus(message.into()))
}
