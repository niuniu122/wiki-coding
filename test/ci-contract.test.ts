import assert from "node:assert/strict";
import {readFile} from "node:fs/promises";
import {resolve} from "node:path";
import test from "node:test";
import {validateCiWorkflow} from "./ci-contract.js";

const VALID_WORKFLOW = `name: CI

on:
  push:
  pull_request:

permissions:
  contents: read

jobs:
  verify:
    runs-on: \${{ matrix.os }}
    timeout-minutes: 15
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, windows-latest]
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 20
          cache: npm
      - run: npm ci
      - run: npm run check
      - run: npm test
      - run: npm run build
      - run: npm run eval:retrieval
      - run: npm run eval:provider
`;

test("the committed workflow satisfies the structural offline CI contract", async () => {
  const workflow = await readFile(resolve(".github/workflows/ci.yml"), "utf8");
  assert.deepEqual(validateCiWorkflow(workflow), {valid: true, errors: []});
});

test("the validator accepts harmless spacing and comments", () => {
  const workflow = VALID_WORKFLOW
    .replace("permissions:", "permissions: # top-level only")
    .replace("      - run: npm test", "      # smoke:provider is documentation only\n      -   run:   npm test");

  assert.deepEqual(validateCiWorkflow(workflow), {valid: true, errors: []});
});

test("top-level permissions reject extra write authority", () => {
  const workflow = VALID_WORKFLOW.replace(
    "  contents: read",
    "  contents: read\n  pull-requests: write"
  );
  assertInvalid(workflow, /top-level permissions/i);
});

test("job-local permissions cannot substitute for top-level permissions", () => {
  const workflow = VALID_WORKFLOW
    .replace("permissions:\n  contents: read\n\n", "")
    .replace("  verify:\n", "  verify:\n    permissions:\n      contents: read\n");
  assertInvalid(workflow, /top-level permissions/i);
});

test("job-local permissions cannot override the read-only top-level grant", () => {
  const workflow = VALID_WORKFLOW.replace(
    "  verify:\n",
    "  verify:\n    permissions:\n      contents: write\n"
  );
  assertInvalid(workflow, /only top-level permissions/i);
});

test("required commands cannot be supplied only by comments", () => {
  const workflow = VALID_WORKFLOW.replace(
    "      - run: npm test",
    "      # run: npm test"
  );
  assertInvalid(workflow, /steps|run commands/i);
});

test("required commands in another job do not count for jobs.verify", () => {
  const workflow = VALID_WORKFLOW.replace("      - run: npm test\n", "") + `
  decoy:
    runs-on: ubuntu-latest
    steps:
      - run: npm test
`;
  assertInvalid(workflow, /steps|run commands/i);
});

test("direct smoke paths are rejected even without the package script name", () => {
  const workflow = VALID_WORKFLOW.replace(
    "      - run: npm run build",
    "      - run: npm run build\n      - run: npx tsx src/smoke/provider-smoke.ts"
  );
  assertInvalid(workflow, /live-provider path/i);
});

test("active workflow environment credential injection is rejected", () => {
  const workflow = VALID_WORKFLOW.replace(
    "permissions:",
    "env:\n  MINIMAX_API_KEY: \${{ secrets.MINIMAX_API_KEY }}\n\npermissions:"
  );
  assertInvalid(workflow, /environment|credential|secrets/i);
});

for (const [name, mutate] of [
  [
    "job if cannot disable verification",
    (workflow: string) => workflow.replace("  verify:\n", "  verify:\n    if: false\n")
  ],
  [
    "job continue-on-error cannot forgive verification failure",
    (workflow: string) =>
      workflow.replace("  verify:\n", "  verify:\n    continue-on-error: true\n")
  ],
  [
    "job needs cannot alter the isolated verification graph",
    (workflow: string) => workflow.replace("  verify:\n", "  verify:\n    needs: prepare\n")
  ],
  [
    "checkout step if cannot skip repository setup",
    (workflow: string) =>
      workflow.replace(
        "      - uses: actions/checkout@v4",
        "      - uses: actions/checkout@v4\n        if: false"
      )
  ],
  [
    "run step continue-on-error cannot hide a failed gate",
    (workflow: string) =>
      workflow.replace("      - run: npm test", "      - run: npm test\n        continue-on-error: true")
  ],
  [
    "run step shell cannot reinterpret an exact command",
    (workflow: string) =>
      workflow.replace("      - run: npm test", "      - run: npm test\n        shell: bash")
  ],
  [
    "run step working-directory cannot move a gate out of the repository root",
    (workflow: string) =>
      workflow.replace(
        "      - run: npm test",
        "      - run: npm test\n        working-directory: fixtures"
      )
  ],
  [
    "an extra step cannot extend the offline contract",
    (workflow: string) =>
      workflow.replace(
        "      - run: npm run build",
        "      - run: npm run build\n      - uses: actions/cache@v4"
      )
  ]
] as const) {
  test(name, () => {
    assertInvalid(mutate(VALID_WORKFLOW), /jobs\.verify|step|unsupported/i);
  });
}

test("setup-node must remain before every offline run command", () => {
  const setup = `      - uses: actions/setup-node@v4
        with:
          node-version: 20
          cache: npm
`;
  const workflow = VALID_WORKFLOW
    .replace(setup, "")
    .replace("      - run: npm run build\n", `      - run: npm run build\n${setup}`);

  assertInvalid(workflow, /step order|jobs\.verify steps/i);
});

test("jobs contains only the verification job", () => {
  const workflow = VALID_WORKFLOW + `
  decoy:
    runs-on: ubuntu-latest
    steps:
      - run: npm test
`;
  assertInvalid(workflow, /jobs (?:keys must be exactly|must contain exactly)/i);
});

test("offline evaluation scripts are exact and never alias the live smoke command", async () => {
  const packageJson = JSON.parse(await readFile(resolve("package.json"), "utf8")) as {scripts?: Record<string, string>};
  assert.equal(packageJson.scripts?.["eval:retrieval"], "tsx src/eval/capability-retrieval-report.ts");
  assert.equal(packageJson.scripts?.["eval:provider"], "tsx src/eval/provider-conformance.ts");
  assert.doesNotMatch(`${packageJson.scripts?.["eval:retrieval"]} ${packageJson.scripts?.["eval:provider"]}`, /smoke|download|provider-smoke/i);
});

function assertInvalid(workflow: string, expected: RegExp): void {
  const result = validateCiWorkflow(workflow);
  assert.equal(result.valid, false);
  assert.match(result.errors.join("\n"), expected);
}
