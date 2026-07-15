import {realpath, stat} from "node:fs/promises";
import {isAbsolute, relative, resolve, sep} from "node:path";

export class CapabilityPathError extends Error {
  readonly name = "CapabilityPathError";
  constructor(readonly code: "outside_managed_root" | "invalid_path") {
    super(`Capability path rejected (${code}).`);
  }
}

export async function resolveManagedPath(root: string, candidate: string): Promise<string> {
  const rootReal = await realpath(resolve(root));
  const requested = isAbsolute(candidate) ? resolve(candidate) : resolve(rootReal, candidate);
  const candidateReal = await realpath(requested);
  if (!isWithin(rootReal, candidateReal)) {
    throw new CapabilityPathError("outside_managed_root");
  }
  const info = await stat(candidateReal);
  if (!info.isFile() && !info.isDirectory()) {
    throw new CapabilityPathError("invalid_path");
  }
  return candidateReal;
}

export function isWithin(root: string, candidate: string): boolean {
  const path = relative(normalizePath(root), normalizePath(candidate));
  return path === "" || (!path.startsWith(`..${sep}`) && path !== ".." && !isAbsolute(path));
}

export function normalizeCapabilityPath(value: string): string {
  return normalizePath(resolve(value));
}

function normalizePath(value: string): string {
  const normalized = value.normalize("NFC");
  return process.platform === "win32" ? normalized.toLocaleLowerCase("en-US") : normalized;
}
