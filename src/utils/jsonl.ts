import {randomUUID} from "node:crypto";
import {appendFile, mkdir, open, readFile, rename, rm} from "node:fs/promises";
import {basename, dirname, join} from "node:path";

export interface ReadJsonFileOptions<T> {
  validate?: (value: unknown) => value is T;
  parse?: (value: unknown) => T;
}

export interface WriteJsonFileOptions {
  mode?: number;
  backup?: boolean;
}

export async function ensureParentDir(filePath: string): Promise<void> {
  await mkdir(dirname(filePath), {recursive: true});
}

export async function writeJsonFile<T>(
  filePath: string,
  value: T,
  options: WriteJsonFileOptions = {}
): Promise<void> {
  const rendered = `${JSON.stringify(value, null, 2)}\n`;
  const mode = options.mode ?? 0o600;
  if (options.backup !== false) {
    const current = await readTextFile(filePath, null);
    if (current !== null && isValidJson(current)) {
      await atomicReplace(`${filePath}.bak`, current, mode);
    }
  }
  await atomicReplace(filePath, rendered, mode);
}

export async function readJsonFile<T>(
  filePath: string,
  fallback: T,
  options: ReadJsonFileOptions<T> = {}
): Promise<T> {
  const primary = await tryReadJson(filePath, options);
  if (primary.ok) {
    return primary.value;
  }

  const backupPath = `${filePath}.bak`;
  const backup = await tryReadJson(backupPath, options);
  if (backup.ok) {
    await atomicReplace(filePath, backup.raw, 0o600);
    return backup.value;
  }

  if (primary.missing && backup.missing) {
    return fallback;
  }

  throw new Error(
    `No valid primary or backup JSON file for ${filePath}. ` +
      `Primary: ${describeFailure(primary)}. Backup: ${describeFailure(backup)}.`
  );
}

export async function appendJsonl(filePath: string, value: unknown): Promise<void> {
  await ensureParentDir(filePath);
  await appendFile(filePath, `${JSON.stringify(value)}\n`, {
    encoding: "utf8",
    mode: 0o600
  });
}

export async function readJsonl<T>(filePath: string): Promise<T[]> {
  const raw = await readTextFile(filePath, "");
  if (!raw) {
    return [];
  }

  const lines = raw.split("\n");
  const nonEmptyIndexes = lines
    .map((line, index) => ({line, index}))
    .filter(({line}) => line.trim().length > 0)
    .map(({index}) => index);
  const lastRecordIndex = nonEmptyIndexes.at(-1) ?? -1;
  const values: T[] = [];
  const validLines: string[] = [];

  for (const index of nonEmptyIndexes) {
    const line = lines[index]?.trim() ?? "";
    try {
      values.push(JSON.parse(line) as T);
      validLines.push(line);
    } catch (error) {
      const isInterruptedTail = index === lastRecordIndex && !raw.endsWith("\n");
      if (!isInterruptedTail) {
        throw new Error(`JSONL corruption in ${filePath} at line ${index + 1}.`, {
          cause: error
        });
      }

      const repaired = validLines.length > 0 ? `${validLines.join("\n")}\n` : "";
      await atomicReplace(filePath, repaired, 0o600);
      return values;
    }
  }

  return values;
}

type JsonReadResult<T> =
  | {ok: true; value: T; raw: string}
  | {ok: false; missing: boolean; error?: unknown};

async function tryReadJson<T>(
  filePath: string,
  options: ReadJsonFileOptions<T>
): Promise<JsonReadResult<T>> {
  try {
    const raw = await readFile(filePath, "utf8");
    const parsed = JSON.parse(raw) as unknown;
    const value = options.parse
      ? options.parse(parsed)
      : options.validate
        ? options.validate(parsed)
          ? parsed
          : (() => {
              throw new Error("JSON value failed validation.");
            })()
        : (parsed as T);
    return {ok: true, value, raw};
  } catch (error) {
    return {
      ok: false,
      missing: (error as NodeJS.ErrnoException).code === "ENOENT",
      error
    };
  }
}

function describeFailure<T>(result: JsonReadResult<T>): string {
  if (result.ok) {
    return "valid";
  }
  if (result.missing) {
    return "missing";
  }
  return result.error instanceof Error ? result.error.message : String(result.error);
}

async function atomicReplace(filePath: string, content: string, mode: number): Promise<void> {
  await ensureParentDir(filePath);
  const tempPath = join(
    dirname(filePath),
    `.${basename(filePath)}.${process.pid}.${randomUUID()}.tmp`
  );
  const handle = await open(tempPath, "wx", mode);
  try {
    await handle.writeFile(content, "utf8");
    await handle.sync();
  } finally {
    await handle.close();
  }

  try {
    await rename(tempPath, filePath);
  } catch (error) {
    await rm(tempPath, {force: true});
    throw error;
  }
}

function isValidJson(raw: string): boolean {
  try {
    JSON.parse(raw);
    return true;
  } catch {
    return false;
  }
}

async function readTextFile<T extends string | null>(
  filePath: string,
  fallback: T
): Promise<string | T> {
  try {
    return await readFile(filePath, "utf8");
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") {
      return fallback;
    }
    throw error;
  }
}
