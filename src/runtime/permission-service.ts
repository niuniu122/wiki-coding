export type PermissionMode = "confirm" | "workspace_read" | "full_access";

export class PermissionService {
  private mode: PermissionMode = "confirm";
  get current(): PermissionMode { return this.mode; }
  set(mode: PermissionMode): PermissionMode { this.mode = mode; return this.mode; }
  resetSession(): void { this.mode = "confirm"; }
}
