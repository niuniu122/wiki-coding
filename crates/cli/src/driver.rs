use std::fmt;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::time::{SystemTime, UNIX_EPOCH};

use minimax_core::{
    CompactionBudget, CompactionError, LocalCompactor, RunEffect, RunInput, RunMachine, RunState,
    SessionCommand, SessionEffect, SessionSummary,
};
use minimax_protocol::{
    CompactionId, CompactionRecord, JournalRecord, ModelBinding, RecordId, RequestId,
    RuntimeErrorCode, RuntimeEvent, RuntimeEventV1, RuntimeFailure, SessionId, SessionRecord,
    SessionRecordV1, StreamEvent, TurnId, TurnReceipt, TurnRequest,
};
use minimax_provider::{HttpProviderClient, ResolvedCredential};
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
}

impl fmt::Display for DriverError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Runtime(error) => error.fmt(formatter),
            Self::Store(error) => error.fmt(formatter),
            Self::Compaction(error) => error.fmt(formatter),
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
    pub receipt: TurnReceipt,
}

pub struct RuntimeDriver<P> {
    store: RuntimeStore,
    provider: P,
    cancellation: CancellationToken,
    ids: DriverIds,
}

impl<P: ProviderPort> RuntimeDriver<P> {
    pub fn open(
        project_root: impl AsRef<Path>,
        binding: ModelBinding,
        provider: P,
        ids: DriverIds,
    ) -> Result<Self, DriverError> {
        let store = RuntimeStore::open(project_root).map_err(DriverError::Store)?;
        let mut driver = Self {
            store,
            provider,
            cancellation: CancellationToken::new(),
            ids,
        };
        if driver.store.machine().active_session().is_none() {
            driver.create_session(binding)?;
        }
        Ok(driver)
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

        let (sender, mut receiver) = mpsc::unbounded_channel();
        let mut emit = move |event| {
            let _ = sender.send(event);
        };
        let provider_future = self.provider.stream(&request, &run_cancellation, &mut emit);
        tokio::pin!(provider_future);

        let provider_result = loop {
            tokio::select! {
                event = receiver.recv() => {
                    if let Some(event) = event
                        && !matches!(machine.state(), RunState::Terminal { .. }) {
                        apply_runtime_input(
                            &mut self.store,
                            &mut self.ids,
                            &mut machine,
                            RunInput::ProviderEvent(event),
                            &mut events,
                                &mut assistant,
                                &run_cancellation,
                                publish,
                        )?;
                    }
                }
                result = &mut provider_future => break result,
            }
        };

        while let Ok(event) = receiver.try_recv() {
            if matches!(machine.state(), RunState::Terminal { .. }) {
                break;
            }
            apply_runtime_input(
                &mut self.store,
                &mut self.ids,
                &mut machine,
                RunInput::ProviderEvent(event),
                &mut events,
                &mut assistant,
                &run_cancellation,
                publish,
            )?;
        }

        if !matches!(machine.state(), RunState::Terminal { .. }) {
            let input = match provider_result {
                Ok(()) => RunInput::ProviderEof,
                Err(failure) if failure.code == RuntimeErrorCode::Interrupted => RunInput::Cancel,
                Err(failure) => RunInput::ProviderFailed(failure),
            };
            apply_runtime_input(
                &mut self.store,
                &mut self.ids,
                &mut machine,
                input,
                &mut events,
                &mut assistant,
                &run_cancellation,
                publish,
            )?;
        }

        let RunState::Terminal { receipt } = machine.state() else {
            return Err(DriverError::Runtime(RuntimeErrorCode::Recovery));
        };
        Ok(RunReport {
            events,
            receipt: receipt.clone(),
        })
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
            RunEffect::OpenProvider(_) | RunEffect::Finalize(_) => {}
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
