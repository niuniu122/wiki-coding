use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use minimax_compat_harness::{
    provider_evaluation_authorizes_release, provider_report_json, run_provider_evaluation,
    verify_provider_evaluation,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

static TEMP_ID: AtomicU64 = AtomicU64::new(0);

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("repository root")
        .to_path_buf()
}

#[test]
fn provider_evaluation_matches_committed_golden_and_is_repeatable() {
    let root = repository_root();
    let first = run_provider_evaluation(&root).expect("first provider evaluation");
    let second = run_provider_evaluation(&root).expect("second provider evaluation");
    let first_json = provider_report_json(&first).expect("first provider report JSON");
    let second_json = provider_report_json(&second).expect("second provider report JSON");
    let golden =
        fs::read_to_string(root.join("fixtures/compat/evaluations/provider-report.expected.json"))
            .expect("provider report golden");

    assert_eq!(first, second);
    assert_eq!(first_json, normalize_newline(&golden));
    assert_eq!(first_json, second_json);
    assert!(first.passed);
    assert_eq!(first.protocols.len(), 2);
    assert_eq!(first.totals.protocols, 2);
    assert_eq!(first.totals.checks, 20);
    assert_eq!(first.totals.passed, 20);
    assert_eq!(first.totals.failed, 0);
    assert!(
        first
            .protocols
            .iter()
            .all(|protocol| protocol.checks.len() == 10 && protocol.passed)
    );
}

#[test]
fn provider_manifest_rejects_unknown_fields_duplicate_checks_and_fingerprint_drift() {
    let repository = repository_root();

    let unknown = FixtureRepository::copy_from(&repository);
    let manifest_path = unknown.path("fixtures/compat/evaluations/provider.v1.json");
    let raw = fs::read_to_string(&manifest_path).expect("provider manifest");
    fs::write(
        &manifest_path,
        raw.replacen(
            "\"schemaVersion\": 1,",
            "\"schemaVersion\": 1, \"unknown\": true,",
            1,
        ),
    )
    .expect("mutate manifest");
    assert!(
        run_provider_evaluation(unknown.root())
            .expect_err("unknown manifest field must fail")
            .to_string()
            .contains("manifest")
    );

    let duplicate = FixtureRepository::copy_from(&repository);
    let manifest_path = duplicate.path("fixtures/compat/evaluations/provider.v1.json");
    let mut manifest: Value =
        serde_json::from_str(&fs::read_to_string(&manifest_path).expect("provider manifest"))
            .expect("provider manifest JSON");
    let checks = manifest["protocols"][0]["requiredChecks"]
        .as_array_mut()
        .expect("required checks");
    checks[1] = checks[0].clone();
    fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).expect("mutated manifest JSON") + "\n",
    )
    .expect("write duplicate check");
    assert!(
        run_provider_evaluation(duplicate.root())
            .expect_err("duplicate check must fail")
            .to_string()
            .contains("required check")
    );

    let drifted = FixtureRepository::copy_from(&repository);
    fs::write(
        drifted.path("fixtures/compat/provider-streams/responses.valid.jsonl"),
        "fixture drift\n",
    )
    .expect("mutate provider fixture");
    assert!(
        run_provider_evaluation(drifted.root())
            .expect_err("fixture fingerprint drift must fail")
            .to_string()
            .contains("fingerprint")
    );
}

#[test]
fn provider_evaluation_fails_when_a_valid_stream_loses_its_terminal_event() {
    let repository = repository_root();
    let fixture = FixtureRepository::copy_from(&repository);
    let fixture_path = fixture.path("fixtures/compat/provider-streams/responses.valid.jsonl");
    let raw = fs::read_to_string(&fixture_path).expect("responses fixture");
    let mut lines = raw.lines().map(str::to_owned).collect::<Vec<_>>();
    let mut first: Value = serde_json::from_str(&lines[0]).expect("first response case");
    first["raw"].as_array_mut().expect("raw frames").pop();
    first["expected_events"]
        .as_array_mut()
        .expect("expected events")
        .pop();
    lines[0] = serde_json::to_string(&first).expect("mutated response case");
    fs::write(&fixture_path, lines.join("\n") + "\n").expect("write response fixture");
    update_manifest_hash(
        fixture.root(),
        "fixtures/compat/provider-streams/responses.valid.jsonl",
    );

    let report = run_provider_evaluation(fixture.root()).expect("failed evaluation report");
    assert!(!report.passed);
    assert!(report.totals.failed > 0);
    assert!(
        report.protocols[0]
            .checks
            .iter()
            .any(|check| check.id == "terminal_ordering" && !check.passed)
    );
}

#[test]
fn provider_golden_drift_is_release_blocking() {
    let repository = repository_root();
    let fixture = FixtureRepository::copy_from(&repository);
    fs::write(
        fixture.path("fixtures/compat/evaluations/provider-report.expected.json"),
        "{}\n",
    )
    .expect("mutate provider golden");

    assert!(
        verify_provider_evaluation(fixture.root())
            .expect_err("golden drift must fail")
            .to_string()
            .contains("golden")
    );
}

#[test]
fn provider_eval_command_is_json_only_deterministic_and_credential_independent() {
    let root = repository_root();
    let binary = env!("CARGO_BIN_EXE_minimax-compat-harness");
    let baseline = Command::new(binary)
        .args(["provider-eval", "--format", "json"])
        .current_dir(&root)
        .output()
        .expect("run Provider evaluator");
    let secret_environment = Command::new(binary)
        .args(["provider-eval", "--format", "json"])
        .current_dir(&root)
        .env("MINIMAX_API_KEY", "SECRET_MINIMAX_CREDENTIAL")
        .env("HASHSIGHT_API_KEY", "SECRET_HASHSIGHT_CREDENTIAL")
        .env("OPENAI_API_KEY", "SECRET_OPENAI_CREDENTIAL")
        .env("TEST_PROVIDER_KEY", "SECRET_TEST_CREDENTIAL")
        .output()
        .expect("run Provider evaluator with secret-bearing environment");
    let golden = fs::read(root.join("fixtures/compat/evaluations/provider-report.expected.json"))
        .expect("Provider report golden");

    assert!(baseline.status.success());
    assert!(secret_environment.status.success());
    assert!(baseline.stderr.is_empty());
    assert!(secret_environment.stderr.is_empty());
    assert_eq!(baseline.stdout, golden);
    assert_eq!(secret_environment.stdout, baseline.stdout);
    assert!(!String::from_utf8_lossy(&baseline.stdout).contains("SECRET_"));
}

#[test]
fn provider_failure_blocks_release_even_when_package_smoke_succeeds() {
    let root = repository_root();
    let passing = run_provider_evaluation(&root).expect("passing Provider evaluation");
    assert!(provider_evaluation_authorizes_release(&passing, true));

    let mut failing = passing;
    failing.protocols[0].checks[0].passed = false;
    failing.protocols[0].passed = false;
    failing.totals.passed -= 1;
    failing.totals.failed += 1;
    failing.passed = false;
    assert!(!provider_evaluation_authorizes_release(&failing, true));
}

#[test]
fn package_alias_and_repository_verification_use_the_rust_provider_gate() {
    let root = repository_root();
    let package: Value = serde_json::from_str(
        &fs::read_to_string(root.join("package.json")).expect("package metadata"),
    )
    .expect("package JSON");
    assert_eq!(
        package["scripts"]["eval:provider"],
        "cargo run -p minimax-compat-harness --locked -- provider-eval --format json"
    );

    let main = fs::read_to_string(root.join("crates/compat-harness/src/main.rs"))
        .expect("compatibility harness main");
    let verification = &main[main
        .find("fn verify_repository")
        .expect("repository verification function")..];
    let provider_gate = verification
        .find("verify_provider_fixtures")
        .expect("Rust Provider fixture gate");
    let compatibility = verification
        .find("verify_fixture_compatibility")
        .expect("fixture compatibility gate");
    assert!(compatibility < provider_gate);
}

#[test]
fn immutable_retrieval_corpus_preserves_transitional_175_case_contract() {
    let root = repository_root();
    let source_path = "test/fixtures/capabilities/retrieval-cases-expanded.json";
    let target_path = "fixtures/compat/retrieval/capability-cases-expanded.v1.json";
    let source_bytes = fs::read(root.join(source_path)).expect("transitional retrieval corpus");
    let source: Value = serde_json::from_slice(&source_bytes).expect("source retrieval JSON");
    let target: Value = serde_json::from_str(
        &fs::read_to_string(root.join(target_path)).expect("immutable retrieval corpus"),
    )
    .expect("immutable retrieval JSON");

    assert_object_keys(
        &target,
        &[
            "caseGroups",
            "corpusFingerprint",
            "corpusId",
            "descriptors",
            "schemaVersion",
            "source",
            "thresholds",
        ],
    );
    assert_object_keys(&target["source"], &["path", "retainedUntil", "sha256"]);
    assert_object_keys(
        &target["thresholds"],
        &[
            "idValidity",
            "minimumCases",
            "mrr",
            "noMatchPrecision",
            "recallAt5",
            "top1",
        ],
    );
    assert_eq!(target["schemaVersion"], 1);
    assert_eq!(target["corpusId"], "capability-retrieval-expanded-v1");
    assert_eq!(target["source"]["path"], source_path);
    assert_eq!(target["source"]["sha256"], sha256(&source_bytes));
    assert_eq!(target["source"]["retainedUntil"], "14-01");
    assert_eq!(target["descriptors"], source["descriptors"]);
    assert_eq!(
        target["thresholds"],
        json!({
            "minimumCases": 150,
            "recallAt5": 0.95,
            "top1": 0.85,
            "mrr": 0.9,
            "noMatchPrecision": 0.95,
            "idValidity": 1
        })
    );

    let source_groups = source["caseGroups"].as_array().expect("source groups");
    let target_groups = target["caseGroups"].as_array().expect("target groups");
    assert_eq!(target_groups.len(), source_groups.len());
    let mut query_ids = std::collections::BTreeSet::new();
    let mut cases = 0usize;
    for (source_group, target_group) in source_groups.iter().zip(target_groups) {
        assert_object_keys(
            target_group,
            &["expectedIds", "id", "noMatch", "queries", "queryIds"],
        );
        assert_eq!(target_group["expectedIds"], source_group["expectedIds"]);
        assert_eq!(
            target_group["noMatch"].as_bool().expect("target noMatch"),
            source_group["noMatch"].as_bool().unwrap_or(false)
        );
        assert_eq!(target_group["queries"], source_group["queries"]);
        let queries = target_group["queries"].as_array().expect("queries");
        let ids = target_group["queryIds"].as_array().expect("query IDs");
        assert_eq!(ids.len(), queries.len());
        for id in ids {
            let id = id.as_str().expect("stable query ID");
            assert_eq!(id, format!("capability-case-{:03}", cases + 1));
            assert!(query_ids.insert(id));
            cases += 1;
        }
    }
    assert_eq!(cases, 175);
    assert_eq!(query_ids.len(), 175);

    let fingerprint_input = json!({
        "caseGroups": target["caseGroups"],
        "descriptors": target["descriptors"],
        "thresholds": target["thresholds"]
    });
    assert_eq!(
        target["corpusFingerprint"],
        sha256(&serde_json::to_vec(&fingerprint_input).expect("fingerprint input"))
    );

    let matrix: Value = serde_json::from_str(
        &fs::read_to_string(
            root.join("fixtures/compat/verification/typescript-responsibilities.v1.json"),
        )
        .expect("TypeScript responsibility matrix"),
    )
    .expect("responsibility matrix JSON");
    let retrieval = matrix["sources"]
        .as_array()
        .expect("matrix sources")
        .iter()
        .find(|entry| entry["sourcePath"] == "src/eval/capability-retrieval-report.ts")
        .expect("retrieval evaluator responsibility");
    assert!(
        retrieval["responsibilities"][0]["evidence"]
            .as_array()
            .expect("retrieval evidence")
            .iter()
            .any(|evidence| {
                evidence["path"] == "crates/compat-harness/tests/retrieval_eval.rs"
                    && evidence["test"]
                        == "retrieval_evaluation_matches_committed_golden_and_is_repeatable"
            })
    );
    assert!(
        retrieval["responsibilities"][0]["rationale"]
            .as_str()
            .expect("retrieval rationale")
            .contains("Rust retrieval evaluation")
    );
}

fn update_manifest_hash(root: &Path, fixture_path: &str) {
    let manifest_path = root.join("fixtures/compat/evaluations/provider.v1.json");
    let mut manifest: Value =
        serde_json::from_str(&fs::read_to_string(&manifest_path).expect("provider manifest"))
            .expect("provider manifest JSON");
    let digest = sha256(&fs::read(root.join(fixture_path)).expect("fixture bytes"));
    let fixture = manifest["protocols"]
        .as_array_mut()
        .expect("protocols")
        .iter_mut()
        .find(|protocol| protocol["fixture"]["path"] == fixture_path)
        .expect("fixture reference");
    fixture["fixture"]["sha256"] = Value::String(digest);
    fs::write(
        manifest_path,
        serde_json::to_string_pretty(&manifest).expect("provider manifest JSON") + "\n",
    )
    .expect("write provider manifest");
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn assert_object_keys(value: &Value, expected: &[&str]) {
    let actual = value
        .as_object()
        .expect("strict schema object")
        .keys()
        .map(String::as_str)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(actual, expected.iter().copied().collect());
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
        let root =
            std::env::temp_dir().join(format!("minimax-provider-eval-{}-{id}", std::process::id()));
        if root.exists() {
            fs::remove_dir_all(&root).expect("remove stale fixture repository");
        }
        for relative in [
            "fixtures/compat/evaluations/provider.v1.json",
            "fixtures/compat/evaluations/provider-report.expected.json",
            "fixtures/compat/provider-streams/responses.valid.jsonl",
            "fixtures/compat/provider-streams/chat-completions.valid.jsonl",
            "fixtures/compat/provider-streams/invalid-cases.v1.json",
            "fixtures/compat/providers.v1.json",
            "fixtures/compat/commands.v1.json",
            "fixtures/compat/public-contract.v1.json",
        ] {
            let destination = root.join(relative);
            fs::create_dir_all(destination.parent().expect("fixture parent"))
                .expect("create fixture parent");
            fs::copy(repository.join(relative), destination).expect("copy evaluation fixture");
        }
        let public_contract: Value = serde_json::from_str(
            &fs::read_to_string(repository.join("fixtures/compat/public-contract.v1.json"))
                .expect("public contract fixture"),
        )
        .expect("public contract JSON");
        for evidence in public_contract["items"]
            .as_array()
            .expect("public contract items")
            .iter()
            .flat_map(|item| item["evidence"].as_array().expect("item evidence"))
        {
            let relative = evidence.as_str().expect("evidence path");
            let destination = root.join(relative);
            if destination.exists() {
                continue;
            }
            fs::create_dir_all(destination.parent().expect("evidence parent"))
                .expect("create evidence parent");
            fs::copy(repository.join(relative), destination)
                .expect("copy public contract evidence");
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
