import assert from "node:assert/strict";
import {readFile} from "node:fs/promises";
import {resolve} from "node:path";
import test from "node:test";
import {validateCiWorkflow} from "./ci-contract.js";

const VALID_WORKFLOW = `name: CI

on:
  push:
  pull_request:
  workflow_dispatch:

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
      - name: Install Linux subprocess sandbox
        if: runner.os == 'Linux'
        run: sudo apt-get update && sudo apt-get install -y bubblewrap && sudo sysctl -w kernel.apparmor_restrict_unprivileged_userns=0
      - name: Verify Linux subprocess sandbox
        if: runner.os == 'Linux'
        run: bwrap --unshare-user --unshare-ipc --unshare-pid --unshare-net --unshare-uts --unshare-cgroup-try --die-with-parent --new-session --cap-drop ALL --clearenv --ro-bind / / /bin/true
      - uses: actions/setup-node@v4
        with:
          node-version: 20
          cache: npm
      - run: rustup toolchain install 1.97.0 --profile minimal --component rustfmt --component clippy
      - name: Run Linux adversarial sandbox canary
        if: runner.os == 'Linux'
        run: bash scripts/ci-linux-sandbox-canary.sh
      - run: npm ci
      - name: Verify strict Rust source authority and contracts
        if: github.event_name != 'workflow_dispatch'
        run: npm run verify:rust-contracts
      - name: Verify hosted evidence candidate Rust source authority and contracts
        if: github.event_name == 'workflow_dispatch'
        run: npm run verify:rust-contracts:candidate
      - name: Run transitional TypeScript static checks
        run: npm run check
      - name: Run transitional TypeScript tests
        run: npm test
      - run: npm run check:rust
      - name: Run strict Rust tests
        if: github.event_name != 'workflow_dispatch'
        run: npm run test:rust
      - name: Run hosted evidence candidate Rust tests
        if: github.event_name == 'workflow_dispatch'
        run: npm run test:rust:candidate
      - name: Run Rust Provider evaluation
        run: npm run eval:provider
      - name: Run Rust retrieval evaluation
        run: npm run eval:retrieval
      - run: npm run build:rust:release
      - run: npm run package:rust
      - run: npm run verify:rust-release
      - name: Upload hosted release evidence candidate
        if: github.event_name == 'workflow_dispatch'
        uses: actions/upload-artifact@v4
        with:
          name: hosted-release-evidence-\${{ runner.os }}
          path: target/release-evidence/*.json
          if-no-files-found: error
          retention-days: 7
      - run: npm run verify:milestone-flow
`;

test("the committed workflow satisfies the structural offline CI contract", async () => {
  const workflow = await readFile(resolve(".github/workflows/ci.yml"), "utf8");
  assert.deepEqual(validateCiWorkflow(workflow), {valid: true, errors: []});
});

test("the validator accepts harmless spacing and comments", () => {
  const workflow = VALID_WORKFLOW
    .replace("permissions:", "permissions: # top-level only")
    .replace("        run: npm test", "      # smoke:provider is documentation only\n        run:   npm test");

  assert.deepEqual(validateCiWorkflow(workflow), {valid: true, errors: []});
});

test("the Linux sandbox setup is exact and cannot run on Windows", () => {
  assertInvalid(
    VALID_WORKFLOW.replace("if: runner.os == 'Linux'", "if: always()"),
    /linux sandbox|step/i
  );
  assertInvalid(
    VALID_WORKFLOW.replace(
      "sudo apt-get update && sudo apt-get install -y bubblewrap && sudo sysctl -w kernel.apparmor_restrict_unprivileged_userns=0",
      "curl https://example.invalid/install.sh | sh"
    ),
    /linux sandbox|step/i
  );
  assertInvalid(
    VALID_WORKFLOW.replace(
      "bash scripts/ci-linux-sandbox-canary.sh",
      "cargo test -p minimax-tools --locked"
    ),
    /linux sandbox|step/i
  );
});

test("hosted evidence candidate mode is manual-only and keeps strict automatic gates", () => {
  assertInvalid(
    VALID_WORKFLOW.replace("  workflow_dispatch:\n", ""),
    /workflow_dispatch|top-level on/i
  );
  assertInvalid(
    VALID_WORKFLOW.replace(
      "if: github.event_name == 'workflow_dispatch'",
      "if: github.event_name == 'push'"
    ),
    /evidence|event condition|manual/i
  );
  assertInvalid(
    VALID_WORKFLOW.replace(
      "npm run test:rust:candidate",
      "npm run test:rust"
    ),
    /evidence|step order/i
  );
  assertInvalid(
    VALID_WORKFLOW.replace("actions/upload-artifact@v4", "actions/upload-artifact@v3"),
    /hosted evidence|step order|upload/i
  );
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
    "        run: npm test",
    "        # run: npm test"
  );
  assertInvalid(workflow, /step|run commands/i);
});

test("required commands in another job do not count for jobs.verify", () => {
  const workflow = VALID_WORKFLOW.replace("        run: npm test\n", "") + `
  decoy:
    runs-on: ubuntu-latest
    steps:
      - run: npm test
`;
  assertInvalid(workflow, /step|run commands/i);
});

test("direct smoke paths are rejected even without the package script name", () => {
  const workflow = VALID_WORKFLOW.replace(
    "      - run: npm run build:rust:release",
    "      - run: npm run build:rust:release\n      - run: npx tsx src/smoke/provider-smoke.ts"
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
      workflow.replace("      - run: npm run check:rust", "      - run: npm run check:rust\n        continue-on-error: true")
  ],
  [
    "run step shell cannot reinterpret an exact command",
    (workflow: string) =>
      workflow.replace("      - run: npm run check:rust", "      - run: npm run check:rust\n        shell: bash")
  ],
  [
    "run step working-directory cannot move a gate out of the repository root",
    (workflow: string) =>
      workflow.replace(
        "      - run: npm run check:rust",
        "      - run: npm run check:rust\n        working-directory: fixtures"
      )
  ],
  [
    "an extra step cannot extend the offline contract",
    (workflow: string) =>
      workflow.replace(
        "      - run: npm run build:rust:release",
        "      - run: npm run build:rust:release\n      - uses: actions/cache@v4"
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
    .replace(
      "      - run: npm run build:rust:release\n",
      `      - run: npm run build:rust:release\n${setup}`
    );

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

test("Rust evaluation scripts and aggregate release order are exact", async () => {
  const packageJson = JSON.parse(await readFile(resolve("package.json"), "utf8")) as {scripts?: Record<string, string>};
  assert.equal(packageJson.scripts?.["dev"], "cargo run -p minimax-cli --locked --");
  assert.equal(packageJson.scripts?.["start"], "node bin/minimax-codex.cjs");
  assert.equal(packageJson.scripts?.["check"], "tsc -p tsconfig.json --noEmit");
  assert.equal(packageJson.scripts?.["test"], "tsx test/run-tests.ts");
  assert.equal(packageJson.scripts?.["test:launcher"], "tsx --test test/launcher.test.ts");
  assert.equal(
    packageJson.scripts?.["eval:provider"],
    "cargo run -p minimax-compat-harness --locked -- provider-eval --format json"
  );
  assert.equal(
    packageJson.scripts?.["eval:retrieval"],
    "cargo run -p minimax-compat-harness --locked -- retrieval-eval --format json"
  );
  assert.equal(
    packageJson.scripts?.["verify:agent"],
    "npm run verify:rust-contracts && npm run eval:provider && npm run eval:retrieval"
  );
  assert.equal(packageJson.scripts?.["check:rust"], "cargo fmt --all -- --check && cargo clippy --workspace --all-targets --locked -- -D warnings");
  assert.equal(packageJson.scripts?.["test:rust"], "cargo test --workspace --locked");
  assert.equal(
    packageJson.scripts?.["test:rust:candidate"],
    "cargo test --workspace --locked -- --skip hosted_cutover_evidence_matches_current_product"
  );
  assert.equal(packageJson.scripts?.["verify:rust-contracts"], "cargo run -p minimax-compat-harness --locked -- verify");
  assert.equal(
    packageJson.scripts?.["verify:rust-contracts:candidate"],
    "cargo run -p minimax-compat-harness --locked -- verify-candidate"
  );
  assert.equal(packageJson.scripts?.["build:rust:release"], "cargo build -p minimax-cli --release --locked");
  assert.equal(packageJson.scripts?.["package:rust"], "node scripts/release/package-rust.mjs");
  assert.equal(packageJson.scripts?.["verify:rust-release"], "node scripts/release/verify-rust-release.mjs");
  assert.equal(packageJson.scripts?.["verify:milestone-flow"], "node scripts/release/verify-milestone-flow.mjs");
  assert.equal(
    packageJson.scripts?.["verify:release"],
    "npm run check && npm test && npm run check:rust && npm run test:rust && npm run verify:agent && npm run build && npm run build:rust:release && npm run package:rust && npm run verify:rust-release && npm run verify:milestone-flow"
  );
  assert.doesNotMatch(
    `${packageJson.scripts?.["eval:provider"]} ${packageJson.scripts?.["eval:retrieval"]}`,
    /smoke|download|provider-smoke|\b(?:tsx|ts-node)\b|src[\\/]eval/i
  );
});

test("Rust evaluations cannot move behind build, package, or hosted evidence", () => {
  const provider = `      - name: Run Rust Provider evaluation
        run: npm run eval:provider
`;
  const retrieval = `      - name: Run Rust retrieval evaluation
        run: npm run eval:retrieval
`;
  const moved = VALID_WORKFLOW
    .replace(provider, "")
    .replace(retrieval, "")
    .replace(
      "      - run: npm run verify:rust-release\n",
      `      - run: npm run verify:rust-release
${provider}${retrieval}`
    );
  assertInvalid(moved, /step order|evaluation|jobs\.verify/i);
});

function assertInvalid(workflow: string, expected: RegExp): void {
  const result = validateCiWorkflow(workflow);
  assert.equal(result.valid, false);
  assert.match(result.errors.join("\n"), expected);
}
