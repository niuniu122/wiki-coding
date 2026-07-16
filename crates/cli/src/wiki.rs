use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::path::PathBuf;

use minimax_core::{
    DurabilityGate, DurabilitySignals, KnowledgeCommitError, KnowledgeEffect, KnowledgeGuardError,
    KnowledgeInput, KnowledgePatchValidator, KnowledgePort, KnowledgeValidationContext,
    MainModelWikiWorkflow, WikiCurrentExcerpt, WikiEvidenceChunk, WikiGenerationError,
    WikiGenerationPort,
};
use minimax_protocol::{
    KnowledgeOperation, KnowledgePage, KnowledgePageStatus, KnowledgePatch, KnowledgeReceipt,
    KnowledgeReceiptOutcome, ModelBinding, ProjectId, TransactionId, WikiWorkflowEvent,
};
use minimax_vault::{
    FinalizedSessionEvidence, KnowledgeWorkflowStore, PreparedWikiTransaction, ProjectVault,
    StoredGeneration, VaultError, WikiChange, hash_vault_bytes, knowledge_job_for_session,
    read_wiki_pages, recover_wiki_transaction, render_wiki_page, wiki_transaction_exists,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectVaultBinding {
    pub project_root: PathBuf,
    pub vault_root: PathBuf,
    pub project_id: ProjectId,
    pub created_at_unix_ms: u64,
}

impl ProjectVaultBinding {
    pub fn open(&self) -> Result<ProjectVault, VaultError> {
        ProjectVault::bootstrap(
            &self.project_root,
            &self.vault_root,
            self.project_id.clone(),
            self.created_at_unix_ms,
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WikiFaultPoint {
    AfterGenerationRecord,
    AfterCommit,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WikiRunReport {
    pub events: Vec<WikiWorkflowEvent>,
    pub receipt: KnowledgeReceipt,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WikiDriverError {
    Vault(VaultError),
    Workflow,
    MissingReceipt,
    FaultInjected,
}

impl fmt::Display for WikiDriverError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Vault(error) => error.fmt(formatter),
            Self::Workflow => formatter.write_str("the Wiki workflow entered an invalid state"),
            Self::MissingReceipt => {
                formatter.write_str("the Wiki workflow ended without a receipt")
            }
            Self::FaultInjected => {
                formatter.write_str("the test interrupted the Wiki workflow at a durable boundary")
            }
        }
    }
}

impl std::error::Error for WikiDriverError {}

impl From<VaultError> for WikiDriverError {
    fn from(value: VaultError) -> Self {
        Self::Vault(value)
    }
}

pub struct MainModelWikiDriver<G, K> {
    vault: ProjectVaultBinding,
    generation: G,
    knowledge: K,
}

impl<G, K> MainModelWikiDriver<G, K>
where
    G: WikiGenerationPort,
    K: KnowledgePort,
{
    #[must_use]
    pub const fn new(vault: ProjectVaultBinding, generation: G, knowledge: K) -> Self {
        Self {
            vault,
            generation,
            knowledge,
        }
    }

    pub async fn run(
        &self,
        evidence: &FinalizedSessionEvidence,
        signals: DurabilitySignals,
        evidence_chunks: Vec<WikiEvidenceChunk>,
        current: Vec<WikiCurrentExcerpt>,
        validation: KnowledgeValidationContext,
    ) -> Result<WikiRunReport, WikiDriverError> {
        self.run_inner(
            evidence,
            signals,
            evidence_chunks,
            current,
            validation,
            None,
            None,
        )
        .await
    }

    pub async fn run_rebound(
        &self,
        evidence: &FinalizedSessionEvidence,
        signals: DurabilitySignals,
        evidence_chunks: Vec<WikiEvidenceChunk>,
        current: Vec<WikiCurrentExcerpt>,
        validation: KnowledgeValidationContext,
        model_binding: ModelBinding,
    ) -> Result<WikiRunReport, WikiDriverError> {
        self.run_inner(
            evidence,
            signals,
            evidence_chunks,
            current,
            validation,
            Some(model_binding),
            None,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn run_with_fault(
        &self,
        evidence: &FinalizedSessionEvidence,
        signals: DurabilitySignals,
        evidence_chunks: Vec<WikiEvidenceChunk>,
        current: Vec<WikiCurrentExcerpt>,
        validation: KnowledgeValidationContext,
        fault: WikiFaultPoint,
    ) -> Result<WikiRunReport, WikiDriverError> {
        self.run_inner(
            evidence,
            signals,
            evidence_chunks,
            current,
            validation,
            None,
            Some(fault),
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn run_inner(
        &self,
        evidence: &FinalizedSessionEvidence,
        signals: DurabilitySignals,
        evidence_chunks: Vec<WikiEvidenceChunk>,
        current: Vec<WikiCurrentExcerpt>,
        mut validation: KnowledgeValidationContext,
        requested_rebind: Option<ModelBinding>,
        fault: Option<WikiFaultPoint>,
    ) -> Result<WikiRunReport, WikiDriverError> {
        let job = knowledge_job_for_session(evidence)?;
        validation.job_id = job.job_id.clone();
        let history = self.history(&job)?;
        if let Some(model_binding) = requested_rebind.clone() {
            let receipt = history.receipt().ok_or(WikiDriverError::Workflow)?;
            if receipt.outcome != KnowledgeReceiptOutcome::Pending {
                return Err(WikiDriverError::Workflow);
            }
            self.with_store(&job, |store| store.append_rebind(model_binding))?;
        }
        let rebind = requested_rebind.or_else(|| history.latest_rebind().cloned());
        if let Some(receipt) = history.receipt()
            && rebind.as_ref().is_none_or(|binding| {
                receipt.outcome != KnowledgeReceiptOutcome::Pending
                    || receipt.model_binding == *binding
            })
        {
            return Ok(WikiRunReport {
                events: Vec::new(),
                receipt: receipt.clone(),
            });
        }

        let decision = DurabilityGate::evaluate(signals);
        let mut workflow =
            MainModelWikiWorkflow::new(job.clone()).map_err(|_| WikiDriverError::Workflow)?;
        let mut effects = VecDeque::from(
            workflow
                .apply(
                    KnowledgeInput::Evaluated(decision),
                    &evidence_chunks,
                    &current,
                )
                .map_err(|_| WikiDriverError::Workflow)?,
        );
        let mut events = Vec::new();
        let mut receipt = None;
        while let Some(effect) = effects.pop_front() {
            match effect {
                KnowledgeEffect::PersistEvent(event) => {
                    self.with_store(&job, |store| store.append_event(event))?;
                }
                KnowledgeEffect::PublishEvent(event) => events.push(event),
                KnowledgeEffect::Generate(request) => {
                    let input = self
                        .generation_input(
                            &job,
                            &request,
                            &validation,
                            fault == Some(WikiFaultPoint::AfterGenerationRecord),
                        )
                        .await?;
                    effects.extend(
                        workflow
                            .apply(input, &evidence_chunks, &current)
                            .map_err(|_| WikiDriverError::Workflow)?,
                    );
                }
                KnowledgeEffect::Validate(patch) => {
                    let input = match KnowledgePatchValidator::validate(patch.clone(), &validation)
                    {
                        Ok(_) => KnowledgeInput::PatchValidated {
                            patch_hash: hash_vault_bytes(
                                &serde_json::to_vec(&patch)
                                    .map_err(|_| WikiDriverError::Workflow)?,
                            ),
                        },
                        Err(error) => KnowledgeInput::SemanticRejected {
                            code: guard_code(error).to_owned(),
                        },
                    };
                    effects.extend(
                        workflow
                            .apply(input, &evidence_chunks, &current)
                            .map_err(|_| WikiDriverError::Workflow)?,
                    );
                }
                KnowledgeEffect::Commit { patch, .. } => {
                    match self.knowledge.commit(&patch).await {
                        Ok(transaction_id) => {
                            if fault == Some(WikiFaultPoint::AfterCommit) {
                                return Err(WikiDriverError::FaultInjected);
                            }
                            effects.extend(
                                workflow
                                    .apply(
                                        KnowledgeInput::Committed { transaction_id },
                                        &evidence_chunks,
                                        &current,
                                    )
                                    .map_err(|_| WikiDriverError::Workflow)?,
                            );
                        }
                        Err(error) => {
                            let code = match error {
                                KnowledgeCommitError::Conflict => "knowledge_conflict",
                                KnowledgeCommitError::Unavailable => "knowledge_unavailable",
                                KnowledgeCommitError::Invalid => "knowledge_invalid",
                            };
                            effects.extend(
                                workflow
                                    .apply(
                                        KnowledgeInput::CommitPending {
                                            code: code.to_owned(),
                                        },
                                        &evidence_chunks,
                                        &current,
                                    )
                                    .map_err(|_| WikiDriverError::Workflow)?,
                            );
                        }
                    }
                }
                KnowledgeEffect::PersistReceipt(value) => {
                    self.with_store(&job, |store| store.append_receipt(value))?;
                }
                KnowledgeEffect::PublishReceipt(value) => {
                    if value.outcome == KnowledgeReceiptOutcome::Pending
                        && rebind
                            .as_ref()
                            .is_some_and(|binding| value.model_binding != *binding)
                    {
                        let model_binding = rebind.clone().ok_or(WikiDriverError::Workflow)?;
                        effects.extend(
                            workflow
                                .apply(
                                    KnowledgeInput::Rebind { model_binding },
                                    &evidence_chunks,
                                    &current,
                                )
                                .map_err(|_| WikiDriverError::Workflow)?,
                        );
                    } else {
                        receipt = Some(value);
                    }
                }
            }
        }
        Ok(WikiRunReport {
            events,
            receipt: receipt.ok_or(WikiDriverError::MissingReceipt)?,
        })
    }

    async fn generation_input(
        &self,
        job: &minimax_protocol::KnowledgeEvaluationJob,
        request: &minimax_core::WikiGenerationRequest,
        validation: &KnowledgeValidationContext,
        fault_after_record: bool,
    ) -> Result<KnowledgeInput, WikiDriverError> {
        let attempt = if request.schema_repair_only { 2 } else { 1 };
        let history = self.history(job)?;
        if let Some(stored) = history
            .terminal_generation(attempt, &request.job.model_binding)
            .cloned()
        {
            return stored_input(stored);
        }
        if history.generation_started(attempt, &request.job.model_binding) {
            let stored = StoredGeneration::Unavailable {
                attempt,
                model_binding: request.job.model_binding.clone(),
                code: "generation_outcome_unknown".to_owned(),
            };
            self.with_store(job, |store| store.append_generation(stored.clone()))?;
            return stored_input(stored);
        }
        self.with_store(job, |store| {
            store.append_generation(StoredGeneration::Started {
                attempt,
                model_binding: request.job.model_binding.clone(),
            })
        })?;
        let stored = match self.generation.generate(request).await {
            Ok(output) => match serde_json::from_str::<KnowledgePatch>(&output.raw_json)
                .ok()
                .and_then(|patch| patch.validate().ok())
            {
                Some(patch) => match KnowledgePatchValidator::validate(patch.clone(), validation) {
                    Ok(_) => StoredGeneration::Accepted {
                        attempt,
                        model_binding: request.job.model_binding.clone(),
                        patch,
                        usage: output.usage,
                    },
                    Err(error) => StoredGeneration::UnsafeRejected {
                        attempt,
                        model_binding: request.job.model_binding.clone(),
                        code: guard_code(error).to_owned(),
                        usage: output.usage,
                    },
                },
                None => StoredGeneration::SchemaRejected {
                    attempt,
                    model_binding: request.job.model_binding.clone(),
                    code: "schema_invalid".to_owned(),
                    usage: output.usage,
                },
            },
            Err(error) => StoredGeneration::Unavailable {
                attempt,
                model_binding: request.job.model_binding.clone(),
                code: match error {
                    WikiGenerationError::Unavailable => "model_unavailable",
                    WikiGenerationError::BindingMismatch => "model_binding_mismatch",
                    WikiGenerationError::Failed => "generation_failed",
                }
                .to_owned(),
            },
        };
        self.with_store(job, |store| store.append_generation(stored.clone()))?;
        if fault_after_record {
            return Err(WikiDriverError::FaultInjected);
        }
        stored_input(stored)
    }

    fn history(
        &self,
        job: &minimax_protocol::KnowledgeEvaluationJob,
    ) -> Result<minimax_vault::KnowledgeWorkflowHistory, WikiDriverError> {
        let vault = self.vault.open()?;
        Ok(KnowledgeWorkflowStore::open(&vault, job.clone())?
            .history()
            .clone())
    }

    fn with_store<R>(
        &self,
        job: &minimax_protocol::KnowledgeEvaluationJob,
        operation: impl FnOnce(&mut KnowledgeWorkflowStore<'_>) -> Result<R, VaultError>,
    ) -> Result<R, WikiDriverError> {
        let vault = self.vault.open()?;
        let mut store = KnowledgeWorkflowStore::open(&vault, job.clone())?;
        operation(&mut store).map_err(Into::into)
    }
}

fn stored_input(stored: StoredGeneration) -> Result<KnowledgeInput, WikiDriverError> {
    Ok(match stored {
        StoredGeneration::Accepted { patch, usage, .. } => {
            KnowledgeInput::GeneratedPatch { patch, usage }
        }
        StoredGeneration::SchemaRejected { code, usage, .. } => {
            KnowledgeInput::SchemaRejected { code, usage }
        }
        StoredGeneration::UnsafeRejected { code, usage, .. } => {
            KnowledgeInput::UnsafeGenerationRejected { code, usage }
        }
        StoredGeneration::Unavailable { code, .. } => {
            KnowledgeInput::GenerationUnavailable { code }
        }
        StoredGeneration::Started { .. } => return Err(WikiDriverError::Workflow),
    })
}

fn guard_code(error: KnowledgeGuardError) -> &'static str {
    match error {
        KnowledgeGuardError::InvalidPatch => "invalid_patch",
        KnowledgeGuardError::WrongJob => "wrong_job",
        KnowledgeGuardError::UnknownSource => "unknown_source",
        KnowledgeGuardError::SourceHashMismatch => "source_hash_mismatch",
        KnowledgeGuardError::DuplicatePage => "duplicate_page",
        KnowledgeGuardError::DuplicateTarget => "duplicate_target",
        KnowledgeGuardError::StaleHash => "stale_hash",
        KnowledgeGuardError::CurrentTruthConflict => "current_truth_conflict",
        KnowledgeGuardError::InvalidSupersession => "invalid_supersession",
        KnowledgeGuardError::SensitiveContent => "sensitive_content",
        KnowledgeGuardError::InjectionContent => "injection_content",
        KnowledgeGuardError::OversizedPatch => "oversized_patch",
    }
}

#[derive(Clone, Debug)]
pub struct VaultKnowledgePort {
    binding: ProjectVaultBinding,
}

impl VaultKnowledgePort {
    #[must_use]
    pub const fn new(binding: ProjectVaultBinding) -> Self {
        Self { binding }
    }

    fn commit_patch(&self, patch: &KnowledgePatch) -> Result<TransactionId, KnowledgeCommitError> {
        let patch_bytes = serde_json::to_vec(patch).map_err(|_| KnowledgeCommitError::Invalid)?;
        let patch_hash = hash_vault_bytes(&patch_bytes);
        let transaction_id = TransactionId::new(format!("wiki:{}", &patch_hash.as_str()[..24]))
            .map_err(|_| KnowledgeCommitError::Invalid)?;
        let vault = self.binding.open().map_err(map_vault_error)?;
        if wiki_transaction_exists(&vault, &transaction_id) {
            recover_wiki_transaction(&vault, &transaction_id).map_err(map_vault_error)?;
            return Ok(transaction_id);
        }
        let mut pages = read_wiki_pages(&vault).map_err(map_vault_error)?;
        let mut changes = Vec::new();
        for operation in &patch.operations {
            match operation {
                KnowledgeOperation::Create { page } => {
                    changes.push(WikiChange {
                        relative_path: page.relative_path.clone(),
                        expected_old_hash: None,
                        bytes: render_wiki_page(page).map_err(map_vault_error)?,
                    });
                    pages.insert(page.relative_path.clone(), page.clone());
                }
                KnowledgeOperation::Replace {
                    page,
                    expected_hash,
                } => {
                    changes.push(WikiChange {
                        relative_path: page.relative_path.clone(),
                        expected_old_hash: Some(expected_hash.clone()),
                        bytes: render_wiki_page(page).map_err(map_vault_error)?,
                    });
                    pages.insert(page.relative_path.clone(), page.clone());
                }
                KnowledgeOperation::Remove { .. } => return Err(KnowledgeCommitError::Invalid),
            }
        }
        let index_path = vault.root().join("wiki/index.md");
        let index_before =
            std::fs::read(&index_path).map_err(|_| KnowledgeCommitError::Unavailable)?;
        changes.push(WikiChange {
            relative_path: "wiki/index.md".to_owned(),
            expected_old_hash: Some(hash_vault_bytes(&index_before)),
            bytes: render_index(&pages),
        });
        let log_path = vault.root().join("log.md");
        let log_before = std::fs::read(&log_path).map_err(|_| KnowledgeCommitError::Unavailable)?;
        let mut log_after = log_before.clone();
        log_after.extend_from_slice(
            format!(
                "- wiki job {} committed as {}\n",
                patch.job_id.as_str(),
                transaction_id.as_str()
            )
            .as_bytes(),
        );
        changes.push(WikiChange {
            relative_path: "log.md".to_owned(),
            expected_old_hash: Some(hash_vault_bytes(&log_before)),
            bytes: log_after,
        });
        PreparedWikiTransaction::prepare(&vault, transaction_id.clone(), changes, 0)
            .map_err(map_vault_error)?
            .roll_forward()
            .map_err(map_vault_error)?;
        Ok(transaction_id)
    }
}

impl KnowledgePort for VaultKnowledgePort {
    fn commit<'a>(&'a self, patch: &'a KnowledgePatch) -> minimax_core::KnowledgeCommitFuture<'a> {
        Box::pin(async move { self.commit_patch(patch) })
    }
}

fn render_index(pages: &BTreeMap<String, KnowledgePage>) -> Vec<u8> {
    let mut output = String::from("# Project Wiki\n\n");
    for page in pages
        .values()
        .filter(|page| page.status == KnowledgePageStatus::Current)
    {
        let target = page
            .relative_path
            .strip_prefix("wiki/")
            .unwrap_or(&page.relative_path)
            .strip_suffix(".md")
            .unwrap_or(&page.relative_path);
        output.push_str(&format!("- [[{target}|{}]]\n", page.title));
    }
    output.into_bytes()
}

fn map_vault_error(error: VaultError) -> KnowledgeCommitError {
    match error {
        VaultError::Busy | VaultError::Io => KnowledgeCommitError::Unavailable,
        VaultError::Conflict | VaultError::Finalized => KnowledgeCommitError::Conflict,
        _ => KnowledgeCommitError::Invalid,
    }
}
