import assert from "node:assert/strict";
import {createHash} from "node:crypto";
import {readFile} from "node:fs/promises";
import {dirname, join, resolve} from "node:path";
import {fileURLToPath} from "node:url";
import test from "node:test";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");

test("Provider evaluator source stays inert and Rust owns the report authority", async () => {
  const evaluatorPath = "src/eval/provider-conformance.ts";
  const testPath = "test/provider-conformance.test.ts";
  const [evaluator, testSource, manifestSource, matrixSource, packageSource, reportGolden] = await Promise.all([
    readFile(join(repositoryRoot, evaluatorPath)),
    readFile(join(repositoryRoot, testPath), "utf8"),
    readFile(join(repositoryRoot, "fixtures/compat/source-authority.v1.json"), "utf8"),
    readFile(join(repositoryRoot, "fixtures/compat/verification/typescript-responsibilities.v1.json"), "utf8"),
    readFile(join(repositoryRoot, "package.json"), "utf8"),
    readFile(join(repositoryRoot, "fixtures/compat/evaluations/provider-report.expected.json"), "utf8")
  ]);
  const manifest = JSON.parse(manifestSource) as SourceAuthorityFixture;
  const matrix = JSON.parse(matrixSource) as ResponsibilityFixture;
  const packageManifest = JSON.parse(packageSource) as PackageFixture;
  const evaluatorEntry = manifest.transitionalTypeScript.entries.find((entry) => entry.path === evaluatorPath);
  const testEntry = manifest.transitionalTypeScript.entries.find((entry) => entry.path === testPath);
  const source = matrix.sources.find((entry) => entry.sourcePath === testPath);
  const contract = matrix.evidenceContracts.find((entry) => entry.id === "provider-evaluation-authority");
  const forbiddenPrefix = "../src/" + "eval/";

  assert.ok(evaluator.length > 0, "the Phase 14 evaluator source input must remain present");
  assert.equal(evaluatorEntry?.sha256, sha256(evaluator));
  assert.equal(testEntry?.sha256, sha256(Buffer.from(testSource)));
  assert.equal(source?.sourceSha256, testEntry?.sha256);
  assert.equal(source?.responsibilities[0]?.disposition, "rust_covered");
  assert.deepEqual(contract?.evidence, [{
    path: "crates/compat-harness/tests/provider_eval.rs",
    test: "provider_evaluation_matches_committed_golden_and_is_repeatable"
  }]);
  assert.equal(
    packageManifest.scripts["eval:provider"],
    "cargo run -p minimax-compat-harness --locked -- provider-eval --format json"
  );
  assert.equal(testSource.includes(`from "${forbiddenPrefix}`), false);
  assert.equal(testSource.includes(`import("${forbiddenPrefix}`), false);
  assert.equal((JSON.parse(reportGolden) as {passed?: boolean}).passed, true);
});

function sha256(value: Uint8Array): string {
  return createHash("sha256").update(value).digest("hex");
}

interface SourceAuthorityFixture {
  transitionalTypeScript: {entries: {path: string; sha256: string}[]};
}

interface ResponsibilityFixture {
  evidenceContracts: {id: string; evidence: {path: string; test?: string}[]}[];
  sources: {
    sourcePath: string;
    sourceSha256: string;
    responsibilities: {disposition: string}[];
  }[];
}

interface PackageFixture {
  scripts: Record<string, string>;
}
