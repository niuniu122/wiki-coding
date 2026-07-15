use minimax_protocol::{
    RuntimeErrorCode, RuntimeEvent, RuntimeEventV1, RuntimeFailure, RuntimeTerminalOutcome,
    StreamEvent, TerminalOutcome, TurnReceipt, TurnRequest, Usage,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RunState {
    Idle,
    Running {
        request: TurnRequest,
        usage: Option<Usage>,
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
    Cancel,
    Shutdown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RunEffect {
    Persist(RuntimeEventV1),
    Publish(RuntimeEventV1),
    OpenProvider(TurnRequest),
    AbortProvider,
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
                if let RunState::Running {
                    usage: active_usage,
                    ..
                } = &mut self.state
                {
                    *active_usage = Some(usage);
                }
                Ok(observable(RuntimeEvent::Usage { usage }))
            }
            StreamEvent::ToolCallFragments { fragments } => {
                let mut effects = Vec::new();
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
                Ok(effects)
            }
            StreamEvent::Terminal { outcome } => {
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
        let RunState::Running { request, usage } = &self.state else {
            return Err(RuntimeErrorCode::ProtocolEventAfterTerminal);
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
}

fn observable(event: RuntimeEvent) -> Vec<RunEffect> {
    let event = RuntimeEventV1::new(event);
    vec![RunEffect::Persist(event.clone()), RunEffect::Publish(event)]
}
