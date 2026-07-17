use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

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
    let source_authority = main
        .find("validate_source_authority")
        .expect("source authority preflight");
    let coverage = main
        .find("validate_coverage_matrix")
        .expect("coverage preflight");
    let compatibility = main
        .find("load_compat_manifests")
        .expect("compatibility manifests");

    assert!(source_authority < coverage);
    assert!(coverage < compatibility);
}
