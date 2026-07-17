use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use minimax_compat_harness::{
    provider_report_json, run_provider_evaluation, verify_provider_evaluation,
};
use serde_json::Value;
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
    let golden = fs::read_to_string(
        root.join("fixtures/compat/evaluations/provider-report.expected.json"),
    )
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
    let mut manifest: Value = serde_json::from_str(
        &fs::read_to_string(&manifest_path).expect("provider manifest"),
    )
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

fn update_manifest_hash(root: &Path, fixture_path: &str) {
    let manifest_path = root.join("fixtures/compat/evaluations/provider.v1.json");
    let mut manifest: Value = serde_json::from_str(
        &fs::read_to_string(&manifest_path).expect("provider manifest"),
    )
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
            "minimax-provider-eval-{}-{id}",
            std::process::id()
        ));
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
            "fixtures/compat/baseline-status.v1.json",
        ] {
            let destination = root.join(relative);
            fs::create_dir_all(destination.parent().expect("fixture parent"))
                .expect("create fixture parent");
            fs::copy(repository.join(relative), destination).expect("copy evaluation fixture");
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
