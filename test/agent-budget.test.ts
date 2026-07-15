import assert from "node:assert/strict";
import test from "node:test";
import {AgentBudget, AgentBudgetError} from "../src/agent/agent-budget.js";

test("Agent budget enforces step, tool and cumulative token ceilings", () => {
  const steps = new AgentBudget({maxSteps: 1, maxToolCalls: 2, maxTotalTokens: 10, timeoutMs: 1_000});
  assert.equal(steps.beginStep(), 1);
  assert.throws(() => steps.beginStep(), (error: unknown) => error instanceof AgentBudgetError && error.code === "step_limit");

  const tools = new AgentBudget({maxSteps: 2, maxToolCalls: 1, maxTotalTokens: 10, timeoutMs: 1_000});
  assert.equal(tools.consumeToolCall(), 1);
  assert.throws(() => tools.consumeToolCall(), (error: unknown) => error instanceof AgentBudgetError && error.code === "tool_limit");

  const tokens = new AgentBudget({maxSteps: 2, maxToolCalls: 2, maxTotalTokens: 5, timeoutMs: 1_000});
  tokens.recordUsage(3);
  assert.throws(() => tokens.recordUsage(3), (error: unknown) => error instanceof AgentBudgetError && error.code === "token_limit");
});

test("Agent budget uses a deterministic monotonic deadline", () => {
  let now = 100;
  const budget = new AgentBudget({maxSteps: 2, maxToolCalls: 2, maxTotalTokens: 10, timeoutMs: 50}, () => now);
  now = 149;
  assert.equal(budget.expired, false);
  now = 150;
  assert.equal(budget.expired, true);
  assert.throws(() => budget.assertTime(), (error: unknown) => error instanceof AgentBudgetError && error.code === "time_limit");
});

test("Agent budgets reject unsafe or nonsensical limits", () => {
  assert.throws(() => new AgentBudget({maxSteps: 0, maxToolCalls: 1, maxTotalTokens: 1, timeoutMs: 1}));
  assert.throws(() => new AgentBudget({maxSteps: 101, maxToolCalls: 1, maxTotalTokens: 1, timeoutMs: 1}));
});
