use std::collections::BTreeSet;

use minimax_protocol::{
    AgentLimits, SchemaVersion, ToolCallId, ToolDecision, ToolDecisionKind, ToolInvocation,
    ToolResult, ToolTerminalStatus,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PermissionMode {
    #[default]
    Confirm,
    FullAccess,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolSandboxPolicy {
    Restricted,
    Disabled,
}

impl PermissionMode {
    #[must_use]
    pub const fn sandbox_policy(self) -> ToolSandboxPolicy {
        match self {
            Self::Confirm => ToolSandboxPolicy::Restricted,
            Self::FullAccess => ToolSandboxPolicy::Disabled,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecisionSnapshot {
    pub decision: ToolDecision,
    pub permission_mode: PermissionMode,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InvocationState {
    Requested,
    AwaitingDecision,
    Approved { snapshot: DecisionSnapshot },
    Started { snapshot: DecisionSnapshot },
    Terminal { result: ToolResult },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InvocationInput {
    PreflightAllowed {
        permission_mode: PermissionMode,
    },
    PreflightDenied {
        result: ToolResult,
    },
    PreStartFailed {
        result: ToolResult,
    },
    Decision {
        decision: ToolDecision,
        permission_mode: PermissionMode,
    },
    Start,
    Complete {
        result: ToolResult,
    },
    Cancel,
    Recover,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InvocationEffect {
    PersistRequested(ToolInvocation),
    RequestApproval(ToolInvocation),
    PersistDecision(ToolDecision),
    PersistStarted(ToolCallId),
    Execute {
        invocation: ToolInvocation,
        sandbox_policy: ToolSandboxPolicy,
    },
    PersistTerminal(ToolResult),
    PublishTerminal(ToolResult),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvocationError {
    InvalidTransition,
    WrongCallId,
    WrongToolName,
    DuplicateCallId,
    DuplicateDecision,
    DuplicateStarted,
    DuplicateTerminal,
    InvalidTerminal,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvocationMachine {
    invocation: ToolInvocation,
    state: InvocationState,
}

impl InvocationMachine {
    #[must_use]
    pub fn request(invocation: ToolInvocation) -> (Self, Vec<InvocationEffect>) {
        (
            Self {
                invocation: invocation.clone(),
                state: InvocationState::Requested,
            },
            vec![InvocationEffect::PersistRequested(invocation)],
        )
    }

    #[must_use]
    pub const fn state(&self) -> &InvocationState {
        &self.state
    }

    #[must_use]
    pub const fn invocation(&self) -> &ToolInvocation {
        &self.invocation
    }

    pub fn apply(
        &mut self,
        input: InvocationInput,
    ) -> Result<Vec<InvocationEffect>, InvocationError> {
        match (&self.state, input) {
            (
                InvocationState::Requested,
                InvocationInput::PreflightAllowed {
                    permission_mode: PermissionMode::Confirm,
                },
            ) => {
                self.state = InvocationState::AwaitingDecision;
                Ok(vec![InvocationEffect::RequestApproval(
                    self.invocation.clone(),
                )])
            }
            (
                InvocationState::Requested,
                InvocationInput::PreflightAllowed {
                    permission_mode: PermissionMode::FullAccess,
                },
            ) => self.approve_full_access(),
            (InvocationState::Requested, InvocationInput::PreflightDenied { result }) => {
                self.terminate_preflight(result)
            }
            (InvocationState::Approved { .. }, InvocationInput::PreStartFailed { result }) => {
                self.terminate_preflight(result)
            }
            (
                InvocationState::AwaitingDecision,
                InvocationInput::Decision {
                    decision,
                    permission_mode,
                },
            ) => self.record_decision(decision, permission_mode),
            (InvocationState::Approved { snapshot }, InvocationInput::Start) => {
                let snapshot = snapshot.clone();
                let sandbox_policy = snapshot.permission_mode.sandbox_policy();
                self.state = InvocationState::Started { snapshot };
                Ok(vec![
                    InvocationEffect::PersistStarted(self.invocation.call.call_id.clone()),
                    InvocationEffect::Execute {
                        invocation: self.invocation.clone(),
                        sandbox_policy,
                    },
                ])
            }
            (InvocationState::Started { .. }, InvocationInput::Complete { result }) => {
                self.terminate_started(result)
            }
            (
                InvocationState::Requested
                | InvocationState::AwaitingDecision
                | InvocationState::Approved { .. },
                InvocationInput::Cancel,
            ) => Ok(self.cancel_pre_start()),
            (InvocationState::Started { .. }, InvocationInput::Cancel) => {
                Ok(self.terminate_unknown_effect())
            }
            (
                InvocationState::Requested
                | InvocationState::AwaitingDecision
                | InvocationState::Approved { .. },
                InvocationInput::Recover,
            ) => Ok(self.recover_pre_start()),
            (InvocationState::Started { .. }, InvocationInput::Recover) => {
                Ok(self.terminate_unknown_effect())
            }
            (InvocationState::Terminal { .. }, InvocationInput::Recover) => Ok(Vec::new()),
            (
                InvocationState::Approved { .. } | InvocationState::Started { .. },
                InvocationInput::Decision { .. },
            ) => Err(InvocationError::DuplicateDecision),
            (InvocationState::Started { .. }, InvocationInput::Start) => {
                Err(InvocationError::DuplicateStarted)
            }
            (InvocationState::Terminal { .. }, InvocationInput::Complete { .. }) => {
                Err(InvocationError::DuplicateTerminal)
            }
            (InvocationState::Terminal { .. }, _) => Err(InvocationError::DuplicateTerminal),
            _ => Err(InvocationError::InvalidTransition),
        }
    }

    fn approve_full_access(&mut self) -> Result<Vec<InvocationEffect>, InvocationError> {
        let decision = ToolDecision {
            schema_version: SchemaVersion,
            call_id: self.invocation.call.call_id.clone(),
            decision: ToolDecisionKind::Approved,
            code: "policy_approved".to_owned(),
        };
        self.record_decision(decision, PermissionMode::FullAccess)
    }

    fn record_decision(
        &mut self,
        decision: ToolDecision,
        permission_mode: PermissionMode,
    ) -> Result<Vec<InvocationEffect>, InvocationError> {
        if decision.call_id != self.invocation.call.call_id {
            return Err(InvocationError::WrongCallId);
        }
        let decision = decision
            .validate()
            .map_err(|_| InvocationError::InvalidTransition)?;
        let persist = InvocationEffect::PersistDecision(decision.clone());
        if decision.decision == ToolDecisionKind::Approved {
            self.state = InvocationState::Approved {
                snapshot: DecisionSnapshot {
                    decision,
                    permission_mode,
                },
            };
            Ok(vec![persist])
        } else {
            let result = terminal_result(
                &self.invocation,
                ToolTerminalStatus::Rejected,
                &decision.code,
            );
            self.state = InvocationState::Terminal {
                result: result.clone(),
            };
            Ok(vec![
                persist,
                InvocationEffect::PersistTerminal(result.clone()),
                InvocationEffect::PublishTerminal(result),
            ])
        }
    }

    fn terminate_preflight(
        &mut self,
        result: ToolResult,
    ) -> Result<Vec<InvocationEffect>, InvocationError> {
        validate_result(&self.invocation, &result)?;
        if result.status == ToolTerminalStatus::Succeeded {
            return Err(InvocationError::InvalidTerminal);
        }
        self.state = InvocationState::Terminal {
            result: result.clone(),
        };
        Ok(vec![
            InvocationEffect::PersistTerminal(result.clone()),
            InvocationEffect::PublishTerminal(result),
        ])
    }

    fn terminate_started(
        &mut self,
        result: ToolResult,
    ) -> Result<Vec<InvocationEffect>, InvocationError> {
        validate_result(&self.invocation, &result)?;
        self.state = InvocationState::Terminal {
            result: result.clone(),
        };
        Ok(vec![
            InvocationEffect::PersistTerminal(result.clone()),
            InvocationEffect::PublishTerminal(result),
        ])
    }

    fn cancel_pre_start(&mut self) -> Vec<InvocationEffect> {
        let mut effects = Vec::new();
        if matches!(self.state, InvocationState::AwaitingDecision) {
            let decision = ToolDecision {
                schema_version: SchemaVersion,
                call_id: self.invocation.call.call_id.clone(),
                decision: ToolDecisionKind::Rejected,
                code: "cancelled".to_owned(),
            };
            effects.push(InvocationEffect::PersistDecision(decision));
        }
        let result = terminal_result(
            &self.invocation,
            ToolTerminalStatus::Cancelled,
            "cancelled_before_start",
        );
        self.state = InvocationState::Terminal {
            result: result.clone(),
        };
        effects.push(InvocationEffect::PersistTerminal(result.clone()));
        effects.push(InvocationEffect::PublishTerminal(result));
        effects
    }

    fn recover_pre_start(&mut self) -> Vec<InvocationEffect> {
        let result = terminal_result(
            &self.invocation,
            ToolTerminalStatus::Cancelled,
            "recovered_before_start",
        );
        self.state = InvocationState::Terminal {
            result: result.clone(),
        };
        vec![
            InvocationEffect::PersistTerminal(result.clone()),
            InvocationEffect::PublishTerminal(result),
        ]
    }

    fn terminate_unknown_effect(&mut self) -> Vec<InvocationEffect> {
        let result = terminal_result(
            &self.invocation,
            ToolTerminalStatus::Indeterminate,
            "effect_unknown",
        );
        self.state = InvocationState::Terminal {
            result: result.clone(),
        };
        vec![
            InvocationEffect::PersistTerminal(result.clone()),
            InvocationEffect::PublishTerminal(result),
        ]
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct InvocationRegistry {
    call_ids: BTreeSet<ToolCallId>,
}

impl InvocationRegistry {
    pub fn register(&mut self, invocation: &ToolInvocation) -> Result<(), InvocationError> {
        if !self.call_ids.insert(invocation.call.call_id.clone()) {
            return Err(InvocationError::DuplicateCallId);
        }
        Ok(())
    }

    #[must_use]
    pub fn contains(&self, call_id: &ToolCallId) -> bool {
        self.call_ids.contains(call_id)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BudgetKind {
    InvalidLimits,
    ProviderRounds,
    ToolCalls,
    Elapsed,
    ToolResultBytes,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AgentBudgetError {
    pub kind: BudgetKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentBudget {
    limits: AgentLimits,
    started_at_unix_ms: u64,
    provider_rounds: u16,
    tool_calls: u16,
    tool_result_bytes: u64,
}

impl AgentBudget {
    pub fn new(limits: AgentLimits, started_at_unix_ms: u64) -> Result<Self, AgentBudgetError> {
        limits.validate().map_err(|_| AgentBudgetError {
            kind: BudgetKind::InvalidLimits,
        })?;
        Ok(Self {
            limits,
            started_at_unix_ms,
            provider_rounds: 0,
            tool_calls: 0,
            tool_result_bytes: 0,
        })
    }

    pub fn consume_provider_round(&mut self, now_unix_ms: u64) -> Result<(), AgentBudgetError> {
        self.check_elapsed(now_unix_ms)?;
        if self.provider_rounds >= self.limits.max_provider_rounds {
            return Err(AgentBudgetError {
                kind: BudgetKind::ProviderRounds,
            });
        }
        self.provider_rounds += 1;
        Ok(())
    }

    pub fn consume_tool_call(&mut self, now_unix_ms: u64) -> Result<(), AgentBudgetError> {
        self.check_elapsed(now_unix_ms)?;
        if self.tool_calls >= self.limits.max_tool_calls {
            return Err(AgentBudgetError {
                kind: BudgetKind::ToolCalls,
            });
        }
        self.tool_calls += 1;
        Ok(())
    }

    pub fn consume_result_bytes(
        &mut self,
        bytes: usize,
        now_unix_ms: u64,
    ) -> Result<(), AgentBudgetError> {
        self.check_elapsed(now_unix_ms)?;
        let bytes = u64::try_from(bytes).map_err(|_| AgentBudgetError {
            kind: BudgetKind::ToolResultBytes,
        })?;
        let next = self
            .tool_result_bytes
            .checked_add(bytes)
            .ok_or(AgentBudgetError {
                kind: BudgetKind::ToolResultBytes,
            })?;
        if next > self.limits.max_tool_result_bytes {
            return Err(AgentBudgetError {
                kind: BudgetKind::ToolResultBytes,
            });
        }
        self.tool_result_bytes = next;
        Ok(())
    }

    #[must_use]
    pub fn failure_result(
        &self,
        invocation: &ToolInvocation,
        error: AgentBudgetError,
    ) -> ToolResult {
        let code = match error.kind {
            BudgetKind::InvalidLimits => "invalid_agent_limits",
            BudgetKind::ProviderRounds => "provider_round_budget_exhausted",
            BudgetKind::ToolCalls => "tool_call_budget_exhausted",
            BudgetKind::Elapsed => "elapsed_budget_exhausted",
            BudgetKind::ToolResultBytes => "tool_result_budget_exhausted",
        };
        terminal_result(invocation, ToolTerminalStatus::Failed, code)
    }

    fn check_elapsed(&self, now_unix_ms: u64) -> Result<(), AgentBudgetError> {
        if now_unix_ms.saturating_sub(self.started_at_unix_ms) > self.limits.max_elapsed_ms {
            return Err(AgentBudgetError {
                kind: BudgetKind::Elapsed,
            });
        }
        Ok(())
    }
}

fn validate_result(
    invocation: &ToolInvocation,
    result: &ToolResult,
) -> Result<(), InvocationError> {
    result
        .clone()
        .validate()
        .map_err(|_| InvocationError::InvalidTerminal)?;
    if result.call_id != invocation.call.call_id {
        return Err(InvocationError::WrongCallId);
    }
    if result.tool_name != invocation.call.name {
        return Err(InvocationError::WrongToolName);
    }
    Ok(())
}

fn terminal_result(
    invocation: &ToolInvocation,
    status: ToolTerminalStatus,
    code: &str,
) -> ToolResult {
    ToolResult {
        schema_version: SchemaVersion,
        call_id: invocation.call.call_id.clone(),
        tool_name: invocation.call.name.clone(),
        status,
        code: code.to_owned(),
        output: None,
    }
}
