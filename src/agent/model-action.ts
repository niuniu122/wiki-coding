export interface ModelToolDefinition {
  readonly name: string;
  readonly description: string;
  readonly inputSchema: Readonly<Record<string, unknown>>;
}

export interface ModelToolCall {
  readonly callId: string;
  readonly name: string;
  readonly argumentsJson: string;
}

export type ModelAction =
  | {readonly type: "text"; readonly delta: string}
  | {readonly type: "tool_call"; readonly call: ModelToolCall}
  | {readonly type: "usage"; readonly inputTokens?: number; readonly outputTokens?: number; readonly totalTokens?: number}
  | {readonly type: "completed"}
  | {readonly type: "failure"; readonly code: string};

export function parseToolArguments(call: ModelToolCall): Readonly<Record<string, unknown>> {
  let value: unknown;
  try { value = JSON.parse(call.argumentsJson); } catch { throw new Error("Malformed tool-call arguments."); }
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error("Tool-call arguments must be an object.");
  return Object.freeze(value as Record<string, unknown>);
}
