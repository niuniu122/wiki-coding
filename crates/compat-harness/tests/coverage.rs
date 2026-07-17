use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use minimax_compat_harness::{
    CoverageDisposition, CoverageMatrix, load_coverage_matrix, load_source_authority,
    validate_coverage_matrix,
};

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("repository root")
        .to_path_buf()
}

fn transitional_verification_paths(authority: &Value) -> BTreeSet<(String, String)> {
    let mut paths = authority["transitionalTypeScript"]["entries"]
        .as_array()
        .expect("transitional TypeScript entries")
        .iter()
        .filter_map(|entry| {
            let path = entry["path"].as_str()?;
            (path.starts_with("test/")
                || path.starts_with("src/eval/")
                || path.starts_with("src/smoke/"))
            .then(|| {
                (
                    path.to_owned(),
                    entry["sha256"].as_str().expect("source hash").to_owned(),
                )
            })
        })
        .collect::<BTreeSet<_>>();
    paths.extend(
        authority["transitionalLegacyTestFixtures"]["entries"]
            .as_array()
            .expect("legacy test fixtures")
            .iter()
            .map(|entry| {
                (
                    entry["path"].as_str().expect("fixture path").to_owned(),
                    entry["sha256"].as_str().expect("fixture hash").to_owned(),
                )
            }),
    );
    paths
}

#[test]
fn coverage_matrix_exists_and_matches_the_phase_ten_verification_inventory() {
    let root = repository_root();
    let authority: Value = serde_json::from_str(
        &fs::read_to_string(root.join("fixtures/compat/source-authority.v1.json"))
            .expect("source authority manifest"),
    )
    .expect("source authority JSON");
    let matrix: Value = serde_json::from_str(
        &fs::read_to_string(
            root.join("fixtures/compat/verification/typescript-responsibilities.v1.json"),
        )
        .expect("TypeScript responsibility matrix"),
    )
    .expect("coverage matrix JSON");

    let actual = matrix["sources"]
        .as_array()
        .expect("coverage sources")
        .iter()
        .map(|source| {
            (
                source["sourcePath"]
                    .as_str()
                    .expect("matrix source path")
                    .to_owned(),
                source["sourceSha256"]
                    .as_str()
                    .expect("matrix source hash")
                    .to_owned(),
            )
        })
        .collect::<BTreeSet<_>>();

    assert_eq!(actual, transitional_verification_paths(&authority));
}

#[test]
fn both_compatibility_verification_modes_are_coverage_gated() {
    let main = fs::read_to_string(repository_root().join("crates/compat-harness/src/main.rs"))
        .expect("compatibility main source");
    let verify_repository = main
        .find("fn verify_repository")
        .expect("repository verification function");
    let verification = &main[verify_repository..];
    let source_authority = verification
        .find("validate_source_authority")
        .expect("source authority preflight");
    let coverage = verification
        .find("validate_coverage_matrix")
        .expect("coverage preflight");
    let compatibility = verification
        .find("load_compat_manifests")
        .expect("compatibility manifests");

    assert!(source_authority < coverage);
    assert!(coverage < compatibility);
}

#[test]
fn repository_matrix_validates_with_no_unresolved_responsibility() {
    let root = repository_root();
    let authority = load_source_authority(&root).expect("source authority");
    let matrix = load_coverage_matrix(&root).expect("coverage matrix");

    validate_coverage_matrix(&root, &matrix, &authority).expect("valid coverage matrix");
    assert_eq!(matrix.sources.len(), 97);
    assert!(matrix.sources.iter().all(|source| {
        !source.responsibilities.is_empty()
            && source
                .responsibilities
                .iter()
                .all(|responsibility| !responsibility.evidence.is_empty())
    }));
}

#[test]
fn missing_source_hash_drift_and_duplicate_responsibility_are_rejected() {
    let root = repository_root();
    let authority = load_source_authority(&root).expect("source authority");
    let matrix = load_coverage_matrix(&root).expect("coverage matrix");

    let mut missing = matrix.clone();
    missing.sources.remove(0);
    assert!(
        validate_coverage_matrix(&root, &missing, &authority)
            .expect_err("missing source must fail")
            .to_string()
            .contains("missing source")
    );

    let mut drifted = matrix.clone();
    drifted.sources[0].source_sha256 = "0".repeat(64);
    assert!(
        validate_coverage_matrix(&root, &drifted, &authority)
            .expect_err("hash drift must fail")
            .to_string()
            .contains("source hash")
    );

    let mut duplicate = matrix.clone();
    duplicate.sources[1].responsibilities[0].id =
        duplicate.sources[0].responsibilities[0].id.clone();
    assert!(
        validate_coverage_matrix(&root, &duplicate, &authority)
            .expect_err("duplicate responsibility must fail")
            .to_string()
            .contains("duplicate responsibility")
    );
}

#[test]
fn unresolved_unknown_fields_and_typescript_evidence_are_rejected() {
    let root = repository_root();
    let raw = fs::read_to_string(
        root.join("fixtures/compat/verification/typescript-responsibilities.v1.json"),
    )
    .expect("coverage matrix");
    let unknown = raw.replacen(
        "\"schemaVersion\": 1,",
        "\"schemaVersion\": 1, \"unknown\": true,",
        1,
    );
    assert!(serde_json::from_str::<CoverageMatrix>(&unknown).is_err());
    let unresolved = raw.replacen("\"rust_covered\"", "\"requires_port\"", 1);
    assert!(serde_json::from_str::<CoverageMatrix>(&unresolved).is_err());

    let authority = load_source_authority(&root).expect("source authority");
    let mut matrix = load_coverage_matrix(&root).expect("coverage matrix");
    let responsibility = &mut matrix.sources[0].responsibilities[0];
    responsibility.evidence[0].path = "test/run-tests.ts".to_owned();
    responsibility.evidence[0].test = None;
    assert!(
        validate_coverage_matrix(&root, &matrix, &authority)
            .expect_err("TypeScript evidence must fail")
            .to_string()
            .contains("not Rust or allowed package orchestration")
    );
}

#[test]
fn retirement_of_a_locked_public_contract_and_missing_evidence_are_rejected() {
    let root = repository_root();
    let authority = load_source_authority(&root).expect("source authority");
    let matrix = load_coverage_matrix(&root).expect("coverage matrix");
    let public_index = matrix
        .sources
        .iter()
        .position(|source| source.source_path == "test/chat-input-policy.test.ts")
        .expect("public command responsibility");

    let mut retired_public = matrix.clone();
    retired_public.sources[public_index].responsibilities[0].disposition =
        CoverageDisposition::Retired;
    retired_public.sources[public_index].responsibilities[0].rationale =
        "Dormant internal command behavior was retired despite its locked public contract."
            .to_owned();
    assert!(
        validate_coverage_matrix(&root, &retired_public, &authority)
            .expect_err("public retirement must fail")
            .to_string()
            .contains("locked public contract")
    );

    let mut missing_evidence = matrix;
    missing_evidence.sources[public_index].responsibilities[0]
        .evidence
        .clear();
    assert!(
        validate_coverage_matrix(&root, &missing_evidence, &authority)
            .expect_err("missing evidence must fail")
            .to_string()
            .contains("no replacement evidence")
    );
}
