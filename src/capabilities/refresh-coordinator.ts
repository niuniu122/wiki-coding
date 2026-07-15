import {
  staleCapabilitySnapshot,
  type CapabilitySnapshot
} from "./capability-snapshot.js";
import type {CapabilitySnapshotStore} from "./snapshot-store.js";

export class CapabilityRefreshCoordinator {
  private active: CapabilitySnapshot | undefined;
  private refreshOperation: Promise<CapabilitySnapshot> | undefined;
  private fingerprint: string | undefined;

  constructor(
    private readonly store: CapabilitySnapshotStore,
    private readonly build: () => Promise<CapabilitySnapshot>
  ) {}

  async initialize(): Promise<CapabilitySnapshot | undefined> {
    const stored = await this.store.load();
    if (stored.status === "loaded") {
      this.active = stored.snapshot;
      this.fingerprint = stored.snapshot.fingerprint;
    }
    return this.active;
  }

  getSnapshot(): CapabilitySnapshot | undefined {
    return this.active;
  }

  refresh(nextFingerprint?: string): Promise<CapabilitySnapshot> {
    if (nextFingerprint && nextFingerprint === this.fingerprint && this.active) {
      return Promise.resolve(this.active);
    }
    if (!this.refreshOperation) {
      const operation = this.performRefresh(nextFingerprint);
      const tracked = operation.finally(() => {
        if (this.refreshOperation === tracked) this.refreshOperation = undefined;
      });
      this.refreshOperation = tracked;
    }
    return this.refreshOperation;
  }

  private async performRefresh(nextFingerprint?: string): Promise<CapabilitySnapshot> {
    try {
      const candidate = await this.build();
      await this.store.save(candidate);
      this.active = candidate;
      this.fingerprint = nextFingerprint ?? candidate.fingerprint;
      return candidate;
    } catch (error) {
      if (!this.active) throw error;
      this.active = staleCapabilitySnapshot(this.active, "refresh_failed");
      return this.active;
    }
  }
}
