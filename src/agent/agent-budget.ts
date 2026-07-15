export interface AgentBudgetLimits {
  readonly maxSteps: number;
  readonly maxToolCalls: number;
  readonly maxTotalTokens: number;
  readonly timeoutMs: number;
}

export const DEFAULT_AGENT_BUDGET_LIMITS: AgentBudgetLimits = Object.freeze({
  maxSteps: 8,
  maxToolCalls: 12,
  maxTotalTokens: 48_000,
  timeoutMs: 120_000
});

export type AgentBudgetErrorCode = "step_limit" | "tool_limit" | "token_limit" | "time_limit";

export class AgentBudgetError extends Error {
  constructor(readonly code: AgentBudgetErrorCode) {
    super(`Agent budget exhausted (${code}).`);
    this.name = "AgentBudgetError";
  }
}

export class AgentBudget {
  private steps = 0;
  private toolCalls = 0;
  private tokens = 0;
  private readonly startedAt: number;
  readonly limits: AgentBudgetLimits;

  constructor(limits: AgentBudgetLimits = DEFAULT_AGENT_BUDGET_LIMITS, private readonly now: () => number = Date.now, initial: {steps?: number; toolCalls?: number; tokens?: number} = {}) {
    validateLimits(limits);
    this.limits = Object.freeze({...limits});
    this.steps = initial.steps ?? 0;
    this.toolCalls = initial.toolCalls ?? 0;
    this.tokens = initial.tokens ?? 0;
    if (this.steps < 0 || this.steps > limits.maxSteps || this.toolCalls < 0 || this.toolCalls > limits.maxToolCalls || this.tokens < 0 || this.tokens > limits.maxTotalTokens) {
      throw new Error("Invalid restored Agent budget.");
    }
    this.startedAt = now();
  }

  beginStep(): number {
    this.assertTime();
    if (this.steps >= this.limits.maxSteps) throw new AgentBudgetError("step_limit");
    this.steps += 1;
    return this.steps;
  }

  consumeToolCall(): number {
    this.assertTime();
    if (this.toolCalls >= this.limits.maxToolCalls) throw new AgentBudgetError("tool_limit");
    this.toolCalls += 1;
    return this.toolCalls;
  }

  recordUsage(tokens: number): number {
    this.assertTime();
    if (!Number.isSafeInteger(tokens) || tokens < 0) throw new Error("Invalid Agent token usage.");
    this.tokens += tokens;
    if (this.tokens > this.limits.maxTotalTokens) throw new AgentBudgetError("token_limit");
    return this.tokens;
  }

  assertTime(): void {
    if (this.expired) throw new AgentBudgetError("time_limit");
  }

  get expired(): boolean {
    return this.now() - this.startedAt >= this.limits.timeoutMs;
  }

  get snapshot(): {steps: number; toolCalls: number; tokens: number; remainingToolCalls: number} {
    return Object.freeze({steps: this.steps, toolCalls: this.toolCalls, tokens: this.tokens, remainingToolCalls: Math.max(0, this.limits.maxToolCalls - this.toolCalls)});
  }
}

function validateLimits(limits: AgentBudgetLimits): void {
  for (const [key, value] of Object.entries(limits)) {
    if (!Number.isSafeInteger(value) || value <= 0) throw new Error(`Invalid Agent budget (${key}).`);
  }
  if (limits.maxSteps > 100 || limits.maxToolCalls > 100 || limits.maxTotalTokens > 2_000_000 || limits.timeoutMs > 30 * 60_000) {
    throw new Error("Agent budget exceeds the hard safety ceiling.");
  }
}
