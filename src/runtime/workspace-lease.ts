import {createHash, randomUUID} from "node:crypto";
import {
  access,
  mkdir,
  readFile,
  rename as fsRename,
  rm as fsRemove
} from "node:fs/promises";
import {dirname, join} from "node:path";
import {writeJsonFile} from "../utils/jsonl.js";

export interface WorkspaceLeaseFileOperations {
  rename(from: string, to: string): Promise<void>;
  remove(path: string): Promise<void>;
  writeOwner(path: string, owner: unknown): Promise<void>;
}

export interface WorkspaceLeaseOptions {
  pid?: number;
  isProcessAlive?: (pid: number) => boolean;
  now?: () => string;
  fileOperations?: Partial<WorkspaceLeaseFileOperations>;
}

interface WorkspaceOwner {
  pid: number;
  startedAt: string;
  workspace: string;
  nonce: string;
}

interface OperationOwner extends WorkspaceOwner {
  kind: "workspace_operation";
  targetIdentity: string;
}

type OwnerState<T> =
  | {kind: "valid"; owner: T}
  | {kind: "missing"}
  | {kind: "invalid"; fingerprint: string};

interface OperationAuthority {
  directory: string;
  owner: OperationOwner;
}

export class WorkspaceLease {
  private nonce: string | null = null;
  private releaseOperation: Promise<void> | null = null;

  constructor(
    private readonly stateRoot: string,
    private readonly options: WorkspaceLeaseOptions = {}
  ) {}

  async acquire(): Promise<void> {
    if (this.releaseOperation !== null) {
      await this.releaseOperation;
    }
    if (this.nonce !== null) {
      return;
    }

    const lockDir = this.lockDir();
    await mkdir(dirname(lockDir), {recursive: true});
    const owner: WorkspaceOwner = {
      pid: this.pid(),
      startedAt: this.now(),
      workspace: this.stateRoot,
      nonce: randomUUID()
    };
    const candidateDir = `${lockDir}.candidate.${owner.nonce}`;
    await this.prepareOwnerDirectory(candidateDir, owner);
    let published = false;

    try {
      while (!published) {
        published = await this.tryPublish(candidateDir, lockDir);
        if (!published) {
          await this.recoverWorkspaceLock(lockDir);
        }
      }
      this.nonce = owner.nonce;
    } finally {
      if (!published) {
        await this.remove(candidateDir);
      }
    }
  }

  release(): Promise<void> {
    if (this.releaseOperation !== null) {
      return this.releaseOperation;
    }

    const nonce = this.nonce;
    if (nonce === null) {
      return Promise.resolve();
    }
    this.nonce = null;

    const operation = this.releaseOwnedLock(nonce).catch(async (error: unknown) => {
      let stillOwnsLock = true;
      try {
        stillOwnsLock = await this.isCanonicalOwner(nonce);
      } catch {
        // Preserve retryability when ownership cannot be determined after a failure.
      }
      if (stillOwnsLock && this.nonce === null) {
        this.nonce = nonce;
      }
      throw error;
    });
    const trackedOperation = operation.finally(() => {
      if (this.releaseOperation === trackedOperation) {
        this.releaseOperation = null;
      }
    });
    this.releaseOperation = trackedOperation;
    return trackedOperation;
  }

  private lockDir(): string {
    return join(this.stateRoot, "locks", "runtime.lock");
  }

  private operationDir(lockDir: string): string {
    return `${lockDir}.operation`;
  }

  private pid(): number {
    return this.options.pid ?? process.pid;
  }

  private now(): string {
    return this.options.now?.() ?? new Date().toISOString();
  }

  private isProcessAlive(pid: number): boolean {
    return this.options.isProcessAlive?.(pid) ?? isProcessAlive(pid);
  }

  private async recoverWorkspaceLock(lockDir: string): Promise<void> {
    const ownerPath = join(lockDir, "owner.json");
    const observed = await readWorkspaceOwner(ownerPath);
    if (observed.kind === "missing" && !(await pathExists(lockDir))) {
      return;
    }
    if (hasLiveOwner(observed, (pid) => this.isProcessAlive(pid))) {
      throw workspaceOpenError(observed.owner);
    }

    const targetIdentity = ownerStateIdentity(observed);
    const authority = await this.acquireOperationAuthority(
      lockDir,
      targetIdentity
    );
    try {
      const revalidated = await readWorkspaceOwner(ownerPath);
      if (revalidated.kind === "missing" && !(await pathExists(lockDir))) {
        return;
      }
      if (!sameOwnerState(observed, revalidated)) {
        if (hasLiveOwner(revalidated, (pid) => this.isProcessAlive(pid))) {
          throw workspaceOpenError(revalidated.owner);
        }
        return;
      }
      if (hasLiveOwner(revalidated, (pid) => this.isProcessAlive(pid))) {
        throw workspaceOpenError(revalidated.owner);
      }

      await this.assertOperationAuthority(authority);
      const staleDir = `${lockDir}.stale.${randomUUID()}`;
      try {
        await this.rename(lockDir, staleDir);
      } catch (error) {
        if (isErrorCode(error, "ENOENT")) {
          return;
        }
        throw error;
      }

      const moved = await readWorkspaceOwner(join(staleDir, "owner.json"));
      if (
        !sameOwnerState(revalidated, moved) ||
        hasLiveOwner(moved, (pid) => this.isProcessAlive(pid))
      ) {
        await this.restoreMovedDirectory(staleDir, lockDir);
        if (hasLiveOwner(moved, (pid) => this.isProcessAlive(pid))) {
          throw workspaceOpenError(moved.owner);
        }
        throw new Error("Workspace ownership changed during stale recovery.");
      }
      await this.remove(staleDir);
    } finally {
      await this.releaseOperationAuthority(authority);
    }
  }

  private async releaseOwnedLock(nonce: string): Promise<void> {
    const lockDir = this.lockDir();
    const targetIdentity = `owner:${nonce}`;
    const authority = await this.acquireOperationAuthority(
      lockDir,
      targetIdentity
    );
    try {
      const owner = await readWorkspaceOwner(join(lockDir, "owner.json"));
      if (owner.kind !== "valid" || owner.owner.nonce !== nonce) {
        return;
      }
      await this.assertOperationAuthority(authority);
      await this.remove(lockDir);
    } finally {
      await this.releaseOperationAuthority(authority);
    }
  }

  private async acquireOperationAuthority(
    lockDir: string,
    targetIdentity: string
  ): Promise<OperationAuthority> {
    const operationDir = this.operationDir(lockDir);
    const owner: OperationOwner = {
      kind: "workspace_operation",
      pid: this.pid(),
      startedAt: this.now(),
      workspace: this.stateRoot,
      nonce: randomUUID(),
      targetIdentity
    };
    const candidateDir = `${operationDir}.candidate.${owner.nonce}`;
    await this.prepareOwnerDirectory(candidateDir, owner);
    let published = false;

    try {
      while (!published) {
        published = await this.tryPublish(candidateDir, operationDir);
        if (published) {
          return {directory: operationDir, owner};
        }

        const existing = await readOperationOwner(join(operationDir, "owner.json"));
        if (hasLiveOwner(existing, (pid) => this.isProcessAlive(pid))) {
          throw operationInProgressError(existing.owner);
        }
        await this.recoverOrphanedAuthority(operationDir, existing);
      }
      throw new Error("Unreachable operation authority state.");
    } finally {
      if (!published) {
        await this.remove(candidateDir);
      }
    }
  }

  private async recoverOrphanedAuthority(
    operationDir: string,
    observed: OwnerState<OperationOwner>
  ): Promise<void> {
    if (observed.kind === "missing" && !(await pathExists(operationDir))) {
      return;
    }

    const staleDir = `${operationDir}.stale.${randomUUID()}`;
    try {
      await this.rename(operationDir, staleDir);
    } catch (error) {
      if (isErrorCode(error, "ENOENT")) {
        return;
      }
      throw error;
    }

    const moved = await readOperationOwner(join(staleDir, "owner.json"));
    if (hasLiveOwner(moved, (pid) => this.isProcessAlive(pid))) {
      await this.restoreMovedDirectory(staleDir, operationDir);
      throw operationInProgressError(moved.owner);
    }
    await this.remove(staleDir);
  }

  private async assertOperationAuthority(
    authority: OperationAuthority
  ): Promise<void> {
    const current = await readOperationOwner(
      join(authority.directory, "owner.json")
    );
    if (
      current.kind !== "valid" ||
      current.owner.nonce !== authority.owner.nonce ||
      current.owner.targetIdentity !== authority.owner.targetIdentity
    ) {
      throw new Error("Workspace operation authority was lost.");
    }
  }

  private async releaseOperationAuthority(
    authority: OperationAuthority
  ): Promise<void> {
    const current = await readOperationOwner(
      join(authority.directory, "owner.json")
    );
    if (
      current.kind !== "valid" ||
      current.owner.nonce !== authority.owner.nonce
    ) {
      return;
    }

    const releasedDir = `${authority.directory}.released.${authority.owner.nonce}`;
    try {
      await this.rename(authority.directory, releasedDir);
    } catch (error) {
      if (isErrorCode(error, "ENOENT")) {
        return;
      }
      const revalidated = await readOperationOwner(
        join(authority.directory, "owner.json")
      );
      if (
        revalidated.kind === "valid" &&
        revalidated.owner.nonce === authority.owner.nonce
      ) {
        try {
          await this.remove(authority.directory);
          return;
        } catch (cleanupError) {
          throw new AggregateError(
            [error, cleanupError],
            "Workspace operation authority cleanup failed."
          );
        }
      }
      throw error;
    }

    const moved = await readOperationOwner(join(releasedDir, "owner.json"));
    if (
      moved.kind === "valid" &&
      moved.owner.nonce === authority.owner.nonce
    ) {
      await this.remove(releasedDir);
      return;
    }
    await this.restoreMovedDirectory(releasedDir, authority.directory);
  }

  private async restoreMovedDirectory(
    movedDirectory: string,
    canonicalDirectory: string
  ): Promise<boolean> {
    try {
      await this.rename(movedDirectory, canonicalDirectory);
      return true;
    } catch (error) {
      if (await pathExists(canonicalDirectory)) {
        return false;
      }
      throw error;
    }
  }

  private async isCanonicalOwner(nonce: string): Promise<boolean> {
    const owner = await readWorkspaceOwner(join(this.lockDir(), "owner.json"));
    return owner.kind === "valid" && owner.owner.nonce === nonce;
  }

  private async tryPublish(
    preparedDirectory: string,
    canonicalDirectory: string
  ): Promise<boolean> {
    try {
      await this.rename(preparedDirectory, canonicalDirectory);
      return true;
    } catch (error) {
      if (await pathExists(canonicalDirectory)) {
        return false;
      }
      if (isErrorCode(error, "EEXIST") || isErrorCode(error, "ENOTEMPTY")) {
        return false;
      }
      throw error;
    }
  }

  private async rename(from: string, to: string): Promise<void> {
    const operation = this.options.fileOperations?.rename;
    if (operation !== undefined) {
      await operation(from, to);
      return;
    }
    await fsRename(from, to);
  }

  private async remove(path: string): Promise<void> {
    const operation = this.options.fileOperations?.remove;
    if (operation !== undefined) {
      await operation(path);
      return;
    }
    await fsRemove(path, {recursive: true, force: true});
  }

  private async prepareOwnerDirectory(
    directory: string,
    owner: WorkspaceOwner | OperationOwner
  ): Promise<void> {
    await mkdir(directory);
    try {
      await this.writeOwner(join(directory, "owner.json"), owner);
    } catch (error) {
      try {
        await this.remove(directory);
      } catch (cleanupError) {
        throw new AggregateError(
          [error, cleanupError],
          `Owner candidate cleanup failed for ${directory}.`
        );
      }
      throw error;
    }
  }

  private async writeOwner(path: string, owner: unknown): Promise<void> {
    const operation = this.options.fileOperations?.writeOwner;
    if (operation !== undefined) {
      await operation(path, owner);
      return;
    }
    await writeJsonFile(path, owner, {backup: false});
  }
}

async function readWorkspaceOwner(
  ownerPath: string
): Promise<OwnerState<WorkspaceOwner>> {
  return readOwner(ownerPath, isWorkspaceOwner);
}

async function readOperationOwner(
  ownerPath: string
): Promise<OwnerState<OperationOwner>> {
  return readOwner(ownerPath, isOperationOwner);
}

async function readOwner<T>(
  ownerPath: string,
  validate: (value: unknown) => value is T
): Promise<OwnerState<T>> {
  let raw: string;
  try {
    raw = await readFile(ownerPath, "utf8");
  } catch (error) {
    if (isErrorCode(error, "ENOENT")) {
      return {kind: "missing"};
    }
    throw error;
  }

  try {
    const parsed = JSON.parse(raw) as unknown;
    return validate(parsed)
      ? {kind: "valid", owner: parsed}
      : {kind: "invalid", fingerprint: fingerprint(raw)};
  } catch (error) {
    if (error instanceof SyntaxError) {
      return {kind: "invalid", fingerprint: fingerprint(raw)};
    }
    throw error;
  }
}

function isWorkspaceOwner(value: unknown): value is WorkspaceOwner {
  if (typeof value !== "object" || value === null) {
    return false;
  }
  const owner = value as Partial<WorkspaceOwner>;
  return (
    Number.isInteger(owner.pid) &&
    (owner.pid ?? 0) > 0 &&
    typeof owner.startedAt === "string" &&
    owner.startedAt.length > 0 &&
    typeof owner.workspace === "string" &&
    owner.workspace.length > 0 &&
    typeof owner.nonce === "string" &&
    owner.nonce.length > 0
  );
}

function isOperationOwner(value: unknown): value is OperationOwner {
  return (
    isWorkspaceOwner(value) &&
    (value as Partial<OperationOwner>).kind === "workspace_operation" &&
    typeof (value as Partial<OperationOwner>).targetIdentity === "string" &&
    ((value as Partial<OperationOwner>).targetIdentity?.length ?? 0) > 0
  );
}

function sameOwnerState<T extends WorkspaceOwner>(
  first: OwnerState<T>,
  second: OwnerState<T>
): boolean {
  if (first.kind !== second.kind) {
    return false;
  }
  if (first.kind === "valid" && second.kind === "valid") {
    return first.owner.nonce === second.owner.nonce;
  }
  if (first.kind === "invalid" && second.kind === "invalid") {
    return first.fingerprint === second.fingerprint;
  }
  return first.kind === "missing" && second.kind === "missing";
}

function ownerStateIdentity(owner: OwnerState<WorkspaceOwner>): string {
  if (owner.kind === "valid") {
    return `owner:${owner.owner.nonce}`;
  }
  if (owner.kind === "invalid") {
    return `invalid:${owner.fingerprint}`;
  }
  return "missing";
}

function hasLiveOwner<T extends {pid: number}>(
  owner: OwnerState<T>,
  isAlive: (pid: number) => boolean
): owner is {kind: "valid"; owner: T} {
  return owner.kind === "valid" && isAlive(owner.owner.pid);
}

function isProcessAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return !isErrorCode(error, "ESRCH");
  }
}

function workspaceOpenError(owner: WorkspaceOwner): Error {
  return new Error(
    `This workspace is already open by PID ${owner.pid}: ${owner.workspace}`
  );
}

function operationInProgressError(owner: OperationOwner): Error {
  return new Error(
    `This workspace is already open or changing ownership by PID ${owner.pid}: ${owner.workspace}`
  );
}

function fingerprint(raw: string): string {
  return createHash("sha256").update(raw).digest("hex");
}

async function pathExists(path: string): Promise<boolean> {
  try {
    await access(path);
    return true;
  } catch (error) {
    if (isErrorCode(error, "ENOENT")) {
      return false;
    }
    throw error;
  }
}

function isErrorCode(error: unknown, code: string): boolean {
  return (error as NodeJS.ErrnoException).code === code;
}
