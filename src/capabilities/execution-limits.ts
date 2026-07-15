export interface CapabilityExecutionLimits {
  readonly timeoutMs: number;
  readonly maxOutputBytes: number;
  readonly maxDirectoryEntries: number;
}

export const DEFAULT_CAPABILITY_EXECUTION_LIMITS: CapabilityExecutionLimits = Object.freeze({
  timeoutMs: 30_000,
  maxOutputBytes: 64 * 1024,
  maxDirectoryEntries: 500
});

export function validateExecutionLimits(limits: CapabilityExecutionLimits): CapabilityExecutionLimits {
  if (!Number.isSafeInteger(limits.timeoutMs) || limits.timeoutMs < 10 || limits.timeoutMs > 300_000) {
    throw new Error("Invalid capability execution timeout.");
  }
  if (!Number.isSafeInteger(limits.maxOutputBytes) || limits.maxOutputBytes < 256 || limits.maxOutputBytes > 4 * 1024 * 1024) {
    throw new Error("Invalid capability output limit.");
  }
  if (!Number.isSafeInteger(limits.maxDirectoryEntries) || limits.maxDirectoryEntries < 1 || limits.maxDirectoryEntries > 10_000) {
    throw new Error("Invalid capability directory-entry limit.");
  }
  return Object.freeze({...limits});
}
