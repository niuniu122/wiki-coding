use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use minimax_compat_harness::{
    RETRIEVAL_EVALUATION_GOLDEN, RetrievalEvaluationError, repository_root, retrieval_report_json,
    run_retrieval_evaluation, verify_retrieval_evaluation,
};
use serde_json::{Value, json};

static TEMP_ID: AtomicU64 = AtomicU64::new(1);

#[test]
fn retrieval_evaluation_matches_committed_golden_and_is_repeatable() {
    let root = repository_root();
    let first = run_retrieval_evaluation(&root).expect("first retrieval evaluation");
    let second = run_retrieval_evaluation(&root).expect("second retrieval evaluation");
    assert_eq!(first, second);
    assert!(first.passed);

    let actual = retrieval_report_json(&first).expect("retrieval report JSON");
    let expected = fs::read_to_string(root.join(RETRIEVAL_EVALUATION_GOLDEN))
        .expect("retrieval report golden");
    assert_eq!(actual, normalize_newline(&expected));
    assert_eq!(
        verify_retrieval_evaluation(&root).expect("verified retrieval report"),
        first
    );
}

#[test]
fn report_proves_locked_corpus_metrics_candidate_isolation_and_degradation() {
    let report = run_retrieval_evaluation(&repository_root()).expect("retrieval evaluation");
    let value = serde_json::to_value(&report).expect("report value");

    assert_eq!(
        value["corpus"]["fingerprint"],
        "cb0426f3fc7c111fe06b7a26b0d607eeadc6d165db3faeb85e09f8d898e15a65"
    );
    assert_eq!(value["corpus"]["cases"], 175);
    assert_eq!(value["corpus"]["positiveCases"], 135);
    assert_eq!(value["corpus"]["noMatchCases"], 40);
    assert!(value["corpus"]["exactCases"].as_u64().expect("exact cases") > 0);
    assert!(value["corpus"]["bm25Cases"].as_u64().expect("BM25 cases") > 0);
    assert_eq!(value["corpus"]["metrics"]["recallAt5"], 1.0);
    assert_eq!(value["corpus"]["metrics"]["top1"], 1.0);
    assert_eq!(value["corpus"]["metrics"]["mrr"], 1.0);
    assert_eq!(value["corpus"]["metrics"]["noMatchPrecision"], 1.0);
    assert_eq!(value["corpus"]["metrics"]["idValidity"], 1.0);

    let boundary = &value["candidateBoundary"];
    assert_eq!(
        boundary["observedCandidateIds"],
        boundary["lexicalCandidateIds"]
    );
    assert_eq!(
        boundary["semanticCandidateIds"],
        boundary["lexicalCandidateIds"]
    );
    assert_eq!(boundary["outsiderAttemptedId"], "outside/bm25");
    assert_eq!(
        boundary["outsiderResultIds"],
        boundary["lexicalCandidateIds"]
    );
    assert_eq!(boundary["outsiderRejected"], true);

    let degradations = value["degradations"]
        .as_array()
        .expect("degradation scenarios");
    assert_eq!(
        degradations
            .iter()
            .map(|item| item["scenario"].as_str().expect("scenario"))
            .collect::<Vec<_>>(),
        [
            "no_resource",
            "damaged_resource",
            "runner_failure",
            "malformed_response",
            "timeout",
            "outsider",
        ]
    );
    assert!(
        degradations
            .iter()
            .all(|item| item["mode"] == "bm25" && item["bm25IdsPreserved"] == true)
    );
    assert_eq!(value["workspace"]["cases"], 15);
    assert_eq!(value["workspace"]["passedCases"], 15);
    assert_eq!(
        value["disabledPath"],
        json!({
            "networkRequests": 0,
            "providerRequests": 0,
            "modelDownloads": 0,
            "modelLoads": 0
        })
    );
    assert_eq!(value["passed"], true);
}

#[test]
fn strict_manifest_and_golden_drift_fail_closed() {
    let root = repository_root();
    let repository = FixtureRepository::copy_from(&root);
    let manifest_path = repository.path("fixtures/compat/evaluations/retrieval.v1.json");
    let mut manifest: Value =
        serde_json::from_str(&fs::read_to_string(&manifest_path).expect("retrieval manifest"))
            .expect("retrieval manifest JSON");
    manifest["surprise"] = Value::Bool(true);
    fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).expect("manifest JSON") + "\n",
    )
    .expect("write manifest mutation");
    assert!(matches!(
        run_retrieval_evaluation(repository.root()),
        Err(RetrievalEvaluationError::ManifestParse(_))
    ));

    let repository = FixtureRepository::copy_from(&root);
    fs::write(repository.path(RETRIEVAL_EVALUATION_GOLDEN), "{}\n").expect("write golden drift");
    assert_eq!(
        verify_retrieval_evaluation(repository.root()),
        Err(RetrievalEvaluationError::GoldenDrift)
    );
}

#[test]
fn retrieval_eval_command_is_json_only_and_deterministic() {
    let binary = env!("CARGO_BIN_EXE_minimax-compat-harness");
    let run = || {
        Command::new(binary)
            .args(["retrieval-eval", "--format", "json"])
            .current_dir(repository_root())
            .env("MINIMAX_API_KEY", "must-not-be-read")
            .env("OPENAI_API_KEY", "must-not-be-read")
            .output()
            .expect("retrieval evaluation command")
    };
    let first = run();
    let second = run();
    assert!(first.status.success());
    assert!(second.status.success());
    assert!(first.stderr.is_empty());
    assert!(second.stderr.is_empty());
    assert_eq!(first.stdout, second.stdout);
    let value: Value = serde_json::from_slice(&first.stdout).expect("stdout report JSON");
    assert_eq!(value["passed"], true);
}

#[test]
fn repository_verification_runs_retrieval_evaluation_before_compatibility_decision() {
    let main = fs::read_to_string(repository_root().join("crates/compat-harness/src/main.rs"))
        .expect("compatibility harness main");
    let verification = &main[main
        .find("fn verify_repository")
        .expect("repository verification function")..];
    let retrieval_gate = verification
        .find("verify_retrieval_evaluation")
        .expect("retrieval evaluation gate");
    let compatibility = verification
        .find("load_compat_manifests")
        .expect("compatibility manifest gate");
    assert!(retrieval_gate < compatibility);
}

fn normalize_newline(value: &str) -> String {
    value.trim_end_matches(['\r', '\n']).to_owned() + "\n"
}

struct FixtureRepository {
    root: PathBuf,
}

impl FixtureRepository {
    fn copy_from(repository: &Path) -> Self {
        let id = TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "minimax-retrieval-eval-{}-{id}",
            std::process::id()
        ));
        if root.exists() {
            fs::remove_dir_all(&root).expect("remove stale fixture repository");
        }
        for relative in [
            "fixtures/compat/evaluations/retrieval.v1.json",
            "fixtures/compat/evaluations/retrieval-report.expected.json",
            "fixtures/compat/retrieval/capability-cases-expanded.v1.json",
            "fixtures/compat/retrieval/projects.v1.json",
            "fixtures/compat/retrieval/capability-workspace.v1.json",
            "capabilities/catalogs/projects.v1.json",
            "capabilities/catalogs/skills.v1.json",
            "capabilities/catalogs/mcp.v1.json",
        ] {
            let destination = root.join(relative);
            fs::create_dir_all(destination.parent().expect("fixture parent"))
                .expect("create fixture parent");
            fs::copy(repository.join(relative), destination).expect("copy retrieval fixture");
        }
        Self { root }
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }
}

impl Drop for FixtureRepository {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
