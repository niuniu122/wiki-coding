use std::collections::VecDeque;
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
    RunMachine, RunState, SessionCommand, SessionEffect, SessionSummary, ToolPort,
};
use minimax_protocol::{
    AgentLimits, AssistantToolCallBatch, CompactionId, CompactionRecord, ConversationItem,
    JournalRecord, ModelBinding, RecordId, RequestId, RuntimeErrorCode, RuntimeEvent,
    RuntimeEventV1, RuntimeFailure, SchemaVersion, SessionId, SessionRecord, SessionRecordV1,
    StreamEvent, ToolDecision, ToolDecisionKind, ToolDefinition, ToolInvocation, ToolResult,
    ToolResultMessage, ToolTerminalStatus, TurnId, TurnReceipt, TurnRequest,
};
use minimax_provider::{HttpProviderClient, ResolvedCredential};
use minimax_tools::BuiltinToolPort;
use minimax_tui::{ApprovalInput, EventRenderer};
use minimax_vault::{RuntimeStore, RuntimeStoreError};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

pub trait ProviderPort {
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
}

impl HttpProviderPort {
    #[must_use]
    pub const fn new(client: HttpProviderClient, credential: ResolvedCredential) -> Self {
        Self { client, credential }
    }
}

impl ProviderPort for HttpProviderPort {
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
        if driver.store.machine().active_session().is_none() {
            driver.create_session(binding)?;
        }
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

    pub fn create_session(&mut self, binding: ModelBinding) -> Result<SessionId, DriverError> {
        let record_id = self.ids.record_id()?;
        let session_id = self.ids.session_id()?;
        let now_unix_ms = self.ids.now_unix_ms();
        self.store.apply_command(SessionCommand::Create {
            record_id,
            session_id: session_id.clone(),
            binding,
            now_unix_ms,
        })?;
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
                InvocationEffect::Execute(invocation) => {
                    if cancellation.is_cancelled() {
                        pending.extend(
                            machine
                                .apply(InvocationInput::Cancel)
                                .map_err(DriverError::Invocation)?,
                        );
                        continue;
                    }
                    let cancellation = DriverCancellation(cancellation);
                    let result = self.tools.execute(&invocation, &cancellation).await;
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

        let provider_result = loop {
            tokio::select! {
                event = receiver.recv() => {
                    if let Some(event) = event
                        && matches!(machine.state(), RunState::Running { .. }) {
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
                RuntimeEvent::Terminal { .. } => {
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
                RuntimeEvent::TurnStarted { .. }
                | RuntimeEvent::ReasoningFiltered
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
