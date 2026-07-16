use std::collections::{BTreeMap, BTreeSet};

use minimax_protocol::{
    ContentHash, EvidenceId, KnowledgeEvaluationJob, KnowledgeOperation, KnowledgePageStatus,
    KnowledgePatch, KnowledgeReceipt, KnowledgeReceiptOutcome, ModelBinding, PageId, SchemaVersion,
    SourceCitation, TopicId, TransactionId, Usage, WikiWorkflowEvent, WikiWorkflowState,
    WikiWorkflowUsage,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DurabilitySignals {
    pub decisions: u16,
    pub constraints: u16,
    pub preferences: u16,
    pub architecture_changes: u16,
    pub durable_behavior_changes: u16,
    pub diagnosed_lessons: u16,
    pub todos_or_risks: u16,
    pub repeated_only: bool,
    pub lookup_only: bool,
    pub inconclusive_failure: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DurabilityCode {
    Decision,
    Constraint,
    Preference,
    Architecture,
    DurableBehavior,
    DiagnosedLesson,
    TodoOrRisk,
    Repeated,
    LookupOnly,
    InconclusiveFailure,
    NoDurableSignal,
}

impl DurabilityCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Decision => "durable_decision",
            Self::Constraint => "durable_constraint",
            Self::Preference => "durable_preference",
            Self::Architecture => "durable_architecture",
            Self::DurableBehavior => "durable_behavior",
            Self::DiagnosedLesson => "diagnosed_lesson",
            Self::TodoOrRisk => "todo_or_risk",
            Self::Repeated => "repeated_information",
            Self::LookupOnly => "lookup_only",
            Self::InconclusiveFailure => "inconclusive_failure",
            Self::NoDurableSignal => "no_durable_signal",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DurabilityDecision {
    Durable(DurabilityCode),
    NoOp(DurabilityCode),
}

impl DurabilityDecision {
    #[must_use]
    pub const fn code(self) -> DurabilityCode {
        match self {
            Self::Durable(code) | Self::NoOp(code) => code,
        }
    }
}

pub struct DurabilityGate;

impl DurabilityGate {
    #[must_use]
    pub const fn evaluate(signals: DurabilitySignals) -> DurabilityDecision {
        let durable = if signals.decisions > 0 {
            Some(DurabilityCode::Decision)
        } else if signals.constraints > 0 {
            Some(DurabilityCode::Constraint)
        } else if signals.preferences > 0 {
            Some(DurabilityCode::Preference)
        } else if signals.architecture_changes > 0 {
            Some(DurabilityCode::Architecture)
        } else if signals.durable_behavior_changes > 0 {
            Some(DurabilityCode::DurableBehavior)
        } else if signals.diagnosed_lessons > 0 {
            Some(DurabilityCode::DiagnosedLesson)
        } else if signals.todos_or_risks > 0 {
            Some(DurabilityCode::TodoOrRisk)
        } else {
            None
        };
        if let Some(code) = durable {
            DurabilityDecision::Durable(code)
        } else if signals.repeated_only {
            DurabilityDecision::NoOp(DurabilityCode::Repeated)
        } else if signals.lookup_only {
            DurabilityDecision::NoOp(DurabilityCode::LookupOnly)
        } else if signals.inconclusive_failure {
            DurabilityDecision::NoOp(DurabilityCode::InconclusiveFailure)
        } else {
            DurabilityDecision::NoOp(DurabilityCode::NoDurableSignal)
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WikiEvidenceChunk {
    pub source_id: EvidenceId,
    pub source_hash: ContentHash,
    pub label: String,
    pub content: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WikiCurrentExcerpt {
    pub page_id: PageId,
    pub topic_id: TopicId,
    pub title: String,
    pub content: String,
    pub content_hash: ContentHash,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WikiGenerationRequest {
    pub job: KnowledgeEvaluationJob,
    pub evidence: Vec<WikiEvidenceChunk>,
    pub current: Vec<WikiCurrentExcerpt>,
    pub schema_repair_only: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WikiGenerationOutput {
    pub raw_json: String,
    pub usage: Usage,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WikiGenerationError {
    Unavailable,
    BindingMismatch,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CurrentWikiPage {
    pub page_id: PageId,
    pub topic_id: TopicId,
    pub relative_path: String,
    pub status: KnowledgePageStatus,
    pub superseded_by: Option<PageId>,
    pub content_hash: ContentHash,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KnowledgeValidationContext {
    pub job_id: minimax_protocol::KnowledgeJobId,
    pub evidence: BTreeMap<EvidenceId, ContentHash>,
    pub pages: Vec<CurrentWikiPage>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KnowledgeGuardError {
    InvalidPatch,
    WrongJob,
    UnknownSource,
    SourceHashMismatch,
    DuplicatePage,
    DuplicateTarget,
    StaleHash,
    CurrentTruthConflict,
    InvalidSupersession,
    SensitiveContent,
    InjectionContent,
    OversizedPatch,
}

pub struct KnowledgePatchValidator;

impl KnowledgePatchValidator {
    pub fn validate(
        patch: KnowledgePatch,
        context: &KnowledgeValidationContext,
    ) -> Result<KnowledgePatch, KnowledgeGuardError> {
        let patch = patch
            .validate()
            .map_err(|_| KnowledgeGuardError::InvalidPatch)?;
        if patch.job_id != context.job_id {
            return Err(KnowledgeGuardError::WrongJob);
        }
        if serde_json::to_vec(&patch)
            .map_err(|_| KnowledgeGuardError::InvalidPatch)?
            .len()
            > 256 * 1024
        {
            return Err(KnowledgeGuardError::OversizedPatch);
        }
        validate_initial_pages(&context.pages)?;
        let mut projection = context
            .pages
            .iter()
            .cloned()
            .map(|page| (page.page_id.clone(), page))
            .collect::<BTreeMap<_, _>>();
        let mut paths = context
            .pages
            .iter()
            .map(|page| (page.relative_path.clone(), page.page_id.clone()))
            .collect::<BTreeMap<_, _>>();
        let mut operation_paths = BTreeSet::new();
        for operation in &patch.operations {
            match operation {
                KnowledgeOperation::Create { page } => {
                    validate_page_content(page.title.as_str(), page.body.as_str())?;
                    validate_sources(&page.sources, &context.evidence)?;
                    if projection.contains_key(&page.page_id)
                        || paths.contains_key(&page.relative_path)
                        || !operation_paths.insert(page.relative_path.clone())
                    {
                        return Err(KnowledgeGuardError::DuplicateTarget);
                    }
                    let projected = CurrentWikiPage {
                        page_id: page.page_id.clone(),
                        topic_id: page.topic_id.clone(),
                        relative_path: page.relative_path.clone(),
                        status: page.status,
                        superseded_by: page.superseded_by.clone(),
                        content_hash: zero_hash(),
                    };
                    paths.insert(page.relative_path.clone(), page.page_id.clone());
                    projection.insert(page.page_id.clone(), projected);
                }
                KnowledgeOperation::Replace {
                    page,
                    expected_hash,
                } => {
                    validate_page_content(page.title.as_str(), page.body.as_str())?;
                    validate_sources(&page.sources, &context.evidence)?;
                    if !operation_paths.insert(page.relative_path.clone()) {
                        return Err(KnowledgeGuardError::DuplicateTarget);
                    }
                    let existing = projection
                        .get(&page.page_id)
                        .ok_or(KnowledgeGuardError::DuplicatePage)?;
                    if existing.relative_path != page.relative_path
                        || existing.topic_id != page.topic_id
                        || existing.content_hash != *expected_hash
                    {
                        return Err(KnowledgeGuardError::StaleHash);
                    }
                    projection.insert(
                        page.page_id.clone(),
                        CurrentWikiPage {
                            page_id: page.page_id.clone(),
                            topic_id: page.topic_id.clone(),
                            relative_path: page.relative_path.clone(),
                            status: page.status,
                            superseded_by: page.superseded_by.clone(),
                            content_hash: zero_hash(),
                        },
                    );
                }
                KnowledgeOperation::Remove {
                    page_id,
                    relative_path,
                    expected_hash,
                } => {
                    if !operation_paths.insert(relative_path.clone()) {
                        return Err(KnowledgeGuardError::DuplicateTarget);
                    }
                    let existing = projection
                        .get(page_id)
                        .ok_or(KnowledgeGuardError::DuplicatePage)?;
                    if existing.relative_path != *relative_path
                        || existing.content_hash != *expected_hash
                    {
                        return Err(KnowledgeGuardError::StaleHash);
                    }
                    projection.remove(page_id);
                    paths.remove(relative_path);
                }
            }
        }
        validate_projected_truth(projection.values())?;
        Ok(patch)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum KnowledgeInput {
    Evaluated(DurabilityDecision),
    GeneratedPatch { patch: KnowledgePatch, usage: Usage },
    SchemaRejected { code: String, usage: Usage },
    UnsafeGenerationRejected { code: String, usage: Usage },
    SemanticRejected { code: String },
    GenerationUnavailable { code: String },
    PatchValidated { patch_hash: ContentHash },
    Committed { transaction_id: TransactionId },
    CommitPending { code: String },
    Rebind { model_binding: ModelBinding },
    Recover,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum KnowledgeEffect {
    PersistEvent(WikiWorkflowEvent),
    PublishEvent(WikiWorkflowEvent),
    Generate(WikiGenerationRequest),
    Validate(KnowledgePatch),
    Commit {
        patch: KnowledgePatch,
        patch_hash: ContentHash,
    },
    PersistReceipt(KnowledgeReceipt),
    PublishReceipt(KnowledgeReceipt),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WikiWorkflowError {
    InvalidTransition,
    InvalidJob,
    WrongJob,
    InvalidCode,
    MissingPatch,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MainModelWikiWorkflow {
    job: KnowledgeEvaluationJob,
    state: WikiWorkflowState,
    generation_attempts: u8,
    usage: Option<Usage>,
    patch: Option<KnowledgePatch>,
    patch_hash: Option<ContentHash>,
}

impl MainModelWikiWorkflow {
    pub fn new(job: KnowledgeEvaluationJob) -> Result<Self, WikiWorkflowError> {
        let job = job.validate().map_err(|_| WikiWorkflowError::InvalidJob)?;
        Ok(Self {
            job,
            state: WikiWorkflowState::EvaluationPending,
            generation_attempts: 0,
            usage: None,
            patch: None,
            patch_hash: None,
        })
    }

    #[must_use]
    pub const fn state(&self) -> WikiWorkflowState {
        self.state
    }

    #[must_use]
    pub const fn job(&self) -> &KnowledgeEvaluationJob {
        &self.job
    }

    #[must_use]
    pub const fn generation_attempts(&self) -> u8 {
        self.generation_attempts
    }

    pub fn apply(
        &mut self,
        input: KnowledgeInput,
        evidence: &[WikiEvidenceChunk],
        current: &[WikiCurrentExcerpt],
    ) -> Result<Vec<KnowledgeEffect>, WikiWorkflowError> {
        match (self.state, input) {
            (WikiWorkflowState::EvaluationPending, KnowledgeInput::Evaluated(decision)) => {
                self.after_evaluation(decision, evidence, current)
            }
            (WikiWorkflowState::Generating, KnowledgeInput::GeneratedPatch { patch, usage }) => {
                self.after_generation(patch, usage)
            }
            (WikiWorkflowState::Generating, KnowledgeInput::SchemaRejected { code, usage }) => {
                self.after_schema_rejection(code, usage, evidence, current)
            }
            (
                WikiWorkflowState::Generating,
                KnowledgeInput::UnsafeGenerationRejected { code, usage },
            ) => {
                validate_code(&code)?;
                self.usage = Some(merge_usage(self.usage, usage));
                self.failed(code)
            }
            (WikiWorkflowState::Generating, KnowledgeInput::GenerationUnavailable { code }) => {
                self.pending(code)
            }
            (WikiWorkflowState::Validating, KnowledgeInput::SemanticRejected { code }) => {
                self.failed(code)
            }
            (WikiWorkflowState::Validating, KnowledgeInput::PatchValidated { patch_hash }) => {
                self.after_validation(patch_hash)
            }
            (WikiWorkflowState::Committing, KnowledgeInput::Committed { transaction_id }) => {
                self.synthesized(transaction_id)
            }
            (WikiWorkflowState::Committing, KnowledgeInput::CommitPending { code }) => {
                self.pending(code)
            }
            (WikiWorkflowState::Pending, KnowledgeInput::Rebind { model_binding }) => {
                self.job.model_binding = model_binding;
                self.generation_attempts = 1;
                self.usage = None;
                self.patch = None;
                self.patch_hash = None;
                self.state = WikiWorkflowState::Generating;
                let mut effects = self.observable("explicit_model_rebind", None)?;
                effects.push(KnowledgeEffect::Generate(
                    self.generation_request(evidence, current, false),
                ));
                Ok(effects)
            }
            (
                WikiWorkflowState::SynthesisPending
                | WikiWorkflowState::Generating
                | WikiWorkflowState::Validating
                | WikiWorkflowState::Committing,
                KnowledgeInput::Recover,
            ) => self.pending("recovered_pending".to_owned()),
            (
                WikiWorkflowState::EvaluationPending
                | WikiWorkflowState::NoOp
                | WikiWorkflowState::Synthesized
                | WikiWorkflowState::Pending
                | WikiWorkflowState::Failed,
                KnowledgeInput::Recover,
            ) => Ok(Vec::new()),
            _ => Err(WikiWorkflowError::InvalidTransition),
        }
    }

    fn after_evaluation(
        &mut self,
        decision: DurabilityDecision,
        evidence: &[WikiEvidenceChunk],
        current: &[WikiCurrentExcerpt],
    ) -> Result<Vec<KnowledgeEffect>, WikiWorkflowError> {
        match decision {
            DurabilityDecision::NoOp(code) => {
                self.state = WikiWorkflowState::NoOp;
                let code = code.as_str();
                let mut effects = self.observable(code, None)?;
                effects.extend(self.receipt(KnowledgeReceiptOutcome::NoOp, code, None));
                Ok(effects)
            }
            DurabilityDecision::Durable(code) => {
                self.state = WikiWorkflowState::SynthesisPending;
                let mut effects = self.observable(code.as_str(), None)?;
                self.state = WikiWorkflowState::Generating;
                self.generation_attempts = 1;
                effects.extend(self.observable("generation_started", None)?);
                effects.push(KnowledgeEffect::Generate(
                    self.generation_request(evidence, current, false),
                ));
                Ok(effects)
            }
        }
    }

    fn after_generation(
        &mut self,
        patch: KnowledgePatch,
        usage: Usage,
    ) -> Result<Vec<KnowledgeEffect>, WikiWorkflowError> {
        if patch.job_id != self.job.job_id {
            return Err(WikiWorkflowError::WrongJob);
        }
        self.usage = Some(merge_usage(self.usage, usage));
        self.patch = Some(patch.clone());
        self.state = WikiWorkflowState::Validating;
        let mut effects = self.observable("patch_received", self.usage)?;
        effects.push(KnowledgeEffect::Validate(patch));
        Ok(effects)
    }

    fn after_schema_rejection(
        &mut self,
        code: String,
        usage: Usage,
        evidence: &[WikiEvidenceChunk],
        current: &[WikiCurrentExcerpt],
    ) -> Result<Vec<KnowledgeEffect>, WikiWorkflowError> {
        validate_code(&code)?;
        self.usage = Some(merge_usage(self.usage, usage));
        if self.generation_attempts >= 2 {
            return self.failed("schema_repair_exhausted".to_owned());
        }
        self.generation_attempts += 1;
        let mut effects = self.observable("schema_repair_started", self.usage)?;
        effects.push(KnowledgeEffect::Generate(
            self.generation_request(evidence, current, true),
        ));
        Ok(effects)
    }

    fn after_validation(
        &mut self,
        patch_hash: ContentHash,
    ) -> Result<Vec<KnowledgeEffect>, WikiWorkflowError> {
        let patch = self.patch.clone().ok_or(WikiWorkflowError::MissingPatch)?;
        self.patch_hash = Some(patch_hash.clone());
        self.state = WikiWorkflowState::Committing;
        let mut effects = self.observable("patch_validated", self.usage)?;
        effects.push(KnowledgeEffect::Commit { patch, patch_hash });
        Ok(effects)
    }

    fn synthesized(
        &mut self,
        transaction_id: TransactionId,
    ) -> Result<Vec<KnowledgeEffect>, WikiWorkflowError> {
        self.state = WikiWorkflowState::Synthesized;
        let mut effects = self.observable("synthesized", self.usage)?;
        effects.extend(self.receipt(
            KnowledgeReceiptOutcome::Synthesized,
            "synthesized",
            Some(transaction_id),
        ));
        Ok(effects)
    }

    fn pending(&mut self, code: String) -> Result<Vec<KnowledgeEffect>, WikiWorkflowError> {
        validate_code(&code)?;
        self.state = WikiWorkflowState::Pending;
        let mut effects = self.observable(&code, self.usage)?;
        effects.extend(self.receipt(KnowledgeReceiptOutcome::Pending, &code, None));
        Ok(effects)
    }

    fn failed(&mut self, code: String) -> Result<Vec<KnowledgeEffect>, WikiWorkflowError> {
        validate_code(&code)?;
        self.state = WikiWorkflowState::Failed;
        let mut effects = self.observable(&code, self.usage)?;
        effects.extend(self.receipt(KnowledgeReceiptOutcome::Failed, &code, None));
        Ok(effects)
    }

    fn generation_request(
        &self,
        evidence: &[WikiEvidenceChunk],
        current: &[WikiCurrentExcerpt],
        schema_repair_only: bool,
    ) -> WikiGenerationRequest {
        WikiGenerationRequest {
            job: self.job.clone(),
            evidence: evidence.to_vec(),
            current: current.to_vec(),
            schema_repair_only,
        }
    }

    fn observable(
        &self,
        code: &str,
        usage: Option<Usage>,
    ) -> Result<Vec<KnowledgeEffect>, WikiWorkflowError> {
        validate_code(code)?;
        let event = WikiWorkflowEvent {
            schema_version: SchemaVersion,
            job_id: self.job.job_id.clone(),
            state: self.state,
            code: code.to_owned(),
            usage: usage.map(|usage| WikiWorkflowUsage {
                model_binding: self.job.model_binding.clone(),
                usage,
            }),
        }
        .validate()
        .map_err(|_| WikiWorkflowError::InvalidCode)?;
        Ok(vec![
            KnowledgeEffect::PersistEvent(event.clone()),
            KnowledgeEffect::PublishEvent(event),
        ])
    }

    fn receipt(
        &self,
        outcome: KnowledgeReceiptOutcome,
        code: &str,
        transaction_id: Option<TransactionId>,
    ) -> Vec<KnowledgeEffect> {
        let receipt = KnowledgeReceipt {
            schema_version: SchemaVersion,
            job_id: self.job.job_id.clone(),
            source_id: self.job.source_id.clone(),
            source_hash: self.job.source_hash.clone(),
            outcome,
            code: code.to_owned(),
            model_binding: self.job.model_binding.clone(),
            usage: self.usage,
            patch_hash: self.patch_hash.clone(),
            transaction_id,
        };
        vec![
            KnowledgeEffect::PersistReceipt(receipt.clone()),
            KnowledgeEffect::PublishReceipt(receipt),
        ]
    }
}

fn validate_initial_pages(pages: &[CurrentWikiPage]) -> Result<(), KnowledgeGuardError> {
    let mut ids = BTreeSet::new();
    let mut paths = BTreeSet::new();
    if pages
        .iter()
        .any(|page| !ids.insert(page.page_id.clone()) || !paths.insert(page.relative_path.clone()))
    {
        return Err(KnowledgeGuardError::DuplicatePage);
    }
    validate_projected_truth(pages.iter())
}

fn validate_projected_truth<'a>(
    pages: impl IntoIterator<Item = &'a CurrentWikiPage>,
) -> Result<(), KnowledgeGuardError> {
    let pages = pages.into_iter().collect::<Vec<_>>();
    let mut current_topics = BTreeSet::new();
    for page in &pages {
        if page.status == KnowledgePageStatus::Current
            && !current_topics.insert(page.topic_id.clone())
        {
            return Err(KnowledgeGuardError::CurrentTruthConflict);
        }
    }
    for page in &pages {
        match (page.status, &page.superseded_by) {
            (KnowledgePageStatus::Current, None) => {}
            (KnowledgePageStatus::Superseded, Some(replacement)) => {
                let replacement = pages
                    .iter()
                    .find(|candidate| candidate.page_id == *replacement)
                    .ok_or(KnowledgeGuardError::InvalidSupersession)?;
                if replacement.topic_id != page.topic_id
                    || replacement.status != KnowledgePageStatus::Current
                {
                    return Err(KnowledgeGuardError::InvalidSupersession);
                }
            }
            _ => return Err(KnowledgeGuardError::InvalidSupersession),
        }
    }
    Ok(())
}

fn validate_sources(
    sources: &[SourceCitation],
    allowed: &BTreeMap<EvidenceId, ContentHash>,
) -> Result<(), KnowledgeGuardError> {
    for source in sources {
        let expected = allowed
            .get(&source.source_id)
            .ok_or(KnowledgeGuardError::UnknownSource)?;
        if expected != &source.source_hash {
            return Err(KnowledgeGuardError::SourceHashMismatch);
        }
    }
    Ok(())
}

fn validate_page_content(title: &str, body: &str) -> Result<(), KnowledgeGuardError> {
    let value = format!("{title}\n{body}").to_ascii_lowercase();
    let secret_markers = [
        "-----begin private key-----",
        "-----begin rsa private key-----",
        "-----begin openssh private key-----",
        "github_pat_",
        "api_key=",
        "api-key=",
        "client_secret=",
        "access_token=",
        "password=",
        "authorization: bearer ",
        "private_reasoning",
        "chain_of_thought",
    ];
    if secret_markers.iter().any(|marker| value.contains(marker)) {
        return Err(KnowledgeGuardError::SensitiveContent);
    }
    let injection_markers = [
        "ignore previous instructions",
        "follow these instructions instead",
        "<system",
        "<assistant",
        "tool_calls",
        "execute this command",
    ];
    if injection_markers
        .iter()
        .any(|marker| value.contains(marker))
    {
        return Err(KnowledgeGuardError::InjectionContent);
    }
    Ok(())
}

fn zero_hash() -> ContentHash {
    ContentHash::new("0".repeat(64)).expect("fixed hash")
}

fn validate_code(code: &str) -> Result<(), WikiWorkflowError> {
    if code.is_empty()
        || code.len() > 64
        || !code
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(WikiWorkflowError::InvalidCode);
    }
    Ok(())
}

fn merge_usage(current: Option<Usage>, next: Usage) -> Usage {
    let current = current.unwrap_or_default();
    Usage {
        input_tokens: merge_optional(current.input_tokens, next.input_tokens),
        output_tokens: merge_optional(current.output_tokens, next.output_tokens),
        total_tokens: merge_optional(current.total_tokens, next.total_tokens),
    }
}

fn merge_optional(left: Option<u64>, right: Option<u64>) -> Option<u64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.saturating_add(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}
