#![allow(unreachable_pub)]

#[path = "../src/source_authority.rs"]
mod source_authority;

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;
use source_authority::{
    LEGACY_FIXTURE_PHASE_11_DISPOSITION, LEGACY_FIXTURE_PHASE_14_ZERO_CONTRACT,
    SourceAuthorityError, load_source_authority, parse_source_authority,
};

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository root should resolve")
}

fn manifest_json(root: &Path) -> String {
    fs::read_to_string(root.join("fixtures/compat/source-authority.v1.json"))
        .expect("source authority manifest should be readable")
}

fn mutated_manifest(root: &Path, mutate: impl FnOnce(&mut Value)) -> String {
    let mut value: Value =
        serde_json::from_str(&manifest_json(root)).expect("manifest should be valid JSON");
    mutate(&mut value);
    serde_json::to_string_pretty(&value).expect("mutated manifest should serialize")
}

fn assert_rejected(root: &Path, json: &str, expected: &str) {
    let error = parse_source_authority(root, json).expect_err("manifest mutation must fail");
    assert!(
        error.to_string().contains(expected),
        "expected {expected:?} in {error:?}"
    );
}

#[test]
fn manifest_schema() {
    let root = repository_root();
    let manifest = load_source_authority(&root).expect("committed source authority should load");

    assert_eq!(manifest.schema_version, 1);
    assert_eq!(manifest.executable_entries.len(), 1);
    assert_eq!(manifest.javascript_allowlist.len(), 5);
    assert_eq!(manifest.transitional_legacy_test_fixtures.entries.len(), 3);
    assert_eq!(manifest.state_authority.writable_roots.len(), 1);
    assert_eq!(manifest.state_authority.writable_roots[0].path, ".minimax");
    assert_eq!(manifest.state_authority.migration_input_roots.len(), 1);
    assert_eq!(
        manifest.state_authority.migration_input_roots[0].path,
        ".mini-codex"
    );

    let legacy_paths = manifest
        .transitional_legacy_test_fixtures
        .entries
        .iter()
        .map(|entry| entry.path.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        legacy_paths,
        [
            "test/fixtures/executors/diag-large.js",
            "test/fixtures/executors/diag-ok.js",
            "test/fixtures/executors/diag-slow.js",
        ]
    );
    assert_eq!(
        manifest
            .transitional_legacy_test_fixtures
            .phase_11_disposition,
        LEGACY_FIXTURE_PHASE_11_DISPOSITION
    );
    assert_eq!(
        manifest
            .transitional_legacy_test_fixtures
            .phase_14_zero_contract,
        LEGACY_FIXTURE_PHASE_14_ZERO_CONTRACT
    );

    let unknown = mutated_manifest(&root, |value| {
        value["unexpectedAuthority"] = Value::Bool(true);
    });
    assert_rejected(&root, &unknown, "unknown field");

    let duplicate = mutated_manifest(&root, |value| {
        let first = value["transitionalTypeScript"]["entries"][0].clone();
        value["transitionalTypeScript"]["entries"]
            .as_array_mut()
            .expect("entries should be an array")
            .push(first);
    });
    assert_rejected(&root, &duplicate, "duplicate-free");

    let unsafe_path = mutated_manifest(&root, |value| {
        value["transitionalTypeScript"]["entries"][0]["path"] =
            Value::String("../outside.ts".to_owned());
    });
    assert_rejected(&root, &unsafe_path, "unsafe repository-relative path");

    let hash_drift = mutated_manifest(&root, |value| {
        value["transitionalTypeScript"]["entries"][0]["sha256"] = Value::String("0".repeat(64));
    });
    assert_rejected(&root, &hash_drift, "hash drift");

    let smuggled_fixture = mutated_manifest(&root, |value| {
        let fixture_path = value["transitionalLegacyTestFixtures"]["entries"][0]["path"].clone();
        let fixture_hash = value["transitionalLegacyTestFixtures"]["entries"][0]["sha256"].clone();
        let mut fixture = value["javascriptAllowlist"][0].clone();
        fixture["path"] = fixture_path;
        fixture["sha256"] = fixture_hash;
        value["javascriptAllowlist"]
            .as_array_mut()
            .expect("allowlist should be an array")[0] = fixture;
    });
    assert_rejected(
        &root,
        &smuggled_fixture,
        "unknown JavaScript authority path",
    );

    assert!(matches!(
        parse_source_authority(&root, "{}"),
        Err(SourceAuthorityError::ManifestParse(_))
    ));
}
