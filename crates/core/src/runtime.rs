use minimax_protocol::{
    ConversationItem, RuntimeErrorCode, RuntimeEvent, RuntimeEventV1, RuntimeFailure,
    RuntimeTerminalOutcome, StreamEvent, TerminalOutcome, ToolCall, ToolEffect, ToolInvocation,
    TurnReceipt, TurnRequest, Usage,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RunState {
    Idle,
    Running {
        request: TurnRequest,
        usage: Option<Usage>,
        pending_tools: Vec<ToolInvocation>,
    },
    AwaitingTools {
        request: TurnRequest,
        usage: Option<Usage>,
        invocations: Vec<ToolInvocation>,
    },
    Terminal {
        receipt: TurnReceipt,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RunInput {
    Begin(TurnRequest),
    ProviderEvent(StreamEvent),
    ProviderFailed(RuntimeFailure),
    ProviderEof,
    ContinueAfterTools(TurnRequest),
    Cancel,
    Shutdown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RunEffect {
    Persist(RuntimeEventV1),
    Publish(RuntimeEventV1),
    OpenProvider(TurnRequest),
    AbortProvider,
    BeginTools(Vec<ToolInvocation>),
    Finalize(TurnReceipt),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunMachine {
    state: RunState,
}

impl Default for RunMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl RunMachine {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: RunState::Idle,
        }
    }

    #[must_use]
    pub const fn state(&self) -> &RunState {
        &self.state
    }

    pub fn apply(&mut self, input: RunInput) -> Result<Vec<RunEffect>, RuntimeErrorCode> {
        match (&self.state, input) {
            (RunState::Idle, RunInput::Begin(request)) => self.begin(request),
            (RunState::Idle, RunInput::Shutdown) => Ok(Vec::new()),
            (RunState::Idle, _) => Err(RuntimeErrorCode::ProtocolEventAfterTerminal),
            (RunState::Running { .. }, RunInput::Begin(_)) => Err(RuntimeErrorCode::WorkspaceBusy),
            (RunState::Running { .. }, RunInput::ContinueAfterTools(_)) => {
                Err(RuntimeErrorCode::WorkspaceBusy)
            }
            (RunState::Running { .. }, RunInput::ProviderEvent(event)) => {
                self.provider_event(event)
            }
            (RunState::Running { .. }, RunInput::ProviderFailed(failure)) => {
                self.finish(RuntimeTerminalOutcome::Failed { failure }, true)
            }
            (RunState::Running { .. }, RunInput::ProviderEof) => self.finish(
                RuntimeTerminalOutcome::Failed {
                    failure: RuntimeFailure::new(RuntimeErrorCode::ProtocolPrematureEof),
                },
                true,
            ),
            (RunState::Running { .. }, RunInput::Cancel | RunInput::Shutdown) => {
                self.finish(RuntimeTerminalOutcome::Interrupted, true)
            }
            (RunState::AwaitingTools { .. }, RunInput::ContinueAfterTools(request)) => {
                self.continue_after_tools(request)
            }
            (RunState::AwaitingTools { .. }, RunInput::Cancel | RunInput::Shutdown) => {
                self.finish(RuntimeTerminalOutcome::Interrupted, false)
            }
            (RunState::AwaitingTools { .. }, RunInput::Begin(_)) => {
                Err(RuntimeErrorCode::WorkspaceBusy)
            }
            (RunState::AwaitingTools { .. }, _) => {
                Err(RuntimeErrorCode::ProtocolEventAfterTerminal)
            }
            (RunState::Terminal { .. }, RunInput::Shutdown | RunInput::Cancel) => Ok(Vec::new()),
            (RunState::Terminal { .. }, RunInput::ProviderEvent(event)) if event.is_terminal() => {
                Err(RuntimeErrorCode::ProtocolDuplicateTerminal)
            }
            (RunState::Terminal { .. }, _) => Err(RuntimeErrorCode::ProtocolEventAfterTerminal),
        }
    }

    fn begin(&mut self, request: TurnRequest) -> Result<Vec<RunEffect>, RuntimeErrorCode> {
        let request = request.validate()?;
        let started = RuntimeEventV1::new(RuntimeEvent::TurnStarted {
            session_id: request.session_id.clone(),
            turn_id: request.turn_id.clone(),
            request_id: request.request_id.clone(),
        });
        self.state = RunState::Running {
            request: request.clone(),
            usage: None,
            pending_tools: Vec::new(),
        };
        Ok(vec![
            RunEffect::Persist(started.clone()),
            RunEffect::Publish(started),
            RunEffect::OpenProvider(request),
        ])
    }

    fn provider_event(&mut self, event: StreamEvent) -> Result<Vec<RunEffect>, RuntimeErrorCode> {
        match event {
            StreamEvent::ReasoningFiltered => Ok(observable(RuntimeEvent::ReasoningFiltered)),
            StreamEvent::VisibleTextDelta { delta } => {
                Ok(observable(RuntimeEvent::VisibleTextDelta { delta }))
            }
            StreamEvent::Usage { usage } => {
                if let RunState::Running { usage: active, .. } = &mut self.state {
                    *active = Some(merge_usage(*active, usage));
                }
                Ok(observable(RuntimeEvent::Usage { usage }))
            }
            StreamEvent::ToolCallFragments { fragments } => {
                let mut effects = Vec::new();
                let RunState::Running {
                    request,
                    pending_tools,
                    ..
                } = &mut self.state
                else {
                    return Err(RuntimeErrorCode::ProtocolEventAfterTerminal);
                };
                if request.tools.is_empty() {
                    for fragment in fragments {
                        effects.extend(observable(RuntimeEvent::ToolCallObserved {
                            call_id: fragment.call_id,
                            name: fragment.name,
                        }));
                    }
                    effects.extend(self.finish(
                        RuntimeTerminalOutcome::Failed {
                            failure: RuntimeFailure::new(RuntimeErrorCode::ToolUnavailable),
                        },
                        true,
                    )?);
                    return Ok(effects);
                }
                for fragment in fragments {
                    if !fragment.arguments_complete {
                        return Err(RuntimeErrorCode::ProtocolMalformedJson);
                    }
                    let name = fragment
                        .name
                        .clone()
                        .ok_or(RuntimeErrorCode::ProtocolMalformedJson)?;
                    let call = ToolCall::new(
                        fragment.call_id.clone(),
                        name.clone(),
                        fragment
                            .arguments_delta
                            .clone()
                            .ok_or(RuntimeErrorCode::ProtocolMalformedJson)?,
                    )
                    .map_err(|_| RuntimeErrorCode::ProtocolMalformedJson)?;
                    let definition = request
                        .tools
                        .iter()
                        .find(|definition| definition.name == call.name)
                        .ok_or(RuntimeErrorCode::ToolUnavailable)?;
                    definition
                        .validate_call(&call)
                        .map_err(|_| RuntimeErrorCode::ProtocolMalformedJson)?;
                    if pending_tools
                        .iter()
                        .any(|invocation| invocation.call.call_id == call.call_id)
                    {
                        return Err(RuntimeErrorCode::ProtocolMalformedJson);
                    }
                    let effect =
                        effect_for_tool(&call.name).ok_or(RuntimeErrorCode::ToolUnavailable)?;
                    effects.extend(observable(RuntimeEvent::ToolCallObserved {
                        call_id: call.call_id.clone(),
                        name: Some(name),
                    }));
                    pending_tools.push(
                        ToolInvocation::new(call, effect)
                            .map_err(|_| RuntimeErrorCode::ProtocolMalformedJson)?,
                    );
                }
                Ok(effects)
            }
            StreamEvent::Terminal { outcome } => {
                if outcome == TerminalOutcome::Completed {
                    let RunState::Running {
                        request,
                        usage,
                        pending_tools,
                    } = &mut self.state
                    else {
                        return Err(RuntimeErrorCode::ProtocolEventAfterTerminal);
                    };
                    if !pending_tools.is_empty() {
                        let invocations = std::mem::take(pending_tools);
                        let effect = RunEffect::BeginTools(invocations.clone());
                        self.state = RunState::AwaitingTools {
                            request: request.clone(),
                            usage: *usage,
                            invocations,
                        };
                        return Ok(vec![effect]);
                    }
                }
                let outcome = match outcome {
                    TerminalOutcome::Completed => RuntimeTerminalOutcome::Completed,
                    TerminalOutcome::Failed { code } => RuntimeTerminalOutcome::Failed {
                        failure: RuntimeFailure::new(code.into()),
                    },
                    TerminalOutcome::Interrupted => RuntimeTerminalOutcome::Interrupted,
                    TerminalOutcome::Stopped => RuntimeTerminalOutcome::Stopped,
                };
                self.finish(outcome, false)
            }
        }
    }

    fn finish(
        &mut self,
        outcome: RuntimeTerminalOutcome,
        abort_provider: bool,
    ) -> Result<Vec<RunEffect>, RuntimeErrorCode> {
        let (request, usage) = match &self.state {
            RunState::Running { request, usage, .. }
            | RunState::AwaitingTools { request, usage, .. } => (request, usage),
            RunState::Idle | RunState::Terminal { .. } => {
                return Err(RuntimeErrorCode::ProtocolEventAfterTerminal);
            }
        };
        let receipt = TurnReceipt {
            session_id: request.session_id.clone(),
            turn_id: request.turn_id.clone(),
            request_id: request.request_id.clone(),
            outcome: outcome.clone(),
            usage: *usage,
        };
        let terminal = RuntimeEventV1::new(RuntimeEvent::Terminal { outcome });
        self.state = RunState::Terminal {
            receipt: receipt.clone(),
        };
        let mut effects = Vec::with_capacity(4);
        if abort_provider {
            effects.push(RunEffect::AbortProvider);
        }
        effects.push(RunEffect::Persist(terminal.clone()));
        effects.push(RunEffect::Publish(terminal));
        effects.push(RunEffect::Finalize(receipt));
        Ok(effects)
    }

    fn continue_after_tools(
        &mut self,
        request: TurnRequest,
    ) -> Result<Vec<RunEffect>, RuntimeErrorCode> {
        let request = request.validate()?;
        let RunState::AwaitingTools {
            request: previous,
            usage,
            invocations,
        } = &self.state
        else {
            return Err(RuntimeErrorCode::ProtocolEventAfterTerminal);
        };
        if request.session_id != previous.session_id
            || request.turn_id != previous.turn_id
            || request.request_id != previous.request_id
            || request.provider_id != previous.provider_id
            || request.model_id != previous.model_id
            || request.protocol != previous.protocol
            || !invocations.iter().all(|invocation| {
                request.messages.iter().any(|message| {
                    matches!(
                        message,
                        ConversationItem::ToolResult(result)
                            if result.tool_result.call_id == invocation.call.call_id
                    )
                })
            })
        {
            return Err(RuntimeErrorCode::Recovery);
        }
        let usage = *usage;
        self.state = RunState::Running {
            request: request.clone(),
            usage,
            pending_tools: Vec::new(),
        };
        Ok(vec![RunEffect::OpenProvider(request)])
    }
}

fn observable(event: RuntimeEvent) -> Vec<RunEffect> {
    let event = RuntimeEventV1::new(event);
    vec![RunEffect::Persist(event.clone()), RunEffect::Publish(event)]
}

fn effect_for_tool(name: &str) -> Option<ToolEffect> {
    match name {
        "read_file" | "list_directory" => Some(ToolEffect::Read),
        "apply_patch" | "write_file" => Some(ToolEffect::Write),
        "run_diagnostic" | "git_status" | "git_diff" | "npm_diagnostic" => {
            Some(ToolEffect::Process)
        }
        _ => None,
    }
}

fn merge_usage(current: Option<Usage>, next: Usage) -> Usage {
    let current = current.unwrap_or_default();
    Usage {
        input_tokens: add_optional(current.input_tokens, next.input_tokens),
        output_tokens: add_optional(current.output_tokens, next.output_tokens),
        total_tokens: add_optional(current.total_tokens, next.total_tokens),
    }
}

fn add_optional(left: Option<u64>, right: Option<u64>) -> Option<u64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.saturating_add(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}
