use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::future::Future;
use std::io::Write as _;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use minimax_core::{
    AgentBudget, ApprovalPort, CancellationFuture, CancellationPort, CompactionBudget,
    CompactionError, InvocationEffect, InvocationError, InvocationInput, InvocationMachine,
    InvocationRegistry, InvocationState, LocalCompactor, PermissionMode, RunEffect, RunInput,
    RunMachine, RunState, SafeTraceFact, SafeTraceRecorder, SessionCommand, SessionEffect,
    SessionSummary, ToolPort, WikiGenerationError, WikiGenerationFuture, WikiGenerationOutput,
    WikiGenerationPort, WikiGenerationRequest,
};
use minimax_protocol::{
    AgentLimits, AssistantToolCallBatch, CompactionId, CompactionRecord, ConversationItem,
    JournalRecord, MessageRole, ModelBinding, ModelMessage, OutputSettings, RecordId, RequestId,
    RuntimeErrorCode, RuntimeEvent, RuntimeEventV1, RuntimeFailure, SchemaVersion, SessionId,
    SessionRecord, SessionRecordV1, StreamEvent, TerminalOutcome, ToolDecision, ToolDecisionKind,
    ToolDefinition, ToolInvocation, ToolResult, ToolResultMessage, ToolTerminalStatus, TraceCode,
    TraceEntry, TurnId, TurnReceipt, TurnRequest, Usage,
};
use minimax_provider::{HttpProviderClient, ResolvedCredential};
use minimax_tools::BuiltinToolPort;
use minimax_tui::{ApprovalInput, EventRenderer};
use minimax_vault::{
    FinalizedSessionEvidence, ProjectVault, RuntimeStore, RuntimeStoreError, VaultError,
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

pub trait ProviderPort {
    fn rebind(&mut self, binding: &ModelBinding);

    fn stream<'a>(
        &'a mut self,
        request: &'a TurnRequest,
        cancellation: &'a CancellationToken,
        emit: &'a mut (dyn FnMut(StreamEvent) + Send),
    ) -> Pin<Box<dyn Future<Output = Result<(), RuntimeFailure>> + Send + 'a>>;
}

pub struct HttpProviderPort {
    client: HttpProviderClient,
    credential: ResolvedCredential,
    binding: ModelBinding,
}

impl HttpProviderPort {
    #[must_use]
    pub const fn new(
        client: HttpProviderClient,
        credential: ResolvedCredential,
        binding: ModelBinding,
    ) -> Self {
        Self {
            client,
            credential,
            binding,
        }
    }

    #[must_use]
    pub const fn binding(&self) -> &ModelBinding {
        &self.binding
    }
}

impl ProviderPort for HttpProviderPort {
    fn rebind(&mut self, binding: &ModelBinding) {
        self.binding.clone_from(binding);
    }

    fn stream<'a>(
        &'a mut self,
        request: &'a TurnRequest,
        cancellation: &'a CancellationToken,
        emit: &'a mut (dyn FnMut(StreamEvent) + Send),
    ) -> Pin<Box<dyn Future<Output = Result<(), RuntimeFailure>> + Send + 'a>> {
        Box::pin(async move {
            self.client
                .stream_with(request, self.credential.secret(), cancellation, |event| {
                    emit(event);
                    std::future::ready(())
                })
                .await
        })
    }
}

impl WikiGenerationPort for HttpProviderPort {
    fn generate<'a>(&'a self, request: &'a WikiGenerationRequest) -> WikiGenerationFuture<'a> {
        Box::pin(async move {
            if request.job.model_binding != self.binding {
                return Err(WikiGenerationError::BindingMismatch);
            }
            let input = wiki_generation_input(request).map_err(|_| WikiGenerationError::Failed)?;
            let turn = TurnRequest {
                session_id: SessionId::new(format!("wiki:{}", request.job.job_id.as_str()))
                    .map_err(|_| WikiGenerationError::Failed)?,
                turn_id: TurnId::new(format!("wiki:{}:turn", request.job.job_id.as_str()))
                    .map_err(|_| WikiGenerationError::Failed)?,
                request_id: RequestId::new(format!("wiki:{}:request", request.job.job_id.as_str()))
                    .map_err(|_| WikiGenerationError::Failed)?,
                provider_id: request.job.model_binding.provider_id.clone(),
                model_id: request.job.model_binding.model_id.clone(),
                protocol: request.job.model_binding.protocol,
                messages: vec![
                    ModelMessage {
                        role: MessageRole::System,
                        content: wiki_system_instruction(request.schema_repair_only),
                    }
                    .into(),
                    ModelMessage {
                        role: MessageRole::User,
                        content: input,
                    }
                    .into(),
                ],
                tools: Vec::new(),
                agent_limits: None,
                output: OutputSettings::new(request.job.max_output_tokens)
                    .map_err(|_| WikiGenerationError::Failed)?,
            }
            .validate()
            .map_err(|_| WikiGenerationError::Failed)?;
            let cancellation = CancellationToken::new();
            let events = self
                .client
                .stream_collect(&turn, self.credential.secret(), &cancellation)
                .await
                .map_err(|_| WikiGenerationError::Unavailable)?;
            let mut raw_json = String::new();
            let mut usage = Usage::default();
            let mut completed = false;
            for event in events {
                match event {
                    StreamEvent::VisibleTextDelta { delta } => raw_json.push_str(&delta),
                    StreamEvent::Usage { usage: value } => usage = value,
                    StreamEvent::ReasoningFiltered => {}
                    StreamEvent::Terminal {
                        outcome: TerminalOutcome::Completed,
                    } => completed = true,
                    StreamEvent::Terminal { .. } => return Err(WikiGenerationError::Unavailable),
                    StreamEvent::ToolCallFragments { .. } => {
                        return Err(WikiGenerationError::Failed);
                    }
                }
            }
            if !completed || raw_json.trim().is_empty() {
                return Err(WikiGenerationError::Failed);
            }
            Ok(WikiGenerationOutput { raw_json, usage })
        })
    }
}

fn wiki_system_instruction(schema_repair_only: bool) -> String {
    let mode = if schema_repair_only {
        "Repair the previous schema failure."
    } else {
        "Summarize only durable project knowledge."
    };
    format!(
        "{mode} Return exactly one JSON object and no Markdown. The object must use schemaVersion=1, the supplied jobId, and a non-empty operations array. Each create/replace page must contain schemaVersion, pageId, topicId, relativePath under wiki/, title, status, sources using supplied sourceId/sourceHash, and body. Never invent evidence, credentials, private reasoning, commands, or facts."
    )
}

fn wiki_generation_input(request: &WikiGenerationRequest) -> Result<String, serde_json::Error> {
    let evidence = request
        .evidence
        .iter()
        .map(|chunk| {
            serde_json::json!({
                "sourceId": chunk.source_id.as_str(),
                "sourceHash": chunk.source_hash.as_str(),
                "label": chunk.label,
                "content": chunk.content,
            })
        })
        .collect::<Vec<_>>();
    let current = request
        .current
        .iter()
        .map(|page| {
            serde_json::json!({
                "pageId": page.page_id.as_str(),
                "topicId": page.topic_id.as_str(),
                "title": page.title,
                "content": page.content,
                "contentHash": page.content_hash.as_str(),
            })
        })
        .collect::<Vec<_>>();
    serde_json::to_string(&serde_json::json!({
        "jobId": request.job.job_id.as_str(),
        "sourceId": request.job.source_id.as_str(),
        "sourceHash": request.job.source_hash.as_str(),
        "evidence": evidence,
        "currentWiki": current,
        "schemaRepairOnly": request.schema_repair_only,
    }))
}

#[derive(Clone, Copy, Debug, Default)]
pub struct HeadlessApprovalPort;

impl ApprovalPort for HeadlessApprovalPort {
    fn decide<'a>(&'a self, invocation: &'a ToolInvocation) -> minimax_core::ApprovalFuture<'a> {
        Box::pin(async move {
            ToolDecision {
                schema_version: SchemaVersion,
                call_id: invocation.call.call_id.clone(),
                decision: ToolDecisionKind::Rejected,
                code: "approval_unavailable".to_owned(),
            }
        })
    }
}

pub struct InteractiveApprovalPort {
    input: Arc<dyn ApprovalInput>,
}

impl InteractiveApprovalPort {
    #[must_use]
    pub fn new(input: Box<dyn ApprovalInput>) -> Self {
        Self {
            input: Arc::from(input),
        }
    }
}

impl ApprovalPort for InteractiveApprovalPort {
    fn decide<'a>(&'a self, invocation: &'a ToolInvocation) -> minimax_core::ApprovalFuture<'a> {
        let input = Arc::clone(&self.input);
        let invocation = invocation.clone();
        let call_id = invocation.call.call_id.clone();
        Box::pin(async move {
            match tokio::task::spawn_blocking(move || decide_interactively(&*input, &invocation))
                .await
            {
                Ok(decision) => decision,
                Err(_) => ToolDecision {
                    schema_version: SchemaVersion,
                    call_id,
                    decision: ToolDecisionKind::Rejected,
                    code: "approval_task_failed".to_owned(),
                },
            }
        })
    }
}

fn decide_interactively(input: &dyn ApprovalInput, invocation: &ToolInvocation) -> ToolDecision {
    let decision = if !input.is_interactive() {
        (ToolDecisionKind::Rejected, "approval_noninteractive")
    } else if write_approval_prompt(invocation).is_err() {
        (ToolDecisionKind::Rejected, "approval_output_failed")
    } else {
        match input.read_approval() {
            Ok(Some(answer)) if answer.trim_end_matches(['\r', '\n']) == "yes" => {
                (ToolDecisionKind::Approved, "user_approved")
            }
            Ok(Some(answer)) if answer.trim_end_matches(['\r', '\n']) == "no" => {
                (ToolDecisionKind::Rejected, "user_rejected")
            }
            Ok(Some(_)) => (ToolDecisionKind::Rejected, "approval_invalid"),
            Ok(None) => (ToolDecisionKind::Rejected, "approval_eof"),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {
                (ToolDecisionKind::Rejected, "approval_interrupted")
            }
            Err(_) => (ToolDecisionKind::Rejected, "approval_input_failed"),
        }
    };
    ToolDecision {
        schema_version: SchemaVersion,
        call_id: invocation.call.call_id.clone(),
        decision: decision.0,
        code: decision.1.to_owned(),
    }
}

fn write_approval_prompt(invocation: &ToolInvocation) -> std::io::Result<()> {
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(EventRenderer::approval_request(invocation).as_bytes())?;
    stdout.flush()
}

#[derive(Clone, Copy, Debug, Default)]
struct ToolUnavailable;

impl ToolPort for ToolUnavailable {
    fn preflight(
        &self,
        invocation: &ToolInvocation,
        _cancellation: &dyn CancellationPort,
    ) -> Result<(), ToolResult> {
        Err(ToolResult {
            schema_version: SchemaVersion,
            call_id: invocation.call.call_id.clone(),
            tool_name: invocation.call.name.clone(),
            status: ToolTerminalStatus::Failed,
            code: "tool_unavailable".to_owned(),
            output: None,
        })
    }

    fn execute<'a>(
        &'a self,
        invocation: &'a ToolInvocation,
        _sandbox_policy: minimax_core::ToolSandboxPolicy,
        _cancellation: &'a dyn CancellationPort,
    ) -> minimax_core::ToolFuture<'a> {
        Box::pin(async move {
            ToolResult {
                schema_version: SchemaVersion,
                call_id: invocation.call.call_id.clone(),
                tool_name: invocation.call.name.clone(),
                status: ToolTerminalStatus::Failed,
                code: "tool_unavailable".to_owned(),
                output: None,
            }
        })
    }
}

struct DriverCancellation<'a>(&'a CancellationToken);

impl CancellationPort for DriverCancellation<'_> {
    fn is_cancelled(&self) -> bool {
        self.0.is_cancelled()
    }

    fn cancelled<'a>(&'a self) -> CancellationFuture<'a> {
        Box::pin(self.0.cancelled())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DriverIds {
    prefix: String,
    next: u64,
    base_unix_ms: u64,
}

impl DriverIds {
    #[must_use]
    pub fn new(prefix: impl Into<String>, base_unix_ms: u64) -> Self {
        Self {
            prefix: prefix.into(),
            next: 0,
            base_unix_ms,
        }
    }

    #[must_use]
    pub fn system() -> Self {
        let duration = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        Self::new(
            format!("{}-{}", std::process::id(), duration.as_nanos()),
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX),
        )
    }

    fn next_raw(&mut self, kind: &str) -> String {
        self.next = self.next.saturating_add(1);
        format!("{kind}-{}-{}", self.prefix, self.next)
    }

    const fn now_unix_ms(&self) -> u64 {
        self.base_unix_ms.saturating_add(self.next)
    }

    fn record_id(&mut self) -> Result<RecordId, DriverError> {
        RecordId::new(self.next_raw("record")).map_err(DriverError::Runtime)
    }

    fn session_id(&mut self) -> Result<SessionId, DriverError> {
        SessionId::new(self.next_raw("session"))
            .map_err(RuntimeErrorCode::from)
            .map_err(DriverError::Runtime)
    }

    fn turn_id(&mut self) -> Result<TurnId, DriverError> {
        TurnId::new(self.next_raw("turn"))
            .map_err(RuntimeErrorCode::from)
            .map_err(DriverError::Runtime)
    }

    fn request_id(&mut self) -> Result<RequestId, DriverError> {
        RequestId::new(self.next_raw("request")).map_err(DriverError::Runtime)
    }

    fn compaction_id(&mut self) -> Result<CompactionId, DriverError> {
        CompactionId::new(self.next_raw("compaction")).map_err(DriverError::Runtime)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DriverError {
    Runtime(RuntimeErrorCode),
    Store(RuntimeStoreError),
    Compaction(CompactionError),
    Invocation(InvocationError),
}

impl fmt::Display for DriverError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Runtime(error) => error.fmt(formatter),
            Self::Store(error) => error.fmt(formatter),
            Self::Compaction(error) => error.fmt(formatter),
            Self::Invocation(error) => write!(formatter, "tool invocation error: {error:?}"),
        }
    }
}

impl std::error::Error for DriverError {}

impl From<RuntimeStoreError> for DriverError {
    fn from(value: RuntimeStoreError) -> Self {
        Self::Store(value)
    }
}

impl From<CompactionError> for DriverError {
    fn from(value: CompactionError) -> Self {
        Self::Compaction(value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunReport {
    pub events: Vec<RuntimeEventV1>,
    pub tool_results: Vec<ToolResult>,
    pub receipt: TurnReceipt,
}

pub struct RuntimeDriver<P> {
    store: RuntimeStore,
    provider: P,
    cancellation: CancellationToken,
    ids: DriverIds,
    approval: Box<dyn ApprovalPort>,
    tools: Box<dyn ToolPort>,
    permission_mode: PermissionMode,
    tool_definitions: Vec<ToolDefinition>,
    agent_limits: AgentLimits,
}

impl<P: ProviderPort> RuntimeDriver<P> {
    pub fn open(
        project_root: impl AsRef<Path>,
        binding: ModelBinding,
        provider: P,
        ids: DriverIds,
    ) -> Result<Self, DriverError> {
        Self::open_with_agent_ports(
            project_root,
            binding,
            provider,
            ids,
            Box::new(HeadlessApprovalPort),
            Box::new(ToolUnavailable),
            Vec::new(),
            AgentLimits::default(),
        )
    }

    pub fn open_with_builtin_tools(
        project_root: impl AsRef<Path>,
        binding: ModelBinding,
        provider: P,
        ids: DriverIds,
        approval: Box<dyn ApprovalPort>,
    ) -> Result<Self, DriverError> {
        let project_root = project_root.as_ref().to_path_buf();
        let tools = BuiltinToolPort::production(&project_root)
            .map_err(|_| DriverError::Runtime(RuntimeErrorCode::Configuration))?;
        let definitions = BuiltinToolPort::definitions()
            .map_err(|_| DriverError::Runtime(RuntimeErrorCode::Configuration))?;
        Self::open_with_agent_ports(
            project_root,
            binding,
            provider,
            ids,
            approval,
            Box::new(tools),
            definitions,
            AgentLimits::default(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn open_with_agent_ports(
        project_root: impl AsRef<Path>,
        binding: ModelBinding,
        provider: P,
        ids: DriverIds,
        approval: Box<dyn ApprovalPort>,
        tools: Box<dyn ToolPort>,
        tool_definitions: Vec<ToolDefinition>,
        agent_limits: AgentLimits,
    ) -> Result<Self, DriverError> {
        agent_limits.validate().map_err(DriverError::Runtime)?;
        let mut names = std::collections::BTreeSet::new();
        for definition in tool_definitions.iter().cloned() {
            let definition = definition
                .validate()
                .map_err(|_| DriverError::Runtime(RuntimeErrorCode::Configuration))?;
            if !names.insert(definition.name) {
                return Err(DriverError::Runtime(RuntimeErrorCode::Configuration));
            }
        }
        let store = RuntimeStore::open(project_root).map_err(DriverError::Store)?;
        let mut driver = Self {
            store,
            provider,
            cancellation: CancellationToken::new(),
            ids,
            approval,
            tools,
            permission_mode: PermissionMode::Confirm,
            tool_definitions,
            agent_limits,
        };
        let active_is_finalized = driver
            .store
            .machine()
            .active_session()
            .is_some_and(|session| driver.store.session_is_finalized(&session.session_id));
        if driver.store.machine().active_session().is_none() || active_is_finalized {
            driver.create_session(binding)?;
        }
        let active_binding = driver
            .active_binding()
            .ok_or(DriverError::Runtime(RuntimeErrorCode::Recovery))?;
        driver.provider.rebind(&active_binding);
        Ok(driver)
    }

    #[must_use]
    pub const fn permission_mode(&self) -> PermissionMode {
        self.permission_mode
    }

    pub fn set_permission_mode(&mut self, mode: PermissionMode) {
        self.permission_mode = mode;
    }

    #[must_use]
    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancellation.clone()
    }

    #[must_use]
    pub fn active_session_id(&self) -> Option<SessionId> {
        self.store
            .machine()
            .active_session()
            .map(|session| session.session_id.clone())
    }

    #[must_use]
    pub fn session(&self, session_id: &SessionId) -> Option<SessionRecord> {
        self.store.machine().sessions().get(session_id).cloned()
    }

    #[must_use]
    pub fn active_binding(&self) -> Option<ModelBinding> {
        self.store
            .machine()
            .active_session()
            .map(|session| session.binding.clone())
    }

    #[must_use]
    pub fn active_trace_entries(&self) -> Vec<TraceEntry> {
        self.store
            .machine()
            .active_session()
            .map_or_else(Vec::new, |session| {
                self.store.trace_entries(&session.session_id)
            })
    }

    #[must_use]
    pub fn latest_retryable_turn_id(&self) -> Option<TurnId> {
        self.store
            .machine()
            .active_session()
            .and_then(|session| session.turns.last())
            .filter(|turn| turn.status.is_terminal())
            .map(|turn| turn.turn_id.clone())
    }

    pub fn finalize_active_session(
        &self,
        vault: &ProjectVault,
        finalized_at_unix_ms: u64,
    ) -> Result<FinalizedSessionEvidence, VaultError> {
        let session_id = self
            .active_session_id()
            .ok_or(VaultError::SessionNotFound)?;
        self.store
            .finalize_session(vault, &session_id, finalized_at_unix_ms)
    }

    #[must_use]
    pub const fn provider(&self) -> &P {
        &self.provider
    }

    pub fn create_session(&mut self, binding: ModelBinding) -> Result<SessionId, DriverError> {
        let record_id = self.ids.record_id()?;
        let session_id = self.ids.session_id()?;
        let now_unix_ms = self.ids.now_unix_ms();
        self.store.apply_command(SessionCommand::Create {
            record_id,
            session_id: session_id.clone(),
            binding: binding.clone(),
            now_unix_ms,
        })?;
        self.provider.rebind(&binding);
        Ok(session_id)
    }

    pub fn list_sessions(&mut self) -> Result<Vec<SessionSummary>, DriverError> {
        let effects = self.store.apply_command(SessionCommand::List)?;
        effects
            .into_iter()
            .find_map(|effect| match effect {
                SessionEffect::Listed(sessions) => Some(sessions),
                _ => None,
            })
            .ok_or(DriverError::Runtime(RuntimeErrorCode::Recovery))
    }

    pub fn resume(&mut self, session_id: SessionId) -> Result<(), DriverError> {
        let record_id = self.ids.record_id()?;
        let now_unix_ms = self.ids.now_unix_ms();
        self.store.apply_command(SessionCommand::Resume {
            record_id,
            session_id,
            now_unix_ms,
        })?;
        let binding = self
            .active_binding()
            .ok_or(DriverError::Runtime(RuntimeErrorCode::Recovery))?;
        self.provider.rebind(&binding);
        Ok(())
    }

    pub async fn run_prompt(
        &mut self,
        user_input: impl Into<String>,
        max_output_tokens: u32,
    ) -> Result<RunReport, DriverError> {
        self.run_prompt_with(user_input, max_output_tokens, |_| {})
            .await
    }

    pub async fn run_agent(
        &mut self,
        user_input: impl Into<String>,
        max_output_tokens: u32,
    ) -> Result<RunReport, DriverError> {
        self.run_agent_with(user_input, max_output_tokens, |_| {})
            .await
    }

    pub async fn run_agent_with<F>(
        &mut self,
        user_input: impl Into<String>,
        max_output_tokens: u32,
        mut publish: F,
    ) -> Result<RunReport, DriverError>
    where
        F: FnMut(&RuntimeEventV1),
    {
        let record_id = self.ids.record_id()?;
        let turn_id = self.ids.turn_id()?;
        let request_id = self.ids.request_id()?;
        let now_unix_ms = self.ids.now_unix_ms();
        let effects = self.store.apply_command(SessionCommand::Continue {
            record_id,
            turn_id,
            request_id,
            user_input: user_input.into(),
            max_output_tokens,
            now_unix_ms,
        })?;
        let mut request = start_request(effects)?;
        request.tools.clone_from(&self.tool_definitions);
        request.agent_limits = Some(self.agent_limits);
        let request = request.validate().map_err(DriverError::Runtime)?;
        self.drive_agent_request(request, &mut publish).await
    }

    pub async fn run_prompt_with<F>(
        &mut self,
        user_input: impl Into<String>,
        max_output_tokens: u32,
        mut publish: F,
    ) -> Result<RunReport, DriverError>
    where
        F: FnMut(&RuntimeEventV1),
    {
        let record_id = self.ids.record_id()?;
        let turn_id = self.ids.turn_id()?;
        let request_id = self.ids.request_id()?;
        let now_unix_ms = self.ids.now_unix_ms();
        let effects = self.store.apply_command(SessionCommand::Continue {
            record_id,
            turn_id,
            request_id,
            user_input: user_input.into(),
            max_output_tokens,
            now_unix_ms,
        })?;
        let request = start_request(effects)?;
        self.drive_request(request, &mut publish).await
    }

    pub async fn retry_turn(
        &mut self,
        source_turn_id: TurnId,
        max_output_tokens: u32,
    ) -> Result<RunReport, DriverError> {
        let record_id = self.ids.record_id()?;
        let new_turn_id = self.ids.turn_id()?;
        let request_id = self.ids.request_id()?;
        let now_unix_ms = self.ids.now_unix_ms();
        let effects = self.store.apply_command(SessionCommand::Retry {
            record_id,
            source_turn_id,
            new_turn_id,
            request_id,
            max_output_tokens,
            now_unix_ms,
        })?;
        let request = start_request(effects)?;
        let mut ignore = |_: &RuntimeEventV1| {};
        self.drive_request(request, &mut ignore).await
    }

    pub fn compact_active(
        &mut self,
        budget: CompactionBudget,
    ) -> Result<CompactionRecord, DriverError> {
        let session = self
            .store
            .machine()
            .active_session()
            .cloned()
            .ok_or(DriverError::Runtime(RuntimeErrorCode::Recovery))?;
        let compaction = LocalCompactor::compact(&session, self.ids.compaction_id()?, budget)?;
        let envelope = SessionRecordV1::new(
            self.ids.record_id()?,
            JournalRecord::CompactionStored {
                session_id: session.session_id,
                compaction: Box::new(compaction.clone()),
                stored_at_unix_ms: self.ids.now_unix_ms(),
            },
        );
        self.store.append(envelope)?;
        record_trace(
            &mut self.store,
            &mut self.ids,
            TraceCode::CompactionCompleted,
            BTreeMap::from([
                (
                    "compaction_id".to_owned(),
                    SafeTraceFact::String(compaction.compaction_id.as_str().to_owned()),
                ),
                (
                    "covered_through_turn_id".to_owned(),
                    SafeTraceFact::String(compaction.covered_through_turn_id.as_str().to_owned()),
                ),
                (
                    "before_tokens".to_owned(),
                    SafeTraceFact::U64(compaction.before_estimated_tokens),
                ),
                (
                    "after_tokens".to_owned(),
                    SafeTraceFact::U64(compaction.after_estimated_tokens),
                ),
            ]),
        )?;
        Ok(compaction)
    }

    async fn drive_request(
        &mut self,
        request: TurnRequest,
        publish: &mut dyn FnMut(&RuntimeEventV1),
    ) -> Result<RunReport, DriverError> {
        let mut machine = RunMachine::new();
        let mut events = Vec::new();
        let mut assistant = String::new();
        let run_cancellation = self.cancellation.child_token();
        apply_runtime_input(
            &mut self.store,
            &mut self.ids,
            &mut machine,
            RunInput::Begin(request.clone()),
            &mut events,
            &mut assistant,
            &run_cancellation,
            publish,
        )?;
        self.stream_provider_round(
            &request,
            &mut machine,
            &mut events,
            &mut assistant,
            &run_cancellation,
            publish,
        )
        .await?;

        let RunState::Terminal { receipt } = machine.state() else {
            return Err(DriverError::Runtime(RuntimeErrorCode::Recovery));
        };
        Ok(RunReport {
            events,
            tool_results: Vec::new(),
            receipt: receipt.clone(),
        })
    }

    async fn drive_agent_request(
        &mut self,
        mut request: TurnRequest,
        publish: &mut dyn FnMut(&RuntimeEventV1),
    ) -> Result<RunReport, DriverError> {
        let mut machine = RunMachine::new();
        let mut events = Vec::new();
        let mut tool_results = Vec::new();
        let mut assistant = String::new();
        let run_cancellation = self.cancellation.child_token();
        let mut budget = AgentBudget::new(self.agent_limits, budget_now_unix_ms())
            .map_err(|_| DriverError::Runtime(RuntimeErrorCode::Configuration))?;
        let mut registry = InvocationRegistry::default();
        apply_runtime_input(
            &mut self.store,
            &mut self.ids,
            &mut machine,
            RunInput::Begin(request.clone()),
            &mut events,
            &mut assistant,
            &run_cancellation,
            publish,
        )?;

        loop {
            if run_cancellation.is_cancelled() {
                apply_runtime_input(
                    &mut self.store,
                    &mut self.ids,
                    &mut machine,
                    RunInput::Cancel,
                    &mut events,
                    &mut assistant,
                    &run_cancellation,
                    publish,
                )?;
                let RunState::Terminal { receipt } = machine.state() else {
                    return Err(DriverError::Runtime(RuntimeErrorCode::Recovery));
                };
                return Ok(RunReport {
                    events,
                    tool_results,
                    receipt: receipt.clone(),
                });
            }
            if budget.consume_provider_round(budget_now_unix_ms()).is_err() {
                apply_runtime_input(
                    &mut self.store,
                    &mut self.ids,
                    &mut machine,
                    RunInput::ProviderFailed(RuntimeFailure::new(
                        RuntimeErrorCode::AgentBudgetExhausted,
                    )),
                    &mut events,
                    &mut assistant,
                    &run_cancellation,
                    publish,
                )?;
                let RunState::Terminal { receipt } = machine.state() else {
                    return Err(DriverError::Runtime(RuntimeErrorCode::Recovery));
                };
                return Ok(RunReport {
                    events,
                    tool_results,
                    receipt: receipt.clone(),
                });
            }
            self.stream_provider_round(
                &request,
                &mut machine,
                &mut events,
                &mut assistant,
                &run_cancellation,
                publish,
            )
            .await?;
            match machine.state() {
                RunState::Terminal { receipt } => {
                    return Ok(RunReport {
                        events,
                        tool_results,
                        receipt: receipt.clone(),
                    });
                }
                RunState::AwaitingTools { invocations, .. } => {
                    let invocations = invocations.clone();
                    let mut results = Vec::with_capacity(invocations.len());
                    for invocation in &invocations {
                        registry
                            .register(invocation)
                            .map_err(DriverError::Invocation)?;
                        let result = self
                            .complete_invocation(invocation.clone(), &mut budget, &run_cancellation)
                            .await?;
                        results.push(result);
                    }
                    tool_results.extend(results.iter().cloned());
                    request.messages.push(ConversationItem::AssistantToolCalls(
                        AssistantToolCallBatch {
                            tool_calls: invocations
                                .iter()
                                .map(|invocation| invocation.call.clone())
                                .collect(),
                        },
                    ));
                    request
                        .messages
                        .extend(results.into_iter().map(|tool_result| {
                            ConversationItem::ToolResult(ToolResultMessage { tool_result })
                        }));
                    request = request.validate().map_err(DriverError::Runtime)?;
                    apply_runtime_input(
                        &mut self.store,
                        &mut self.ids,
                        &mut machine,
                        RunInput::ContinueAfterTools(request.clone()),
                        &mut events,
                        &mut assistant,
                        &run_cancellation,
                        publish,
                    )?;
                }
                RunState::Idle | RunState::Running { .. } => {
                    return Err(DriverError::Runtime(RuntimeErrorCode::Recovery));
                }
            }
        }
    }

    async fn complete_invocation(
        &mut self,
        invocation: ToolInvocation,
        budget: &mut AgentBudget,
        cancellation: &CancellationToken,
    ) -> Result<ToolResult, DriverError> {
        let (mut machine, initial) = InvocationMachine::request(invocation.clone());
        let _ = self
            .apply_invocation_effects(&mut machine, initial, budget, cancellation)
            .await?;
        let preflight = if cancellation.is_cancelled() {
            machine
                .apply(InvocationInput::Cancel)
                .map_err(DriverError::Invocation)?
        } else {
            let cancellation = DriverCancellation(cancellation);
            match self.tools.preflight(&invocation, &cancellation) {
                Ok(()) => machine
                    .apply(InvocationInput::PreflightAllowed {
                        permission_mode: self.permission_mode,
                    })
                    .map_err(DriverError::Invocation)?,
                Err(result) => {
                    let result = normalize_preflight_result(&invocation, result);
                    machine
                        .apply(InvocationInput::PreflightDenied { result })
                        .map_err(DriverError::Invocation)?
                }
            }
        };
        self.apply_invocation_effects(&mut machine, preflight, budget, cancellation)
            .await?
            .ok_or(DriverError::Runtime(RuntimeErrorCode::Recovery))
    }

    async fn apply_invocation_effects(
        &mut self,
        machine: &mut InvocationMachine,
        effects: Vec<InvocationEffect>,
        budget: &mut AgentBudget,
        cancellation: &CancellationToken,
    ) -> Result<Option<ToolResult>, DriverError> {
        let mut pending = VecDeque::from(effects);
        let mut terminal = None;
        while let Some(effect) = pending.pop_front() {
            match effect {
                InvocationEffect::PersistRequested(invocation) => {
                    self.store
                        .apply_command(SessionCommand::RecordToolRequested {
                            record_id: self.ids.record_id()?,
                            turn_id: running_turn_id(&self.store)?,
                            invocation,
                            now_unix_ms: self.ids.now_unix_ms(),
                        })?;
                }
                InvocationEffect::RequestApproval(invocation) => {
                    let next = tokio::select! {
                        decision = self.approval.decide(&invocation) => {
                            let decision = normalize_decision(&invocation, decision);
                            machine.apply(InvocationInput::Decision {
                                decision,
                                permission_mode: self.permission_mode,
                            })
                        }
                        () = cancellation.cancelled() => machine.apply(InvocationInput::Cancel),
                    }
                    .map_err(DriverError::Invocation)?;
                    pending.extend(next);
                }
                InvocationEffect::PersistDecision(decision) => {
                    self.store
                        .apply_command(SessionCommand::RecordToolDecision {
                            record_id: self.ids.record_id()?,
                            turn_id: running_turn_id(&self.store)?,
                            decision,
                            now_unix_ms: self.ids.now_unix_ms(),
                        })?;
                    if matches!(machine.state(), InvocationState::Approved { .. }) {
                        let next = match budget.consume_tool_call(budget_now_unix_ms()) {
                            Ok(()) => machine.apply(InvocationInput::Start),
                            Err(error) => {
                                let result = budget.failure_result(machine.invocation(), error);
                                machine.apply(InvocationInput::PreStartFailed { result })
                            }
                        }
                        .map_err(DriverError::Invocation)?;
                        pending.extend(next);
                    }
                }
                InvocationEffect::PersistStarted(call_id) => {
                    self.store
                        .apply_command(SessionCommand::RecordToolStarted {
                            record_id: self.ids.record_id()?,
                            turn_id: running_turn_id(&self.store)?,
                            call_id,
                            now_unix_ms: self.ids.now_unix_ms(),
                        })?;
                }
                InvocationEffect::Execute {
                    invocation,
                    sandbox_policy,
                } => {
                    if cancellation.is_cancelled() {
                        pending.extend(
                            machine
                                .apply(InvocationInput::Cancel)
                                .map_err(DriverError::Invocation)?,
                        );
                        continue;
                    }
                    let cancellation = DriverCancellation(cancellation);
                    let result = self
                        .tools
                        .execute(&invocation, sandbox_policy, &cancellation)
                        .await;
                    let result = normalize_execution_result(&invocation, result);
                    let result_bytes = serde_json::to_vec(&result)
                        .map_err(|_| DriverError::Runtime(RuntimeErrorCode::Recovery))?
                        .len();
                    let result =
                        match budget.consume_result_bytes(result_bytes, budget_now_unix_ms()) {
                            Ok(()) => result,
                            Err(error) => budget.failure_result(&invocation, error),
                        };
                    pending.extend(
                        machine
                            .apply(InvocationInput::Complete { result })
                            .map_err(DriverError::Invocation)?,
                    );
                }
                InvocationEffect::PersistTerminal(result) => {
                    self.store
                        .apply_command(SessionCommand::RecordToolTerminal {
                            record_id: self.ids.record_id()?,
                            turn_id: running_turn_id(&self.store)?,
                            result,
                            now_unix_ms: self.ids.now_unix_ms(),
                        })?;
                }
                InvocationEffect::PublishTerminal(result) => {
                    terminal = Some(result);
                }
            }
        }
        Ok(terminal)
    }

    async fn stream_provider_round(
        &mut self,
        request: &TurnRequest,
        machine: &mut RunMachine,
        events: &mut Vec<RuntimeEventV1>,
        assistant: &mut String,
        run_cancellation: &CancellationToken,
        publish: &mut dyn FnMut(&RuntimeEventV1),
    ) -> Result<(), DriverError> {
        let (sender, mut receiver) = mpsc::unbounded_channel();
        let mut emit = move |event| {
            let _ = sender.send(event);
        };
        let provider_future = self.provider.stream(request, run_cancellation, &mut emit);
        tokio::pin!(provider_future);
        let mut provider_observed = false;

        let provider_result = loop {
            tokio::select! {
                event = receiver.recv() => {
                    if let Some(event) = event
                        && matches!(machine.state(), RunState::Running { .. }) {
                        if !provider_observed {
                            record_provider_connected(&mut self.store, &mut self.ids, request)?;
                            provider_observed = true;
                        }
                        apply_runtime_input(
                            &mut self.store,
                            &mut self.ids,
                            machine,
                            RunInput::ProviderEvent(event),
                            events,
                            assistant,
                            run_cancellation,
                            publish,
                        )?;
                    }
                }
                result = &mut provider_future => break result,
            }
        };

        while let Ok(event) = receiver.try_recv() {
            if !matches!(machine.state(), RunState::Running { .. }) {
                break;
            }
            if !provider_observed {
                record_provider_connected(&mut self.store, &mut self.ids, request)?;
                provider_observed = true;
            }
            apply_runtime_input(
                &mut self.store,
                &mut self.ids,
                machine,
                RunInput::ProviderEvent(event),
                events,
                assistant,
                run_cancellation,
                publish,
            )?;
        }

        if matches!(machine.state(), RunState::Running { .. }) {
            let input = match provider_result {
                Ok(()) => RunInput::ProviderEof,
                Err(failure) if failure.code == RuntimeErrorCode::Interrupted => RunInput::Cancel,
                Err(failure) => RunInput::ProviderFailed(failure),
            };
            apply_runtime_input(
                &mut self.store,
                &mut self.ids,
                machine,
                input,
                events,
                assistant,
                run_cancellation,
                publish,
            )?;
        }
        Ok(())
    }
}

fn start_request(effects: Vec<SessionEffect>) -> Result<TurnRequest, DriverError> {
    effects
        .into_iter()
        .find_map(|effect| match effect {
            SessionEffect::StartTurn(request) => Some(request),
            _ => None,
        })
        .ok_or(DriverError::Runtime(RuntimeErrorCode::Recovery))
}

#[allow(clippy::too_many_arguments)]
fn apply_runtime_input(
    store: &mut RuntimeStore,
    ids: &mut DriverIds,
    machine: &mut RunMachine,
    input: RunInput,
    published: &mut Vec<RuntimeEventV1>,
    assistant: &mut String,
    cancellation: &CancellationToken,
    publish: &mut dyn FnMut(&RuntimeEventV1),
) -> Result<(), DriverError> {
    let effects = machine.apply(input).map_err(DriverError::Runtime)?;
    let receipt = effects.iter().find_map(|effect| match effect {
        RunEffect::Finalize(receipt) => Some(receipt.clone()),
        _ => None,
    });
    for effect in effects {
        match effect {
            RunEffect::Persist(event) => match &event.event {
                RuntimeEvent::TurnStarted { turn_id, .. } => {
                    record_trace(
                        store,
                        ids,
                        TraceCode::TurnStarted,
                        BTreeMap::from([(
                            "turn_id".to_owned(),
                            SafeTraceFact::String(turn_id.as_str().to_owned()),
                        )]),
                    )?;
                }
                RuntimeEvent::VisibleTextDelta { delta } => {
                    let turn_id = running_turn_id(store)?;
                    store.apply_command(SessionCommand::RecordDelta {
                        record_id: ids.record_id()?,
                        turn_id,
                        delta: delta.clone(),
                        now_unix_ms: ids.now_unix_ms(),
                    })?;
                    assistant.push_str(delta);
                }
                RuntimeEvent::Terminal { outcome } => {
                    match outcome {
                        minimax_protocol::RuntimeTerminalOutcome::Failed { failure } => {
                            let session = store
                                .machine()
                                .active_session()
                                .ok_or(DriverError::Runtime(RuntimeErrorCode::Recovery))?;
                            let turn = session
                                .turns
                                .last()
                                .ok_or(DriverError::Runtime(RuntimeErrorCode::Recovery))?;
                            let mut facts = BTreeMap::from([
                                (
                                    "provider_id".to_owned(),
                                    SafeTraceFact::String(
                                        session.binding.provider_id.as_str().to_owned(),
                                    ),
                                ),
                                (
                                    "kind".to_owned(),
                                    SafeTraceFact::String(format!("{:?}", failure.code)),
                                ),
                                (
                                    "request_id".to_owned(),
                                    SafeTraceFact::String(turn.request_id.as_str().to_owned()),
                                ),
                            ]);
                            if let Some(status) = failure.http_status {
                                facts.insert(
                                    "status".to_owned(),
                                    SafeTraceFact::U64(u64::from(status)),
                                );
                            }
                            record_trace(store, ids, TraceCode::ProviderFailed, facts)?;
                        }
                        minimax_protocol::RuntimeTerminalOutcome::Interrupted => {
                            record_trace(
                                store,
                                ids,
                                TraceCode::TurnInterrupted,
                                BTreeMap::from([
                                    (
                                        "turn_id".to_owned(),
                                        SafeTraceFact::String(
                                            running_turn_id(store)?.as_str().to_owned(),
                                        ),
                                    ),
                                    (
                                        "had_assistant_draft".to_owned(),
                                        SafeTraceFact::Bool(!assistant.is_empty()),
                                    ),
                                ]),
                            )?;
                        }
                        minimax_protocol::RuntimeTerminalOutcome::Completed
                        | minimax_protocol::RuntimeTerminalOutcome::Stopped => {}
                    }
                    let receipt = receipt
                        .clone()
                        .ok_or(DriverError::Runtime(RuntimeErrorCode::Recovery))?;
                    store.apply_command(SessionCommand::Finalize {
                        record_id: ids.record_id()?,
                        receipt,
                        assistant_content: (!assistant.is_empty()).then(|| assistant.clone()),
                        now_unix_ms: ids.now_unix_ms(),
                    })?;
                }
                RuntimeEvent::ReasoningFiltered
                | RuntimeEvent::ToolCallObserved { .. }
                | RuntimeEvent::Usage { .. }
                | RuntimeEvent::Diagnostic { .. } => {}
            },
            RunEffect::Publish(event) => {
                publish(&event);
                published.push(event);
            }
            RunEffect::OpenProvider(_) | RunEffect::BeginTools(_) | RunEffect::Finalize(_) => {}
            RunEffect::AbortProvider => cancellation.cancel(),
        }
    }
    Ok(())
}

fn record_provider_connected(
    store: &mut RuntimeStore,
    ids: &mut DriverIds,
    request: &TurnRequest,
) -> Result<(), DriverError> {
    record_trace(
        store,
        ids,
        TraceCode::ProviderConnected,
        BTreeMap::from([
            (
                "provider_id".to_owned(),
                SafeTraceFact::String(request.provider_id.as_str().to_owned()),
            ),
            (
                "protocol".to_owned(),
                SafeTraceFact::String(format!("{:?}", request.protocol)),
            ),
            (
                "model".to_owned(),
                SafeTraceFact::String(request.model_id.as_str().to_owned()),
            ),
        ]),
    )
}

fn record_trace(
    store: &mut RuntimeStore,
    ids: &mut DriverIds,
    code: TraceCode,
    facts: BTreeMap<String, SafeTraceFact>,
) -> Result<(), DriverError> {
    let session_id = store
        .machine()
        .active_session()
        .map(|session| session.session_id.clone())
        .ok_or(DriverError::Runtime(RuntimeErrorCode::Recovery))?;
    let entry = SafeTraceRecorder::record(ids.now_unix_ms(), code, facts);
    store.append(SessionRecordV1::new(
        ids.record_id()?,
        JournalRecord::TraceStored { session_id, entry },
    ))?;
    Ok(())
}

fn running_turn_id(store: &RuntimeStore) -> Result<TurnId, DriverError> {
    store
        .machine()
        .active_session()
        .and_then(|session| session.turns.last())
        .map(|turn| turn.turn_id.clone())
        .ok_or(DriverError::Runtime(RuntimeErrorCode::Recovery))
}

fn budget_now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        })
}

fn normalize_decision(invocation: &ToolInvocation, decision: ToolDecision) -> ToolDecision {
    if decision.call_id == invocation.call.call_id && decision.clone().validate().is_ok() {
        decision
    } else {
        ToolDecision {
            schema_version: SchemaVersion,
            call_id: invocation.call.call_id.clone(),
            decision: ToolDecisionKind::Rejected,
            code: "invalid_approval_decision".to_owned(),
        }
    }
}

fn normalize_preflight_result(invocation: &ToolInvocation, result: ToolResult) -> ToolResult {
    if result.call_id == invocation.call.call_id
        && result.tool_name == invocation.call.name
        && result.status != ToolTerminalStatus::Succeeded
        && result.clone().validate().is_ok()
    {
        result
    } else {
        invalid_adapter_result(invocation, "invalid_preflight_result")
    }
}

fn normalize_execution_result(invocation: &ToolInvocation, result: ToolResult) -> ToolResult {
    if result.call_id == invocation.call.call_id
        && result.tool_name == invocation.call.name
        && result.clone().validate().is_ok()
    {
        result
    } else {
        invalid_adapter_result(invocation, "invalid_tool_result")
    }
}

fn invalid_adapter_result(invocation: &ToolInvocation, code: &str) -> ToolResult {
    ToolResult {
        schema_version: SchemaVersion,
        call_id: invocation.call.call_id.clone(),
        tool_name: invocation.call.name.clone(),
        status: ToolTerminalStatus::Failed,
        code: code.to_owned(),
        output: None,
    }
}
