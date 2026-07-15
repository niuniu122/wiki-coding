import type {Command} from "../protocol.js";

export type KernelPhase =
  | "booting"
  | "idle"
  | "running_mutation"
  | "running_turn"
  | "shutting_down"
  | "stopped";

const NON_MUTATING_COMMANDS = new Set<Command["type"]>([
  "turn.interrupt",
  "thread.list",
  "provider.list",
  "model.list",
  "capability.list",
  "capability.search",
  "permission.show",
  "trace.toggle",
  "app.exit"
]);

export class CommandBusyError extends Error {
  readonly name = "CommandBusyError";

  constructor(
    readonly commandType: Command["type"],
    readonly phase: KernelPhase
  ) {
    super(`Cannot dispatch ${commandType}: runtime is busy (${phase}).`);
  }
}

export class CommandArbiter {
  private phase: KernelPhase = "booting";
  private mutationFinished: Promise<void> = Promise.resolve();
  private resolveMutation: (() => void) | null = null;

  markReady(): void {
    if (this.phase === "booting") {
      this.phase = "idle";
    }
  }

  canDispatch(command: Command): boolean {
    if (this.phase === "booting") {
      return command.type === "app.exit";
    }
    if (this.phase === "idle") {
      return true;
    }
    if (this.phase === "running_turn" || this.phase === "running_mutation") {
      return NON_MUTATING_COMMANDS.has(command.type);
    }
    return false;
  }

  begin(command: Command): {finish(): void} {
    if (!this.canDispatch(command)) {
      throw new CommandBusyError(command.type, this.phase);
    }

    const ownsMutation = !NON_MUTATING_COMMANDS.has(command.type);
    const ownershipPhase: KernelPhase | null = ownsMutation
      ? command.type === "turn.submit"
        || command.type === "agent.submit"
        || command.type === "agent.continue"
        ? "running_turn"
        : "running_mutation"
      : null;
    if (ownershipPhase) {
      this.phase = ownershipPhase;
      this.mutationFinished = new Promise<void>((resolve) => {
        this.resolveMutation = resolve;
      });
    }

    let finished = false;
    return {
      finish: () => {
        if (finished) {
          return;
        }
        finished = true;
        if (ownershipPhase) {
          this.resolveMutation?.();
          this.resolveMutation = null;
        }
        if (ownershipPhase && this.phase === ownershipPhase) {
          this.phase = "idle";
        }
      }
    };
  }

  waitForMutation(): Promise<void> {
    return this.mutationFinished;
  }

  beginShutdown(): void {
    this.phase = "shutting_down";
  }

  markStopped(): void {
    if (this.phase === "shutting_down") {
      this.phase = "stopped";
    }
  }
}
