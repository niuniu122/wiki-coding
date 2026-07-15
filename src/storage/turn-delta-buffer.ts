export interface TurnDeltaBufferOptions {
  delayMs?: number;
  maxCharacters?: number;
}

export class TurnDeltaBuffer {
  private readonly delayMs: number;
  private readonly maxCharacters: number;
  private pending = "";
  private pendingCreatedAt: string | null = null;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private inFlight: Promise<void> | null = null;
  private timerFailure: unknown;
  private closed = false;

  constructor(
    private readonly persist: (delta: string, createdAt: string) => Promise<void>,
    options: TurnDeltaBufferOptions = {}
  ) {
    this.delayMs = options.delayMs ?? 250;
    this.maxCharacters = options.maxCharacters ?? 1024;
    if (!Number.isFinite(this.delayMs) || this.delayMs < 0) {
      throw new Error("Turn delta flush delay must be a non-negative number.");
    }
    if (!Number.isSafeInteger(this.maxCharacters) || this.maxCharacters < 1) {
      throw new Error("Turn delta character limit must be a positive safe integer.");
    }
  }

  async push(delta: string): Promise<void> {
    this.throwTimerFailure();
    if (this.closed) {
      throw new Error("Turn delta buffer is closed.");
    }
    if (!delta) {
      return;
    }
    if (!this.pending) {
      this.pendingCreatedAt = new Date().toISOString();
      this.armTimer();
    }
    this.pending += delta;
    if (this.pending.length >= this.maxCharacters) {
      await this.flush();
    }
  }

  async flush(): Promise<void> {
    this.clearTimer();
    this.timerFailure = undefined;
    await this.drainPending(false);
  }

  private async drainPending(timerAttempt: boolean): Promise<void> {
    while (this.inFlight) {
      try {
        await this.inFlight;
      } catch {
        // The owning drain restored its failed batch before rejecting.
      }
      this.timerFailure = undefined;
    }
    if (!this.pending) {
      return;
    }

    const delta = this.pending;
    const createdAt = this.pendingCreatedAt ?? new Date().toISOString();
    this.pending = "";
    this.pendingCreatedAt = null;
    const operation = this.persist(delta, createdAt);
    this.inFlight = operation;
    try {
      await operation;
    } catch (error) {
      this.pending = `${delta}${this.pending}`;
      this.pendingCreatedAt = createdAt;
      if (timerAttempt) {
        this.timerFailure = error;
      }
      throw error;
    } finally {
      if (this.inFlight === operation) {
        this.inFlight = null;
      }
    }
  }

  async close(): Promise<void> {
    if (this.closed) {
      await this.flush();
      return;
    }
    this.closed = true;
    await this.flush();
  }

  private armTimer(): void {
    this.timer = setTimeout(() => {
      this.timer = null;
      void this.drainPending(true).catch(() => undefined);
    }, this.delayMs);
  }

  private clearTimer(): void {
    if (this.timer) {
      clearTimeout(this.timer);
      this.timer = null;
    }
  }

  private throwTimerFailure(): void {
    if (this.timerFailure !== undefined) {
      const error = this.timerFailure;
      this.timerFailure = undefined;
      if (this.pending && !this.closed && !this.timer && !this.inFlight) {
        this.armTimer();
      }
      throw error;
    }
  }
}
