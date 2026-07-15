import type {ShutdownReason} from "./runtime-application.js";

export type RuntimeSignal = "SIGINT" | "SIGTERM";

export interface SignalSource {
  on(signal: RuntimeSignal, listener: () => void): void;
  off(signal: RuntimeSignal, listener: () => void): void;
}

export interface ShutdownCoordinatorOptions {
  source: SignalSource;
  shutdown(reason: ShutdownReason): Promise<void>;
  afterShutdown?(): void | Promise<void>;
}

export class ShutdownCoordinator {
  private operation: Promise<void> | null = null;
  private installed = false;
  private readonly handleSignal = (): void => {
    void this.request("signal").catch(() => undefined);
  };

  constructor(private readonly options: ShutdownCoordinatorOptions) {}

  get pending(): Promise<void> {
    return this.operation ?? Promise.resolve();
  }

  install(): void {
    if (this.installed) {
      return;
    }
    this.installed = true;
    this.options.source.on("SIGINT", this.handleSignal);
    this.options.source.on("SIGTERM", this.handleSignal);
  }

  uninstall(): void {
    if (!this.installed) {
      return;
    }
    this.installed = false;
    this.options.source.off("SIGINT", this.handleSignal);
    this.options.source.off("SIGTERM", this.handleSignal);
  }

  request(reason: ShutdownReason): Promise<void> {
    if (!this.operation) {
      this.operation = this.performShutdown(reason);
    }
    return this.operation;
  }

  private async performShutdown(reason: ShutdownReason): Promise<void> {
    let failure: unknown;
    try {
      await this.options.shutdown(reason);
    } catch (error) {
      failure = error;
    } finally {
      this.uninstall();
      await this.options.afterShutdown?.();
    }
    if (failure !== undefined) {
      throw failure;
    }
  }
}
