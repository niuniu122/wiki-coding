use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use minimax_compat_harness::{
    validate_migration_fixture_manifest, validate_migration_support_window,
};
use serde_json::{Value, json};

const FIXTURE_RELATIVE: &str = "fixtures/compat/migration/typescript-v1";
const MANIFEST_NAME: &str = "manifest.v1.json";
const SUPPORT_WINDOW_NAME: &str = "support-window.v1.json";

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("repository root")
        .to_path_buf()
}

fn fixture_root(root: &Path) -> PathBuf {
    root.join(FIXTURE_RELATIVE)
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).expect("fixture JSON bytes")).expect("fixture JSON")
}

fn write_json(path: &Path, value: &Value) {
    let mut bytes = serde_json::to_vec_pretty(value).expect("serialize fixture JSON");
    bytes.push(b'\n');
    std::fs::write(path, bytes).expect("write fixture JSON");
}

fn collect_relative_files(root: &Path, directory: &Path, files: &mut Vec<String>) {
    let mut entries = std::fs::read_dir(directory)
        .expect("fixture directory")
        .collect::<Result<Vec<_>, _>>()
        .expect("fixture entries");
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        if entry.file_type().expect("file type").is_dir() {
            collect_relative_files(root, &entry.path(), files);
        } else {
            files.push(
                entry
                    .path()
                    .strip_prefix(root)
                    .expect("relative fixture path")
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
}

struct FixtureCopy {
    root: PathBuf,
}

impl FixtureCopy {
    fn new() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "minimax-migration-support-{}-{id}",
            std::process::id()
        ));
        if root.exists() {
            std::fs::remove_dir_all(&root).expect("remove stale fixture copy");
        }
        copy_directory(&fixture_root(&repository_root()), &fixture_root(&root));
        Self { root }
    }

    fn fixture_root(&self) -> PathBuf {
        fixture_root(&self.root)
    }

    fn manifest_path(&self) -> PathBuf {
        self.fixture_root().join(MANIFEST_NAME)
    }

    fn support_window_path(&self) -> PathBuf {
        self.fixture_root().join(SUPPORT_WINDOW_NAME)
    }
}

impl Drop for FixtureCopy {
    fn drop(&mut self) {
        if self.root.exists() {
            std::fs::remove_dir_all(&self.root).expect("remove fixture copy");
        }
    }
}

fn copy_directory(source: &Path, destination: &Path) {
    std::fs::create_dir_all(destination).expect("destination directory");
    let mut entries = std::fs::read_dir(source)
        .expect("source directory")
        .collect::<Result<Vec<_>, _>>()
        .expect("source entries");
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let target = destination.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_directory(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).expect("copy fixture file");
        }
    }
}

#[test]
fn fixture_manifest_covers_every_source_evidence_file_exactly_once() {
    let root = repository_root();
    validate_migration_fixture_manifest(&root).expect("checked-in fixture manifest");

    let manifest = read_json(&fixture_root(&root).join(MANIFEST_NAME));
    assert_eq!(manifest["schemaVersion"], 1);
    assert_eq!(manifest["fixtureVersion"], "typescript-v1");
    assert_eq!(
        manifest["provenance"]["sourceProduct"],
        "minimax-codex-typescript-v1"
    );
    assert_eq!(manifest["provenance"]["migrationAuthority"], "rust");
    assert_eq!(
        manifest["metadataFilesExcludedFromFingerprint"],
        json!([MANIFEST_NAME, SUPPORT_WINDOW_NAME])
    );

    let entries = manifest["files"].as_array().expect("manifest files");
    let paths = entries
        .iter()
        .map(|entry| {
            assert!(entry["byteLength"].as_u64().is_some());
            assert_eq!(entry["sha256"].as_str().expect("sha256").len(), 64);
            assert!(matches!(
                entry["expectedDisposition"].as_str(),
                Some("include") | Some("exclude")
            ));
            assert!(entry["role"].as_str().is_some_and(|role| !role.is_empty()));
            entry["relativePath"]
                .as_str()
                .expect("relative path")
                .to_owned()
        })
        .collect::<Vec<_>>();
    assert_eq!(paths.len(), paths.iter().collect::<BTreeSet<_>>().len());

    let mut discovered = Vec::new();
    collect_relative_files(&fixture_root(&root), &fixture_root(&root), &mut discovered);
    discovered.retain(|path| !matches!(path.as_str(), MANIFEST_NAME | SUPPORT_WINDOW_NAME));
    discovered.sort();
    let mut recorded = paths;
    recorded.sort();
    assert_eq!(recorded, discovered);
}

#[test]
fn fixture_manifest_rejects_tamper_missing_extra_duplicate_and_metadata_self_entry() {
    for case in ["tamper", "missing", "extra", "duplicate", "metadata"] {
        let fixture = FixtureCopy::new();
        match case {
            "tamper" => std::fs::write(
                fixture.fixture_root().join("capability-snapshot.json"),
                b"changed evidence",
            )
            .expect("tamper fixture"),
            "missing" => {
                std::fs::remove_file(fixture.fixture_root().join("indexes/capability.cache"))
                    .expect("remove fixture")
            }
            "extra" => std::fs::write(fixture.fixture_root().join("extra.json"), b"{}\n")
                .expect("add fixture"),
            "duplicate" => {
                let mut manifest = read_json(&fixture.manifest_path());
                let duplicate = manifest["files"][0].clone();
                manifest["files"]
                    .as_array_mut()
                    .expect("files")
                    .push(duplicate);
                write_json(&fixture.manifest_path(), &manifest);
            }
            "metadata" => {
                let mut manifest = read_json(&fixture.manifest_path());
                manifest["files"]
                    .as_array_mut()
                    .expect("files")
                    .push(json!({
                        "relativePath": MANIFEST_NAME,
                        "byteLength": 1,
                        "sha256": "0".repeat(64),
                        "role": "metadata",
                        "expectedDisposition": "exclude",
                        "exclusionReason": "self"
                    }));
                write_json(&fixture.manifest_path(), &manifest);
            }
            _ => unreachable!(),
        }
        assert!(
            validate_migration_fixture_manifest(&fixture.root).is_err(),
            "{case} must fail closed"
        );
    }
}

#[test]
fn support_window_is_counted_from_distinct_ordered_public_releases_after_v3() {
    let root = repository_root();
    let status = validate_migration_support_window(&root).expect("checked-in support window");
    assert_eq!(status.cutover_release, "3.0.0");
    assert_eq!(status.minimum_subsequent_public_releases, 2);
    assert_eq!(status.observed_subsequent_public_releases, 0);
    assert!(!status.removal_eligible);

    let fixture = FixtureCopy::new();
    let mut support = read_json(&fixture.support_window_path());
    support["observedPublicReleases"] = json!(["3.0.1", "3.1.0"]);
    support["removalEligible"] = Value::Bool(true);
    write_json(&fixture.support_window_path(), &support);
    let eligible =
        validate_migration_support_window(&fixture.root).expect("two later public releases");
    assert_eq!(eligible.observed_subsequent_public_releases, 2);
    assert!(eligible.removal_eligible);
}

#[test]
fn support_window_rejects_premature_duplicate_pre_v3_unordered_and_non_public_evidence() {
    let cases = [
        ("premature", json!(["3.0.1"]), true),
        ("duplicate", json!(["3.0.1", "3.0.1"]), false),
        ("cutover", json!(["3.0.0", "3.0.1"]), false),
        ("pre-v3", json!(["2.9.9", "3.0.1"]), false),
        ("unordered", json!(["3.1.0", "3.0.1"]), true),
        ("prerelease", json!(["3.0.1-rc.1", "3.0.1"]), true),
        ("computed mismatch", json!(["3.0.1", "3.1.0"]), false),
    ];
    for (name, releases, claimed_eligible) in cases {
        let fixture = FixtureCopy::new();
        let mut support = read_json(&fixture.support_window_path());
        support["observedPublicReleases"] = releases;
        support["removalEligible"] = Value::Bool(claimed_eligible);
        write_json(&fixture.support_window_path(), &support);
        assert!(
            validate_migration_support_window(&fixture.root).is_err(),
            "{name} must fail closed"
        );
    }
}
