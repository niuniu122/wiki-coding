import type {Command, RuntimeEvent} from "../protocol.js";
import {ApplicationKernel} from "./application-kernel.js";
import type {
  RuntimeApplication,
  ShutdownReason
} from "./runtime-application.js";

export class CommandDispatcher {
  constructor(
    private readonly application: RuntimeApplication = new ApplicationKernel()
  ) {}

  async init(): Promise<RuntimeEvent[]> {
    try {
      return await this.application.init();
    } catch (error) {
      return [createInitializationFailureEvent(error)];
    }
  }

  async *dispatch(command: Command): AsyncGenerator<RuntimeEvent> {
    yield* this.application.dispatch(command);
  }

  shutdown(reason: ShutdownReason): Promise<void> {
    return this.application.shutdown(reason);
  }
}

export function createInitializationFailureEvent(error: unknown): Extract<
  RuntimeEvent,
  {type: "runtime.init_failed"}
> {
  return {
    type: "runtime.init_failed",
    message: safeInitializationFailureMessage(error)
  };
}

const GENERIC_INITIALIZATION_FAILURE = "Runtime initialization failed.";

function safeInitializationFailureMessage(error: unknown): string {
  try {
    const message = error instanceof Error ? error.message : String(error);
    return typeof message === "string"
      ? redactInitializationFailure(message)
      : GENERIC_INITIALIZATION_FAILURE;
  } catch {
    return GENERIC_INITIALIZATION_FAILURE;
  }
}

function redactInitializationFailure(message: string): string {
  return message
    .replace(
      /-----BEGIN [^-]+ PRIVATE KEY-----[\s\S]*?-----END [^-]+ PRIVATE KEY-----/giu,
      "[REDACTED]"
    )
    .replace(
      /\b((?:API_?KEY|PASSWORD|SECRET|TOKEN|AUTHORIZATION))\b\s*[:=]\s*[^\r\n]*/giu,
      "$1=[REDACTED]"
    )
    .replace(/\bBearer\s+[^\s,;]+/giu, "Bearer [REDACTED]")
    .replace(/\bsk-[A-Za-z0-9_-]{8,}\b/gu, "[REDACTED]")
    .replace(
      /\b(?:gh[pousr]_[A-Za-z0-9]{20,255}|github_pat_[A-Za-z0-9_]{20,255}|glpat-[A-Za-z0-9_-]{20,255})\b/gu,
      "[REDACTED]"
    )
    .replace(
      /\b(?:ABIA|ACCA|AGPA|AIDA|AIPA|AKIA|ANPA|ANVA|APKA|AROA|ASCA|ASIA)[A-Z0-9]{16}\b/gu,
      "[REDACTED]"
    )
    .replace(/[A-Za-z0-9][A-Za-z0-9_+./=-]{31,511}/gu, (candidate) =>
      isHighEntropyToken(candidate) ? "[REDACTED]" : candidate
    );
}

function isHighEntropyToken(candidate: string): boolean {
  const classes = [/[a-z]/u, /[A-Z]/u, /[0-9]/u, /[_+./=-]/u]
    .filter((pattern) => pattern.test(candidate)).length;
  if (classes < 3 || new Set(candidate).size < 10) {
    return false;
  }
  const counts = new Map<string, number>();
  for (const character of candidate) {
    counts.set(character, (counts.get(character) ?? 0) + 1);
  }
  const entropy = Array.from(counts.values()).reduce((total, count) => {
    const probability = count / candidate.length;
    return total - probability * Math.log2(probability);
  }, 0);
  return entropy >= 3.5;
}
