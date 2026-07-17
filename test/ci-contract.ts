interface WorkflowLine {
  readonly indent: number;
  readonly lineNumber: number;
  readonly text: string;
}

interface MappingEntry {
  readonly index: number;
  readonly key: string;
  readonly value: string;
}

interface WorkflowStep {
  readonly entries: ReadonlyMap<string, MappingEntry>;
}

export interface CiValidationResult {
  readonly valid: boolean;
  readonly errors: string[];
}

const REQUIRED_RUN_COMMANDS = [
  "rustup toolchain install 1.97.0 --profile minimal --component rustfmt --component clippy",
  "npm ci",
  "npm run check",
  "npm test",
  "npm run check:rust",
  "npm run test:rust",
  "npm run verify:rust-contracts",
  "npm run build",
  "npm run eval:retrieval",
  "npm run eval:provider",
  "npm run build:rust:release",
  "npm run package:rust",
  "npm run verify:rust-release",
  "npm run verify:milestone-flow"
] as const;

export function validateCiWorkflow(source: string): CiValidationResult {
  const errors: string[] = [];
  let lines: WorkflowLine[];
  try {
    lines = parseActiveLines(source);
  } catch (error) {
    return {
      valid: false,
      errors: [error instanceof Error ? error.message : "Workflow parsing failed."]
    };
  }

  validateForbiddenActiveContent(lines, errors);
  validateTopLevelContract(lines, errors);
  return {valid: errors.length === 0, errors};
}

function validateTopLevelContract(lines: WorkflowLine[], errors: string[]): void {
  const roots = directEntries(lines, -1, lines.length, -1, errors, "top level");
  validateExactKeys(roots, ["name", "on", "permissions", "jobs"], errors, "Top level");
  const name = uniqueEntry(roots, "name", errors, "top-level name");
  if (!name || scalar(name.value) !== "CI") {
    errors.push("Top-level name must be exactly CI.");
  }

  const triggers = uniqueEntry(roots, "on", errors, "top-level on");
  if (triggers) {
    validateTriggers(lines, triggers, errors);
  }

  const permissions = uniqueEntry(
    roots,
    "permissions",
    errors,
    "top-level permissions"
  );
  if (permissions) {
    validatePermissions(lines, permissions, errors);
  }

  const jobs = uniqueEntry(roots, "jobs", errors, "top-level jobs");
  if (!jobs) {
    return;
  }
  if (jobs.value) {
    errors.push("Top-level jobs must be a mapping block.");
    return;
  }
  const jobEntries = childEntries(lines, jobs.index, errors, "jobs");
  validateExactKeys(jobEntries, ["verify"], errors, "jobs");
  const verify = uniqueEntry(jobEntries, "verify", errors, "jobs.verify");
  if (verify) {
    validateVerifyJob(lines, verify, errors);
  }
}

function validateTriggers(
  lines: WorkflowLine[],
  header: MappingEntry,
  errors: string[]
): void {
  if (header.value) {
    errors.push("Top-level on must be a block containing push and pull_request.");
    return;
  }
  const entries = childEntries(lines, header.index, errors, "top-level on");
  const actual = entries.map(({key}) => key).sort();
  if (
    entries.some(({value}) => value !== "") ||
    actual.length !== 2 ||
    actual[0] !== "pull_request" ||
    actual[1] !== "push"
  ) {
    errors.push("Top-level on must contain exactly push and pull_request.");
  }
}

function validatePermissions(
  lines: WorkflowLine[],
  header: MappingEntry,
  errors: string[]
): void {
  if (header.value) {
    errors.push("Top-level permissions must be a block with only contents: read.");
    return;
  }
  const entries = childEntries(lines, header.index, errors, "top-level permissions");
  const end = blockEnd(lines, header.index);
  if (
    entries.length !== 1 ||
    entries[0]?.key !== "contents" ||
    scalar(entries[0].value) !== "read" ||
    end - header.index !== 2
  ) {
    errors.push("Top-level permissions must contain exactly contents: read.");
  }
}

function validateVerifyJob(
  lines: WorkflowLine[],
  verify: MappingEntry,
  errors: string[]
): void {
  if (verify.value) {
    errors.push("jobs.verify must be a mapping block.");
    return;
  }
  const entries = childEntries(lines, verify.index, errors, "jobs.verify");
  validateExactKeys(
    entries,
    ["strategy", "runs-on", "timeout-minutes", "steps"],
    errors,
    "jobs.verify"
  );
  const runsOn = uniqueEntry(entries, "runs-on", errors, "jobs.verify runs-on");
  if (!runsOn || scalar(runsOn.value) !== "${{ matrix.os }}") {
    errors.push("jobs.verify runs-on must be exactly ${{ matrix.os }}.");
  }

  const timeout = uniqueEntry(
    entries,
    "timeout-minutes",
    errors,
    "jobs.verify timeout"
  );
  if (!timeout || scalar(timeout.value) !== "15") {
    errors.push("jobs.verify timeout-minutes must be exactly 15.");
  }

  const strategy = uniqueEntry(entries, "strategy", errors, "jobs.verify strategy");
  if (strategy) {
    validateMatrix(lines, strategy, errors);
  }

  const steps = uniqueEntry(entries, "steps", errors, "jobs.verify steps");
  if (steps) {
    validateSteps(lines, steps, errors);
  }
}

function validateMatrix(
  lines: WorkflowLine[],
  strategy: MappingEntry,
  errors: string[]
): void {
  if (strategy.value) {
    errors.push("jobs.verify strategy must be a mapping block.");
    return;
  }
  const strategyEntries = childEntries(lines, strategy.index, errors, "jobs.verify strategy");
  validateExactKeys(
    strategyEntries,
    ["fail-fast", "matrix"],
    errors,
    "jobs.verify strategy"
  );
  const failFast = uniqueEntry(
    strategyEntries,
    "fail-fast",
    errors,
    "jobs.verify strategy.fail-fast"
  );
  if (!failFast || scalar(failFast.value) !== "false") {
    errors.push("jobs.verify strategy.fail-fast must be exactly false.");
  }
  const matrix = uniqueEntry(
    strategyEntries,
    "matrix",
    errors,
    "jobs.verify strategy.matrix"
  );
  if (!matrix) {
    return;
  }
  if (matrix.value) {
    errors.push("jobs.verify strategy.matrix must be a mapping block.");
    return;
  }
  const matrixEntries = childEntries(lines, matrix.index, errors, "jobs.verify matrix");
  validateExactKeys(matrixEntries, ["os"], errors, "jobs.verify matrix");
  const os = uniqueEntry(matrixEntries, "os", errors, "jobs.verify matrix.os");
  if (!os) {
    return;
  }
  const values = sequenceValue(lines, os, errors).sort();
  if (
    values.length !== 2 ||
    values[0] !== "ubuntu-latest" ||
    values[1] !== "windows-latest"
  ) {
    errors.push("jobs.verify matrix.os must contain exactly ubuntu-latest and windows-latest.");
  }
}

function validateSteps(
  lines: WorkflowLine[],
  stepsHeader: MappingEntry,
  errors: string[]
): void {
  const steps = parseSteps(lines, stepsHeader, errors);
  if (steps.length !== 19) {
    errors.push("jobs.verify steps must contain exactly nineteen allowlisted steps.");
    return;
  }

  validateStep(
    steps[0]!,
    ["uses"],
    "uses",
    "actions/checkout@v4",
    errors,
    1
  );
  validateStep(
    steps[1]!,
    ["name", "if", "run"],
    "run",
    "sudo apt-get update && sudo apt-get install -y bubblewrap",
    errors,
    2
  );
  validateLinuxSandboxStep(
    steps[1]!,
    "Install Linux subprocess sandbox",
    errors,
    2
  );
  validateStep(
    steps[2]!,
    ["name", "if", "run"],
    "run",
    "bwrap --version",
    errors,
    3
  );
  validateLinuxSandboxStep(
    steps[2]!,
    "Verify Linux subprocess sandbox",
    errors,
    3
  );
  validateStep(
    steps[3]!,
    ["uses", "with"],
    "uses",
    "actions/setup-node@v4",
    errors,
    4
  );
  validateSetupNode(lines, steps[3]!, errors);
  validateStep(
    steps[4]!,
    ["run"],
    "run",
    REQUIRED_RUN_COMMANDS[0],
    errors,
    5
  );
  validateStep(
    steps[5]!,
    ["name", "if", "run"],
    "run",
    "cargo test -p minimax-tools --test sandbox_adversarial --locked",
    errors,
    6
  );
  validateLinuxSandboxStep(
    steps[5]!,
    "Run Linux adversarial sandbox canary",
    errors,
    6
  );
  for (let index = 1; index < REQUIRED_RUN_COMMANDS.length; index += 1) {
    validateStep(
      steps[index + 5]!,
      ["run"],
      "run",
      REQUIRED_RUN_COMMANDS[index]!,
      errors,
      index + 6
    );
  }
}

function validateLinuxSandboxStep(
  step: WorkflowStep,
  expectedName: string,
  errors: string[],
  position: number
): void {
  if (
    scalar(step.entries.get("name")?.value ?? "") !== expectedName ||
    scalar(step.entries.get("if")?.value ?? "") !== "runner.os == 'Linux'"
  ) {
    errors.push(
      `jobs.verify Linux sandbox step ${position} must have its exact name and Linux-only condition.`
    );
  }
}

function validateStep(
  step: WorkflowStep,
  keys: readonly string[],
  valueKey: "uses" | "run",
  expectedValue: string,
  errors: string[],
  position: number
): void {
  validateExactKeys(
    [...step.entries.values()],
    keys,
    errors,
    `jobs.verify step ${position}`
  );
  if (scalar(step.entries.get(valueKey)?.value ?? "") !== expectedValue) {
    errors.push(
      `jobs.verify step order is fixed; step ${position} must be ${valueKey}: ${expectedValue}.`
    );
  }
}

function validateSetupNode(
  lines: WorkflowLine[],
  step: WorkflowStep,
  errors: string[]
): void {
  const withEntry = step.entries.get("with");
  if (!withEntry || withEntry.value) {
    errors.push("actions/setup-node@v4 must configure node-version 20 and cache npm.");
    return;
  }
  const entries = childEntries(lines, withEntry.index, errors, "setup-node with");
  const nodeVersion = uniqueEntry(entries, "node-version", errors, "setup-node node-version");
  const cache = uniqueEntry(entries, "cache", errors, "setup-node cache");
  if (
    entries.length !== 2 ||
    scalar(nodeVersion?.value ?? "") !== "20" ||
    scalar(cache?.value ?? "") !== "npm"
  ) {
    errors.push("actions/setup-node@v4 must configure exactly node-version 20 and cache npm.");
  }
}

function parseSteps(
  lines: WorkflowLine[],
  header: MappingEntry,
  errors: string[]
): WorkflowStep[] {
  if (header.value) {
    errors.push("jobs.verify steps must be a sequence block.");
    return [];
  }
  const end = blockEnd(lines, header.index);
  const indent = directIndent(lines, header.index + 1, end);
  if (indent === undefined) {
    errors.push("jobs.verify steps must not be empty.");
    return [];
  }
  const starts = lines
    .map((line, index) => ({line, index}))
    .filter(({line, index}) => index > header.index && index < end && line.indent === indent);
  if (starts.some(({line}) => !/^\-\s+/.test(line.text))) {
    errors.push("jobs.verify steps must contain only sequence items.");
    return [];
  }

  return starts.map(({line, index}, position) => {
    const stepEnd = starts[position + 1]?.index ?? end;
    const entries: MappingEntry[] = [];
    const initial = parseMapping(line.text.replace(/^\-\s+/, ""), index);
    if (initial) {
      entries.push(initial);
    } else {
      errors.push(`Invalid step mapping on workflow line ${line.lineNumber}.`);
    }
    const childIndent = directIndent(lines, index + 1, stepEnd);
    if (childIndent !== undefined) {
      for (let cursor = index + 1; cursor < stepEnd; cursor += 1) {
        if (lines[cursor]!.indent !== childIndent) {
          continue;
        }
        const entry = parseMapping(lines[cursor]!.text, cursor);
        if (entry) {
          entries.push(entry);
        } else {
          errors.push(`Invalid step property on workflow line ${lines[cursor]!.lineNumber}.`);
        }
      }
    }
    return {entries: entryMap(entries, errors, "jobs.verify step")};
  });
}

function sequenceValue(
  lines: WorkflowLine[],
  entry: MappingEntry,
  errors: string[]
): string[] {
  const value = entry.value.trim();
  if (value.startsWith("[") && value.endsWith("]")) {
    const inner = value.slice(1, -1).trim();
    return inner ? inner.split(",").map((part) => scalar(part)) : [];
  }
  if (value) {
    errors.push(`${entry.key} must be a YAML sequence.`);
    return [];
  }
  const end = blockEnd(lines, entry.index);
  const indent = directIndent(lines, entry.index + 1, end);
  if (indent === undefined) {
    return [];
  }
  const values: string[] = [];
  for (let cursor = entry.index + 1; cursor < end; cursor += 1) {
    const line = lines[cursor]!;
    if (line.indent !== indent || !/^\-\s+/.test(line.text)) {
      errors.push(`${entry.key} must contain only scalar sequence items.`);
      continue;
    }
    values.push(scalar(line.text.replace(/^\-\s+/, "")));
  }
  return values;
}

function validateForbiddenActiveContent(lines: WorkflowLine[], errors: string[]): void {
  const active = lines.map(({text}) => text).join("\n");
  if (/smoke:provider|provider-smoke|(?:^|[\\/])src[\\/]smoke/i.test(active)) {
    errors.push("Workflow active content contains a forbidden live-provider path or command.");
  }
  if (/\bsecrets\s*(?:\.|\[)/i.test(active)) {
    errors.push("Workflow active content must not reference GitHub secrets.");
  }
  if (
    /\b[A-Z][A-Z0-9_]*(?:API_KEY|ACCESS_KEY|SECRET|TOKEN)\b/.test(active) ||
    /\bAWS_(?:ACCESS_KEY_ID|SECRET_ACCESS_KEY|SESSION_TOKEN)\b/.test(active)
  ) {
    errors.push("Workflow active content must not contain Provider credential names.");
  }
  if (
    lines.some((line, index) =>
      parseMapping(line.text.replace(/^\-\s+/, ""), index)?.key === "env"
    )
  ) {
    errors.push("Workflow environment injection is not allowed.");
  }
  const permissionHeaders = lines.filter((line, index) => {
    const entry = parseMapping(line.text.replace(/^\-\s+/, ""), index);
    return entry?.key === "permissions";
  });
  if (permissionHeaders.length !== 1 || permissionHeaders[0]?.indent !== 0) {
    errors.push("Only top-level permissions may exist in the workflow.");
  }
}

function parseActiveLines(source: string): WorkflowLine[] {
  return source.split(/\r?\n/).flatMap((raw, index) => {
    if (/^\s*\t/.test(raw)) {
      throw new Error(`Workflow line ${index + 1} uses tab indentation.`);
    }
    const active = stripComment(raw).trimEnd();
    if (!active.trim()) {
      return [];
    }
    const indent = active.length - active.trimStart().length;
    return [{indent, lineNumber: index + 1, text: active.slice(indent)}];
  });
}

function stripComment(line: string): string {
  let singleQuoted = false;
  let doubleQuoted = false;
  for (let index = 0; index < line.length; index += 1) {
    const character = line[index]!;
    if (character === "'" && !doubleQuoted) {
      if (singleQuoted && line[index + 1] === "'") {
        index += 1;
        continue;
      }
      singleQuoted = !singleQuoted;
      continue;
    }
    if (character === '"' && !singleQuoted && line[index - 1] !== "\\") {
      doubleQuoted = !doubleQuoted;
      continue;
    }
    if (
      character === "#" &&
      !singleQuoted &&
      !doubleQuoted &&
      (index === 0 || /\s/.test(line[index - 1]!))
    ) {
      return line.slice(0, index);
    }
  }
  return line;
}

function directEntries(
  lines: WorkflowLine[],
  start: number,
  end: number,
  parentIndent: number,
  errors: string[],
  label: string
): MappingEntry[] {
  const indent = directIndent(lines, start + 1, end, parentIndent);
  if (indent === undefined) {
    errors.push(`Missing ${label} mapping entries.`);
    return [];
  }
  const entries: MappingEntry[] = [];
  for (let index = start + 1; index < end; index += 1) {
    const line = lines[index]!;
    if (line.indent !== indent) {
      continue;
    }
    const entry = parseMapping(line.text, index);
    if (entry) {
      entries.push(entry);
    } else {
      errors.push(`Invalid ${label} mapping on workflow line ${line.lineNumber}.`);
    }
  }
  return entries;
}

function childEntries(
  lines: WorkflowLine[],
  headerIndex: number,
  errors: string[],
  label: string
): MappingEntry[] {
  const header = lines[headerIndex]!;
  return directEntries(
    lines,
    headerIndex,
    blockEnd(lines, headerIndex),
    header.indent,
    errors,
    label
  );
}

function directIndent(
  lines: WorkflowLine[],
  start: number,
  end: number,
  parentIndent = -1
): number | undefined {
  let indent: number | undefined;
  for (let index = start; index < end; index += 1) {
    const candidate = lines[index]!.indent;
    if (candidate <= parentIndent) {
      continue;
    }
    indent = indent === undefined ? candidate : Math.min(indent, candidate);
  }
  return indent;
}

function blockEnd(lines: WorkflowLine[], headerIndex: number): number {
  const indent = lines[headerIndex]!.indent;
  let index = headerIndex + 1;
  while (index < lines.length && lines[index]!.indent > indent) {
    index += 1;
  }
  return index;
}

function parseMapping(text: string, index: number): MappingEntry | undefined {
  const match = /^([^:\s][^:]*?)\s*:\s*(.*)$/.exec(text);
  if (!match) {
    return undefined;
  }
  return {index, key: scalar(match[1]!.trim()), value: match[2]!.trim()};
}

function uniqueEntry(
  entries: readonly MappingEntry[],
  key: string,
  errors: string[],
  label: string
): MappingEntry | undefined {
  const matches = entries.filter((entry) => entry.key === key);
  if (matches.length !== 1) {
    errors.push(`${label} must exist exactly once.`);
    return undefined;
  }
  return matches[0];
}

function entryMap(
  entries: readonly MappingEntry[],
  errors: string[],
  label: string
): ReadonlyMap<string, MappingEntry> {
  const result = new Map<string, MappingEntry>();
  for (const entry of entries) {
    if (result.has(entry.key)) {
      errors.push(`${label} contains duplicate ${entry.key}.`);
    }
    result.set(entry.key, entry);
  }
  return result;
}

function validateExactKeys(
  entries: readonly MappingEntry[],
  expectedKeys: readonly string[],
  errors: string[],
  label: string
): void {
  const actual = entries.map(({key}) => key).sort();
  const expected = [...expectedKeys].sort();
  if (
    actual.length !== expected.length ||
    actual.some((key, index) => key !== expected[index])
  ) {
    errors.push(`${label} keys must be exactly: ${expectedKeys.join(", ")}.`);
  }
}

function scalar(value: string): string {
  const trimmed = value.trim();
  if (
    trimmed.length >= 2 &&
    ((trimmed.startsWith('"') && trimmed.endsWith('"')) ||
      (trimmed.startsWith("'") && trimmed.endsWith("'")))
  ) {
    return trimmed.slice(1, -1);
  }
  return trimmed;
}
