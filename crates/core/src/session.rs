use std::collections::BTreeMap;

use minimax_protocol::{
    JournalRecord, MessageRole, ModelBinding, ModelMessage, OutputSettings, RecordId, RequestId,
    RuntimeErrorCode, RuntimeTerminalOutcome, SessionId, SessionRecord, SessionRecordV1,
    SessionStatus, ToolCallId, ToolDecision, ToolDecisionKind, ToolInvocation,
    ToolInvocationRecord, ToolResult, ToolTerminalStatus, TurnId, TurnReceipt, TurnRecord,
    TurnRequest, TurnStatus, VisibleMessage,
};

use crate::tool::terminal_is_legal_after_start;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionSummary {
    pub session_id: SessionId,
    pub status: SessionStatus,
    pub updated_at_unix_ms: u64,
    pub turn_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionCommand {
    Create {
        record_id: RecordId,
        session_id: SessionId,
        binding: ModelBinding,
        now_unix_ms: u64,
    },
    List,
    Resume {
        record_id: RecordId,
        session_id: SessionId,
        now_unix_ms: u64,
    },
    Continue {
        record_id: RecordId,
        turn_id: TurnId,
        request_id: RequestId,
        user_input: String,
        max_output_tokens: u32,
        now_unix_ms: u64,
    },
    RecordDelta {
        record_id: RecordId,
        turn_id: TurnId,
        delta: String,
        now_unix_ms: u64,
    },
    Interrupt {
        record_id: RecordId,
        turn_id: TurnId,
        partial_assistant: Option<String>,
        now_unix_ms: u64,
    },
    Retry {
        record_id: RecordId,
        source_turn_id: TurnId,
        new_turn_id: TurnId,
        request_id: RequestId,
        max_output_tokens: u32,
        now_unix_ms: u64,
    },
    Finalize {
        record_id: RecordId,
        receipt: TurnReceipt,
        assistant_content: Option<String>,
        now_unix_ms: u64,
    },
    RecordToolRequested {
        record_id: RecordId,
        turn_id: TurnId,
        invocation: ToolInvocation,
        now_unix_ms: u64,
    },
    RecordToolDecision {
        record_id: RecordId,
        turn_id: TurnId,
        decision: ToolDecision,
        now_unix_ms: u64,
    },
    RecordToolStarted {
        record_id: RecordId,
        turn_id: TurnId,
        call_id: ToolCallId,
        now_unix_ms: u64,
    },
    RecordToolTerminal {
        record_id: RecordId,
        turn_id: TurnId,
        result: ToolResult,
        now_unix_ms: u64,
    },
    Recover {
        record_id: RecordId,
        turn_id: TurnId,
        partial_assistant: Option<String>,
        now_unix_ms: u64,
    },
    Replay(SessionRecordV1),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionEffect {
    Persist(SessionRecordV1),
    Listed(Vec<SessionSummary>),
    Activated(SessionId),
    StartTurn(TurnRequest),
    AbortTurn(TurnId),
    Finalized(TurnReceipt),
}

struct StartTurnInput {
    record_id: RecordId,
    turn_id: TurnId,
    request_id: RequestId,
    retry_of: Option<TurnId>,
    user_input: String,
    max_output_tokens: u32,
    now_unix_ms: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SessionMachine {
    sessions: BTreeMap<SessionId, SessionRecord>,
    active_session_id: Option<SessionId>,
    seen_records: BTreeMap<RecordId, SessionRecordV1>,
}

impl SessionMachine {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            sessions: BTreeMap::new(),
            active_session_id: None,
            seen_records: BTreeMap::new(),
        }
    }

    pub fn replay(
        records: impl IntoIterator<Item = SessionRecordV1>,
    ) -> Result<Self, RuntimeErrorCode> {
        let mut machine = Self::new();
        for record in records {
            machine.apply(SessionCommand::Replay(record))?;
        }
        Ok(machine)
    }

    #[must_use]
    pub fn sessions(&self) -> &BTreeMap<SessionId, SessionRecord> {
        &self.sessions
    }

    #[must_use]
    pub fn active_session(&self) -> Option<&SessionRecord> {
        self.active_session_id
            .as_ref()
            .and_then(|id| self.sessions.get(id))
    }

    pub fn apply(
        &mut self,
        command: SessionCommand,
    ) -> Result<Vec<SessionEffect>, RuntimeErrorCode> {
        match command {
            SessionCommand::Create {
                record_id,
                session_id,
                binding,
                now_unix_ms,
            } => {
                if self.sessions.contains_key(&session_id) {
                    return Err(RuntimeErrorCode::Recovery);
                }
                let record = SessionRecordV1::new(
                    record_id,
                    JournalRecord::SessionCreated {
                        session: SessionRecord {
                            session_id,
                            created_at_unix_ms: now_unix_ms,
                            updated_at_unix_ms: now_unix_ms,
                            status: SessionStatus::Active,
                            binding,
                            turns: Vec::new(),
                            compaction: None,
                        },
                    },
                );
                self.persist(record, None)
            }
            SessionCommand::List => Ok(vec![SessionEffect::Listed(self.summaries())]),
            SessionCommand::Resume {
                record_id,
                session_id,
                now_unix_ms,
            } => {
                if self.has_running_turn() || !self.sessions.contains_key(&session_id) {
                    return Err(if self.has_running_turn() {
                        RuntimeErrorCode::WorkspaceBusy
                    } else {
                        RuntimeErrorCode::Recovery
                    });
                }
                let record = SessionRecordV1::new(
                    record_id,
                    JournalRecord::SessionActivated {
                        session_id: session_id.clone(),
                        activated_at_unix_ms: now_unix_ms,
                    },
                );
                self.persist(record, Some(SessionEffect::Activated(session_id)))
            }
            SessionCommand::Continue {
                record_id,
                turn_id,
                request_id,
                user_input,
                max_output_tokens,
                now_unix_ms,
            } => self.start_turn(StartTurnInput {
                record_id,
                turn_id,
                request_id,
                retry_of: None,
                user_input,
                max_output_tokens,
                now_unix_ms,
            }),
            SessionCommand::RecordDelta {
                record_id,
                turn_id,
                delta,
                now_unix_ms,
            } => {
                let session_id = self.running_session_for(&turn_id)?.session_id.clone();
                let record = SessionRecordV1::new(
                    record_id,
                    JournalRecord::TurnDelta {
                        session_id,
                        turn_id,
                        delta,
                        recorded_at_unix_ms: now_unix_ms,
                    },
                );
                self.persist(record, None)
            }
            SessionCommand::Interrupt {
                record_id,
                turn_id,
                partial_assistant,
                now_unix_ms,
            } => {
                let receipt = self.interrupted_receipt(&turn_id)?;
                let session_id = receipt.session_id.clone();
                let record = SessionRecordV1::new(
                    record_id,
                    JournalRecord::TurnTerminal {
                        session_id,
                        receipt: receipt.clone(),
                        assistant_message: partial_message(partial_assistant),
                        completed_at_unix_ms: now_unix_ms,
                    },
                );
                let mut effects = self.persist(
                    record,
                    Some(SessionEffect::AbortTurn(receipt.turn_id.clone())),
                )?;
                effects.push(SessionEffect::Finalized(receipt));
                Ok(effects)
            }
            SessionCommand::Retry {
                record_id,
                source_turn_id,
                new_turn_id,
                request_id,
                max_output_tokens,
                now_unix_ms,
            } => {
                let source = self.find_turn(&source_turn_id)?.clone();
                if !source.status.is_terminal() || self.has_running_turn() {
                    return Err(RuntimeErrorCode::WorkspaceBusy);
                }
                self.start_turn(StartTurnInput {
                    record_id,
                    turn_id: new_turn_id,
                    request_id,
                    retry_of: Some(source_turn_id),
                    user_input: source.user_message.content,
                    max_output_tokens,
                    now_unix_ms,
                })
            }
            SessionCommand::Finalize {
                record_id,
                receipt,
                assistant_content,
                now_unix_ms,
            } => {
                self.running_session_for(&receipt.turn_id)?;
                let message = assistant_content.map(|content| VisibleMessage {
                    role: MessageRole::Assistant,
                    content,
                    partial: !matches!(receipt.outcome, RuntimeTerminalOutcome::Completed),
                });
                let record = SessionRecordV1::new(
                    record_id,
                    JournalRecord::TurnTerminal {
                        session_id: receipt.session_id.clone(),
                        receipt: receipt.clone(),
                        assistant_message: message,
                        completed_at_unix_ms: now_unix_ms,
                    },
                );
                self.persist(record, Some(SessionEffect::Finalized(receipt)))
            }
            SessionCommand::RecordToolRequested {
                record_id,
                turn_id,
                invocation,
                now_unix_ms,
            } => {
                let session_id = self.running_session_for(&turn_id)?.session_id.clone();
                let record = SessionRecordV1::new(
                    record_id,
                    JournalRecord::ToolRequested {
                        session_id,
                        turn_id,
                        invocation,
                        requested_at_unix_ms: now_unix_ms,
                    },
                );
                self.persist(record, None)
            }
            SessionCommand::RecordToolDecision {
                record_id,
                turn_id,
                decision,
                now_unix_ms,
            } => {
                let session_id = self.running_session_for(&turn_id)?.session_id.clone();
                let record = SessionRecordV1::new(
                    record_id,
                    JournalRecord::ToolDecisionRecorded {
                        session_id,
                        turn_id,
                        decision,
                        recorded_at_unix_ms: now_unix_ms,
                    },
                );
                self.persist(record, None)
            }
            SessionCommand::RecordToolStarted {
                record_id,
                turn_id,
                call_id,
                now_unix_ms,
            } => {
                let session_id = self.running_session_for(&turn_id)?.session_id.clone();
                let record = SessionRecordV1::new(
                    record_id,
                    JournalRecord::ToolStarted {
                        session_id,
                        turn_id,
                        call_id,
                        started_at_unix_ms: now_unix_ms,
                    },
                );
                self.persist(record, None)
            }
            SessionCommand::RecordToolTerminal {
                record_id,
                turn_id,
                result,
                now_unix_ms,
            } => {
                let session_id = self.running_session_for(&turn_id)?.session_id.clone();
                let record = SessionRecordV1::new(
                    record_id,
                    JournalRecord::ToolTerminal {
                        session_id,
                        turn_id,
                        result,
                        completed_at_unix_ms: now_unix_ms,
                    },
                );
                self.persist(record, None)
            }
            SessionCommand::Recover {
                record_id,
                turn_id,
                partial_assistant,
                now_unix_ms,
            } => {
                let receipt = self.interrupted_receipt(&turn_id)?;
                let record = SessionRecordV1::new(
                    record_id,
                    JournalRecord::RecoveryApplied {
                        session_id: receipt.session_id.clone(),
                        receipt: receipt.clone(),
                        partial_assistant_message: partial_message(partial_assistant),
                        recovered_at_unix_ms: now_unix_ms,
                    },
                );
                self.persist(record, Some(SessionEffect::Finalized(receipt)))
            }
            SessionCommand::Replay(record) => {
                if let Some(existing) = self.seen_records.get(&record.record_id) {
                    return if existing == &record {
                        Ok(Vec::new())
                    } else {
                        Err(RuntimeErrorCode::Recovery)
                    };
                }
                self.apply_record(&record)?;
                self.seen_records.insert(record.record_id.clone(), record);
                Ok(Vec::new())
            }
        }
    }

    fn start_turn(
        &mut self,
        input: StartTurnInput,
    ) -> Result<Vec<SessionEffect>, RuntimeErrorCode> {
        let StartTurnInput {
            record_id,
            turn_id,
            request_id,
            retry_of,
            user_input,
            max_output_tokens,
            now_unix_ms,
        } = input;
        if user_input.trim().is_empty() || self.has_running_turn() {
            return Err(if self.has_running_turn() {
                RuntimeErrorCode::WorkspaceBusy
            } else {
                RuntimeErrorCode::Configuration
            });
        }
        let session = self.active_session().ok_or(RuntimeErrorCode::Recovery)?;
        if session
            .turns
            .iter()
            .any(|turn| turn.turn_id == turn_id || turn.request_id == request_id)
        {
            return Err(RuntimeErrorCode::Recovery);
        }
        let binding = session.binding.clone();
        let turn = TurnRecord {
            turn_id: turn_id.clone(),
            request_id: request_id.clone(),
            started_at_unix_ms: now_unix_ms,
            completed_at_unix_ms: None,
            retry_of,
            status: TurnStatus::Running,
            user_message: VisibleMessage {
                role: MessageRole::User,
                content: user_input,
                partial: false,
            },
            assistant_message: None,
            usage: None,
            receipt: None,
            tool_invocations: Vec::new(),
        };
        let record = SessionRecordV1::new(
            record_id,
            JournalRecord::TurnStarted {
                session_id: session.session_id.clone(),
                binding: binding.clone(),
                turn: Box::new(turn.clone()),
            },
        );
        self.apply_record(&record)?;
        self.seen_records
            .insert(record.record_id.clone(), record.clone());
        let request = self.turn_request(&turn, &binding, max_output_tokens)?;
        Ok(vec![
            SessionEffect::Persist(record),
            SessionEffect::StartTurn(request),
        ])
    }

    fn persist(
        &mut self,
        record: SessionRecordV1,
        after: Option<SessionEffect>,
    ) -> Result<Vec<SessionEffect>, RuntimeErrorCode> {
        if let Some(existing) = self.seen_records.get(&record.record_id) {
            return if existing == &record {
                Ok(Vec::new())
            } else {
                Err(RuntimeErrorCode::Recovery)
            };
        }
        self.apply_record(&record)?;
        self.seen_records
            .insert(record.record_id.clone(), record.clone());
        let mut effects = vec![SessionEffect::Persist(record)];
        if let Some(effect) = after {
            effects.push(effect);
        }
        Ok(effects)
    }

    fn apply_record(&mut self, envelope: &SessionRecordV1) -> Result<(), RuntimeErrorCode> {
        match &envelope.record {
            JournalRecord::SessionCreated { session } => {
                if self.sessions.contains_key(&session.session_id) {
                    return Err(RuntimeErrorCode::Recovery);
                }
                for existing in self.sessions.values_mut() {
                    existing.status = SessionStatus::Archived;
                }
                self.active_session_id = Some(session.session_id.clone());
                self.sessions
                    .insert(session.session_id.clone(), session.clone());
            }
            JournalRecord::SessionActivated {
                session_id,
                activated_at_unix_ms,
            } => {
                if !self.sessions.contains_key(session_id) {
                    return Err(RuntimeErrorCode::Recovery);
                }
                for session in self.sessions.values_mut() {
                    session.status = if session.session_id == *session_id {
                        session.updated_at_unix_ms = *activated_at_unix_ms;
                        SessionStatus::Active
                    } else {
                        SessionStatus::Archived
                    };
                }
                self.active_session_id = Some(session_id.clone());
            }
            JournalRecord::TurnStarted {
                session_id,
                binding,
                turn,
            } => {
                let session = self
                    .sessions
                    .get_mut(session_id)
                    .ok_or(RuntimeErrorCode::Recovery)?;
                if session.binding != *binding
                    || session.turns.iter().any(|existing| {
                        existing.turn_id == turn.turn_id || existing.request_id == turn.request_id
                    })
                    || turn.status != TurnStatus::Running
                {
                    return Err(RuntimeErrorCode::Recovery);
                }
                session.updated_at_unix_ms = turn.started_at_unix_ms;
                session.turns.push(turn.as_ref().clone());
            }
            JournalRecord::TurnDelta {
                session_id,
                turn_id,
                delta,
                recorded_at_unix_ms,
            } => {
                let turn = find_turn_mut(&mut self.sessions, session_id, turn_id)?;
                if turn.status != TurnStatus::Running {
                    return Err(RuntimeErrorCode::Recovery);
                }
                let message = turn.assistant_message.get_or_insert(VisibleMessage {
                    role: MessageRole::Assistant,
                    content: String::new(),
                    partial: true,
                });
                message.content.push_str(delta);
                self.sessions
                    .get_mut(session_id)
                    .ok_or(RuntimeErrorCode::Recovery)?
                    .updated_at_unix_ms = *recorded_at_unix_ms;
            }
            JournalRecord::TurnTerminal {
                session_id,
                receipt,
                assistant_message,
                completed_at_unix_ms,
            } => self.apply_terminal(
                session_id,
                receipt,
                assistant_message.clone(),
                *completed_at_unix_ms,
            )?,
            JournalRecord::ToolRequested {
                session_id,
                turn_id,
                invocation,
                requested_at_unix_ms,
            } => {
                ToolInvocation::new(invocation.call.clone(), invocation.effect)
                    .map_err(|_| RuntimeErrorCode::Recovery)?;
                let turn = find_turn_mut(&mut self.sessions, session_id, turn_id)?;
                if turn.status != TurnStatus::Running
                    || turn
                        .tool_invocations
                        .iter()
                        .any(|existing| existing.invocation.call.call_id == invocation.call.call_id)
                {
                    return Err(RuntimeErrorCode::Recovery);
                }
                turn.tool_invocations.push(ToolInvocationRecord {
                    invocation: invocation.clone(),
                    requested_at_unix_ms: *requested_at_unix_ms,
                    decision: None,
                    decision_at_unix_ms: None,
                    started_at_unix_ms: None,
                    terminal_result: None,
                    terminal_at_unix_ms: None,
                });
                self.sessions
                    .get_mut(session_id)
                    .ok_or(RuntimeErrorCode::Recovery)?
                    .updated_at_unix_ms = *requested_at_unix_ms;
            }
            JournalRecord::ToolDecisionRecorded {
                session_id,
                turn_id,
                decision,
                recorded_at_unix_ms,
            } => {
                let decision = decision
                    .clone()
                    .validate()
                    .map_err(|_| RuntimeErrorCode::Recovery)?;
                let invocation =
                    find_tool_mut(&mut self.sessions, session_id, turn_id, &decision.call_id)?;
                if invocation.decision.is_some()
                    || invocation.started_at_unix_ms.is_some()
                    || invocation.terminal_result.is_some()
                {
                    return Err(RuntimeErrorCode::Recovery);
                }
                invocation.decision = Some(decision);
                invocation.decision_at_unix_ms = Some(*recorded_at_unix_ms);
                self.sessions
                    .get_mut(session_id)
                    .ok_or(RuntimeErrorCode::Recovery)?
                    .updated_at_unix_ms = *recorded_at_unix_ms;
            }
            JournalRecord::ToolStarted {
                session_id,
                turn_id,
                call_id,
                started_at_unix_ms,
            } => {
                let invocation = find_tool_mut(&mut self.sessions, session_id, turn_id, call_id)?;
                if invocation.started_at_unix_ms.is_some()
                    || invocation.terminal_result.is_some()
                    || !matches!(
                        invocation
                            .decision
                            .as_ref()
                            .map(|decision| decision.decision),
                        Some(ToolDecisionKind::Approved)
                    )
                {
                    return Err(RuntimeErrorCode::Recovery);
                }
                invocation.started_at_unix_ms = Some(*started_at_unix_ms);
                self.sessions
                    .get_mut(session_id)
                    .ok_or(RuntimeErrorCode::Recovery)?
                    .updated_at_unix_ms = *started_at_unix_ms;
            }
            JournalRecord::ToolTerminal {
                session_id,
                turn_id,
                result,
                completed_at_unix_ms,
            } => {
                let result = result
                    .clone()
                    .validate()
                    .map_err(|_| RuntimeErrorCode::Recovery)?;
                let invocation =
                    find_tool_mut(&mut self.sessions, session_id, turn_id, &result.call_id)?;
                if invocation.terminal_result.is_some()
                    || invocation.invocation.call.name != result.tool_name
                    || !terminal_is_legal(invocation, &result)
                {
                    return Err(RuntimeErrorCode::Recovery);
                }
                invocation.terminal_result = Some(result);
                invocation.terminal_at_unix_ms = Some(*completed_at_unix_ms);
                self.sessions
                    .get_mut(session_id)
                    .ok_or(RuntimeErrorCode::Recovery)?
                    .updated_at_unix_ms = *completed_at_unix_ms;
            }
            JournalRecord::RecoveryApplied {
                session_id,
                receipt,
                partial_assistant_message,
                recovered_at_unix_ms,
            } => self.apply_terminal(
                session_id,
                receipt,
                partial_assistant_message.clone(),
                *recovered_at_unix_ms,
            )?,
            JournalRecord::CompactionStored {
                session_id,
                compaction,
                stored_at_unix_ms,
            } => {
                let session = self
                    .sessions
                    .get_mut(session_id)
                    .ok_or(RuntimeErrorCode::Recovery)?;
                let covered_index = session
                    .turns
                    .iter()
                    .position(|turn| {
                        turn.turn_id == compaction.covered_through_turn_id
                            && turn.status == TurnStatus::Completed
                    })
                    .ok_or(RuntimeErrorCode::Recovery)?;
                if compaction.retained_recent_turns.iter().any(|retained| {
                    !session.turns[..=covered_index].iter().any(|turn| {
                        turn.turn_id == retained.turn_id && turn.status == TurnStatus::Completed
                    })
                }) {
                    return Err(RuntimeErrorCode::Recovery);
                }
                if let Some(previous) = &session.compaction {
                    let previous_index = session
                        .turns
                        .iter()
                        .position(|turn| turn.turn_id == previous.covered_through_turn_id)
                        .ok_or(RuntimeErrorCode::Recovery)?;
                    if previous.compaction_id == compaction.compaction_id
                        || previous_index >= covered_index
                    {
                        return Err(RuntimeErrorCode::Recovery);
                    }
                }
                session.compaction = Some(minimax_protocol::CompactionPointer {
                    compaction_id: compaction.compaction_id.clone(),
                    covered_through_turn_id: compaction.covered_through_turn_id.clone(),
                });
                session.updated_at_unix_ms = *stored_at_unix_ms;
            }
            JournalRecord::TraceStored { session_id, .. } => {
                if !self.sessions.contains_key(session_id) {
                    return Err(RuntimeErrorCode::Recovery);
                }
            }
        }
        Ok(())
    }

    fn apply_terminal(
        &mut self,
        session_id: &SessionId,
        receipt: &TurnReceipt,
        assistant_message: Option<VisibleMessage>,
        completed_at_unix_ms: u64,
    ) -> Result<(), RuntimeErrorCode> {
        if receipt.session_id != *session_id {
            return Err(RuntimeErrorCode::Recovery);
        }
        let turn = find_turn_mut(&mut self.sessions, session_id, &receipt.turn_id)?;
        if turn.status != TurnStatus::Running
            || turn.request_id != receipt.request_id
            || turn
                .tool_invocations
                .iter()
                .any(|invocation| invocation.terminal_result.is_none())
        {
            return Err(RuntimeErrorCode::Recovery);
        }
        turn.status = status_from_outcome(&receipt.outcome);
        turn.completed_at_unix_ms = Some(completed_at_unix_ms);
        turn.assistant_message = assistant_message;
        turn.usage = receipt.usage;
        turn.receipt = Some(receipt.clone());
        self.sessions
            .get_mut(session_id)
            .ok_or(RuntimeErrorCode::Recovery)?
            .updated_at_unix_ms = completed_at_unix_ms;
        Ok(())
    }

    fn turn_request(
        &self,
        turn: &TurnRecord,
        binding: &ModelBinding,
        max_output_tokens: u32,
    ) -> Result<TurnRequest, RuntimeErrorCode> {
        let session = self.active_session().ok_or(RuntimeErrorCode::Recovery)?;
        let mut messages = Vec::new();
        for previous in &session.turns {
            if previous.turn_id == turn.turn_id {
                continue;
            }
            if previous.status == TurnStatus::Completed {
                messages.push(
                    ModelMessage {
                        role: MessageRole::User,
                        content: previous.user_message.content.clone(),
                    }
                    .into(),
                );
                if let Some(assistant) = previous
                    .assistant_message
                    .as_ref()
                    .filter(|message| !message.partial)
                {
                    messages.push(
                        ModelMessage {
                            role: MessageRole::Assistant,
                            content: assistant.content.clone(),
                        }
                        .into(),
                    );
                }
            }
        }
        messages.push(
            ModelMessage {
                role: MessageRole::User,
                content: turn.user_message.content.clone(),
            }
            .into(),
        );
        TurnRequest {
            session_id: session.session_id.clone(),
            turn_id: turn.turn_id.clone(),
            request_id: turn.request_id.clone(),
            provider_id: binding.provider_id.clone(),
            model_id: binding.model_id.clone(),
            protocol: binding.protocol,
            messages,
            tools: Vec::new(),
            agent_limits: None,
            output: OutputSettings::new(max_output_tokens)?,
        }
        .validate()
    }

    fn summaries(&self) -> Vec<SessionSummary> {
        self.sessions
            .values()
            .map(|session| SessionSummary {
                session_id: session.session_id.clone(),
                status: session.status,
                updated_at_unix_ms: session.updated_at_unix_ms,
                turn_count: session.turns.len(),
            })
            .collect()
    }

    fn has_running_turn(&self) -> bool {
        self.sessions
            .values()
            .flat_map(|session| &session.turns)
            .any(|turn| turn.status == TurnStatus::Running)
    }

    fn running_session_for(&self, turn_id: &TurnId) -> Result<&SessionRecord, RuntimeErrorCode> {
        self.sessions
            .values()
            .find(|session| {
                session
                    .turns
                    .iter()
                    .any(|turn| turn.turn_id == *turn_id && turn.status == TurnStatus::Running)
            })
            .ok_or(RuntimeErrorCode::Recovery)
    }

    fn find_turn(&self, turn_id: &TurnId) -> Result<&TurnRecord, RuntimeErrorCode> {
        self.sessions
            .values()
            .flat_map(|session| &session.turns)
            .find(|turn| turn.turn_id == *turn_id)
            .ok_or(RuntimeErrorCode::Recovery)
    }

    fn interrupted_receipt(&self, turn_id: &TurnId) -> Result<TurnReceipt, RuntimeErrorCode> {
        let session = self.running_session_for(turn_id)?;
        let turn = session
            .turns
            .iter()
            .find(|turn| turn.turn_id == *turn_id)
            .ok_or(RuntimeErrorCode::Recovery)?;
        Ok(TurnReceipt {
            session_id: session.session_id.clone(),
            turn_id: turn.turn_id.clone(),
            request_id: turn.request_id.clone(),
            outcome: RuntimeTerminalOutcome::Interrupted,
            usage: turn.usage,
        })
    }
}

fn find_turn_mut<'a>(
    sessions: &'a mut BTreeMap<SessionId, SessionRecord>,
    session_id: &SessionId,
    turn_id: &TurnId,
) -> Result<&'a mut TurnRecord, RuntimeErrorCode> {
    sessions
        .get_mut(session_id)
        .and_then(|session| {
            session
                .turns
                .iter_mut()
                .find(|turn| turn.turn_id == *turn_id)
        })
        .ok_or(RuntimeErrorCode::Recovery)
}

fn find_tool_mut<'a>(
    sessions: &'a mut BTreeMap<SessionId, SessionRecord>,
    session_id: &SessionId,
    turn_id: &TurnId,
    call_id: &ToolCallId,
) -> Result<&'a mut ToolInvocationRecord, RuntimeErrorCode> {
    let turn = find_turn_mut(sessions, session_id, turn_id)?;
    if turn.status != TurnStatus::Running {
        return Err(RuntimeErrorCode::Recovery);
    }
    turn.tool_invocations
        .iter_mut()
        .find(|invocation| invocation.invocation.call.call_id == *call_id)
        .ok_or(RuntimeErrorCode::Recovery)
}

fn terminal_is_legal(invocation: &ToolInvocationRecord, result: &ToolResult) -> bool {
    if invocation.started_at_unix_ms.is_some() {
        return terminal_is_legal_after_start(
            &invocation.invocation.call.name,
            result.status,
            &result.code,
        );
    }
    match result.status {
        ToolTerminalStatus::Succeeded | ToolTerminalStatus::Indeterminate => false,
        ToolTerminalStatus::Failed => true,
        ToolTerminalStatus::Rejected => !matches!(
            invocation
                .decision
                .as_ref()
                .map(|decision| decision.decision),
            Some(ToolDecisionKind::Approved)
        ),
        ToolTerminalStatus::Cancelled => true,
    }
}

fn partial_message(content: Option<String>) -> Option<VisibleMessage> {
    content.map(|content| VisibleMessage {
        role: MessageRole::Assistant,
        content,
        partial: true,
    })
}

fn status_from_outcome(outcome: &RuntimeTerminalOutcome) -> TurnStatus {
    match outcome {
        RuntimeTerminalOutcome::Completed => TurnStatus::Completed,
        RuntimeTerminalOutcome::Failed { .. } => TurnStatus::Failed,
        RuntimeTerminalOutcome::Interrupted => TurnStatus::Interrupted,
        RuntimeTerminalOutcome::Stopped => TurnStatus::Stopped,
    }
}
