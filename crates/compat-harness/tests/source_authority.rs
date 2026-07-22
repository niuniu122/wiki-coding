#![allow(unreachable_pub)]

#[path = "../src/source_authority.rs"]
mod source_authority;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;
use sha2::{Digest, Sha256};
use source_authority::{
    CI_WORKFLOW, LEGACY_FIXTURE_PHASE_11_DISPOSITION, LEGACY_FIXTURE_PHASE_14_ZERO_CONTRACT,
    NPM_RELEASE_WORKFLOW, SourceAuthorityError, load_source_authority, parse_source_authority,
    validate_ci_workflow_text, validate_npm_release_workflow_text, validate_source_authority,
};

static NEXT_SYNTHETIC_ROOT: AtomicU64 = AtomicU64::new(1);

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

struct SyntheticRepository {
    root: PathBuf,
}

impl SyntheticRepository {
    fn new() -> Self {
        let sequence = NEXT_SYNTHETIC_ROOT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "minimax-source-authority-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("synthetic repository root should be created");

        for directory in [
            "crates/cli",
            "crates/compat-harness",
            "crates/core",
            "crates/protocol",
            "crates/provider",
            "crates/retrieval",
            "crates/tools",
            "crates/tui",
            "crates/vault",
            "fixtures/compat/migration",
            "fixtures/compat/provider-streams",
            "fixtures/compat/release",
            "fixtures/compat/retrieval",
            "fixtures/compat/tools",
            "fixtures/compat/vault",
            "fixtures/compat/wiki",
        ] {
            fs::create_dir_all(root.join(directory))
                .expect("synthetic authority directory should be created");
        }

        write_file(
            &root,
            "package.json",
            &fs::read_to_string(repository_root().join("package.json"))
                .expect("committed package manifest should be readable"),
        );
        write_file(
            &root,
            "package-lock.json",
            &fs::read_to_string(repository_root().join("package-lock.json"))
                .expect("committed package lock should be readable"),
        );
        for path in [
            "fixtures/compat/evaluations/provider.v1.json",
            "fixtures/compat/evaluations/retrieval.v1.json",
            "fixtures/compat/migration/typescript-v1/manifest.v1.json",
            "fixtures/compat/migration/typescript-v1/support-window.v1.json",
            "fixtures/compat/public-contract.v1.json",
            "fixtures/compat/release/targets.v1.json",
            "fixtures/compat/verification/typescript-responsibilities.v1.json",
        ] {
            write_file(
                &root,
                path,
                &fs::read_to_string(repository_root().join(path))
                    .expect("retained fixture should be readable"),
            );
        }
        for path in [
            "bin/minimax-codex.cjs",
            "scripts/release/package-contract.mjs",
            "scripts/release/package-contract.test.mjs",
            "scripts/release/package-rust.mjs",
            "scripts/release/product-fingerprint.mjs",
            "scripts/release/verify-milestone-flow.mjs",
            "scripts/release/verify-rust-release.mjs",
        ] {
            write_file(&root, path, "\"use strict\";\n");
        }
        write_file(
            &root,
            CI_WORKFLOW,
            &fs::read_to_string(repository_root().join(CI_WORKFLOW))
                .expect("committed CI workflow should be readable"),
        );
        write_file(
            &root,
            NPM_RELEASE_WORKFLOW,
            &fs::read_to_string(repository_root().join(NPM_RELEASE_WORKFLOW))
                .expect("committed npm release workflow should be readable"),
        );
        let committed = manifest_json(&repository_root());
        let mut manifest: Value =
            serde_json::from_str(&committed).expect("committed manifest should parse");
        manifest["transitionalTypeScript"]["entries"] = serde_json::json!([]);
        manifest["transitionalLegacyTestFixtures"]["entries"] = serde_json::json!([]);
        for entry in manifest["javascriptAllowlist"]
            .as_array_mut()
            .expect("JavaScript allowlist should be an array")
        {
            let path = entry["path"]
                .as_str()
                .expect("authority entry should contain a path");
            entry["sha256"] = Value::String(sha256_file(&root.join(path)));
        }
        write_manifest(&root, &manifest);
        Self { root }
    }

    fn load(&self) -> source_authority::SourceAuthorityManifest {
        load_source_authority(&self.root).expect("synthetic source authority should load")
    }

    fn write_javascript(&self, path: &str, contents: &str) {
        write_file(&self.root, path, contents);
        let mut manifest: Value = serde_json::from_str(&manifest_json(&self.root))
            .expect("synthetic manifest should parse");
        let entry = manifest["javascriptAllowlist"]
            .as_array_mut()
            .expect("JavaScript allowlist should be an array")
            .iter_mut()
            .find(|entry| entry["path"] == path)
            .expect("JavaScript path should be allowlisted");
        entry["sha256"] = Value::String(sha256_file(&self.root.join(path)));
        write_manifest(&self.root, &manifest);
    }

    fn mutate_manifest(&self, mutate: impl FnOnce(&mut Value)) {
        let mut manifest: Value = serde_json::from_str(&manifest_json(&self.root))
            .expect("synthetic manifest should parse");
        mutate(&mut manifest);
        write_manifest(&self.root, &manifest);
    }

    fn set_package_script(&self, name: &str, command: &str) {
        let path = self.root.join("package.json");
        let mut package: Value = serde_json::from_str(
            &fs::read_to_string(&path).expect("synthetic package should be readable"),
        )
        .expect("synthetic package should parse");
        package["scripts"][name] = Value::String(command.to_owned());
        fs::write(
            path,
            serde_json::to_string_pretty(&package).expect("synthetic package should serialize"),
        )
        .expect("synthetic package should be written");
    }

    fn mutate_package(&self, mutate: impl FnOnce(&mut Value)) {
        let path = self.root.join("package.json");
        let mut package: Value = serde_json::from_str(
            &fs::read_to_string(&path).expect("synthetic package should be readable"),
        )
        .expect("synthetic package should parse");
        mutate(&mut package);
        fs::write(
            path,
            serde_json::to_string_pretty(&package).expect("synthetic package should serialize"),
        )
        .expect("synthetic package should be written");
    }
}

impl Drop for SyntheticRepository {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn write_file(root: &Path, path: &str, contents: &str) {
    let absolute = root.join(path);
    if let Some(parent) = absolute.parent() {
        fs::create_dir_all(parent).expect("synthetic file parent should be created");
    }
    fs::write(absolute, contents).expect("synthetic file should be written");
}

fn write_manifest(root: &Path, manifest: &Value) {
    let path = root.join("fixtures/compat/source-authority.v1.json");
    fs::create_dir_all(path.parent().expect("manifest should have a parent"))
        .expect("manifest parent should be created");
    fs::write(
        path,
        serde_json::to_string_pretty(manifest).expect("manifest should serialize"),
    )
    .expect("synthetic manifest should be written");
}

fn sha256_file(path: &Path) -> String {
    Sha256::digest(fs::read(path).expect("hash input should be readable"))
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn assert_validation_rejected(
    label: &str,
    configure: impl FnOnce(&SyntheticRepository),
    expected: &str,
) {
    let repository = SyntheticRepository::new();
    configure(&repository);
    let manifest = repository.load();
    let error = validate_source_authority(&repository.root, &manifest)
        .expect_err("synthetic source authority must be rejected");
    assert!(
        error.to_string().contains(expected),
        "{label}: expected {expected:?} in {error:?}"
    );
}

fn package_scripts(root: &Path) -> serde_json::Map<String, Value> {
    let package: Value = serde_json::from_str(
        &fs::read_to_string(root.join("package.json")).expect("package.json should be readable"),
    )
    .expect("package.json should parse");
    package["scripts"]
        .as_object()
        .expect("package scripts should be an object")
        .clone()
}

#[test]
fn manifest_schema() {
    let root = repository_root();
    let manifest = load_source_authority(&root).expect("committed source authority should load");

    assert_eq!(manifest.schema_version, 1);
    assert_eq!(manifest.executable_entries.len(), 1);
    assert_eq!(manifest.javascript_allowlist.len(), 7);
    assert!(manifest.transitional_type_script.entries.is_empty());
    assert!(
        manifest
            .transitional_legacy_test_fixtures
            .entries
            .is_empty()
    );
    assert_eq!(manifest.state_authority.writable_roots.len(), 1);
    assert_eq!(manifest.state_authority.writable_roots[0].path, ".minimax");
    assert_eq!(manifest.state_authority.migration_input_roots.len(), 1);
    assert_eq!(
        manifest.state_authority.migration_input_roots[0].path,
        ".mini-codex"
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
        let first = value["javascriptAllowlist"][0].clone();
        value["javascriptAllowlist"]
            .as_array_mut()
            .expect("entries should be an array")
            .push(first);
    });
    assert_rejected(&root, &duplicate, "exact distribution allowlist");

    let unsafe_path = mutated_manifest(&root, |value| {
        value["javascriptAllowlist"][0]["path"] = Value::String("../outside.mjs".to_owned());
    });
    assert_rejected(&root, &unsafe_path, "unsafe repository-relative path");

    let hash_drift = mutated_manifest(&root, |value| {
        value["javascriptAllowlist"][0]["sha256"] = Value::String("0".repeat(64));
    });
    assert_rejected(&root, &hash_drift, "hash drift");

    let smuggled_fixture = mutated_manifest(&root, |value| {
        let mut fixture = value["javascriptAllowlist"][0].clone();
        fixture["path"] = Value::String("test/fixtures/executors/diag-ok.js".to_owned());
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

#[test]
fn transitional_typescript_authority_is_permanently_zero() {
    let root = repository_root();
    let json = mutated_manifest(&root, |manifest| {
        manifest["transitionalTypeScript"]["entries"] = serde_json::json!([{
            "path": "src/reintroduced.ts",
            "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
            "purpose": "inertShrinkingEvidence"
        }]);
    });
    assert_rejected(&root, &json, "must remain empty after Phase 14");
}

#[test]
fn legacy_fixture_authority_is_permanently_zero() {
    let root = repository_root();
    let json = mutated_manifest(&root, |manifest| {
        manifest["transitionalLegacyTestFixtures"]["entries"] = serde_json::json!([{
            "path": "test/fixtures/executors/diag-ok.js",
            "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
            "purpose": "executorDiagnosticFixture"
        }]);
    });
    assert_rejected(&root, &json, "must remain empty after Phase 14");
}

#[test]
fn package_contract_sources_have_non_product_authority_classes() {
    let root = repository_root();
    let manifest: Value = serde_json::from_str(&manifest_json(&root))
        .expect("source authority manifest should parse");
    let entries = manifest["javascriptAllowlist"]
        .as_array()
        .expect("JavaScript allowlist should be an array");
    for (path, purpose) in [
        (
            "scripts/release/package-contract.mjs",
            "releaseOrchestration",
        ),
        (
            "scripts/release/package-contract.test.mjs",
            "packageTestOnly",
        ),
        ("scripts/release/package-rust.mjs", "rustReleasePackaging"),
        (
            "scripts/release/verify-milestone-flow.mjs",
            "milestoneVerification",
        ),
        (
            "scripts/release/verify-rust-release.mjs",
            "rustReleaseVerification",
        ),
    ] {
        let entry = entries
            .iter()
            .find(|entry| entry["path"] == path)
            .unwrap_or_else(|| panic!("missing package authority path: {path}"));
        assert_eq!(entry["purpose"], purpose);
        assert_ne!(entry["id"], "npm-launcher");
    }

    let package_test = SyntheticRepository::new();
    package_test.write_javascript(
        "scripts/release/package-contract.test.mjs",
        "const assertionFixture = \"fetch('https://example.invalid/runtime') and spawnSync('node', ['dist/cli.js'])\";\n",
    );
    let manifest = package_test.load();
    validate_source_authority(&package_test.root, &manifest)
        .expect("package-test-only assertion strings must not become executable fallback findings");

    assert_validation_rejected(
        "production package product import",
        |repository| {
            repository.write_javascript(
                "scripts/release/package-contract.mjs",
                "import product from '../../src/runtime/application-kernel.ts';\n",
            );
        },
        "product source import",
    );
    assert_validation_rejected(
        "production package fallback spawn",
        |repository| {
            repository.write_javascript(
                "scripts/release/package-contract.mjs",
                "spawnSync('node', ['dist/cli.js']);\n",
            );
        },
        "fallback",
    );
    assert_validation_rejected(
        "production package runtime download",
        |repository| {
            repository.write_javascript(
                "scripts/release/package-contract.mjs",
                "const runtime = await fetch('https://example.invalid/runtime');\n",
            );
        },
        "runtime download",
    );
}

#[test]
fn repository_source_inventory() {
    let root = repository_root();
    let manifest = load_source_authority(&root).expect("committed source authority should load");
    validate_source_authority(&root, &manifest)
        .expect("committed repository should satisfy source authority");
}

#[test]
fn repository_product_scripts_are_rust_owned() {
    let root = repository_root();
    let scripts = package_scripts(&root);
    assert_eq!(scripts.len(), 16, "only Rust distribution scripts remain");
    for legacy in ["dev", "start", "build", "check", "test", "test:launcher"] {
        assert!(
            scripts.get(legacy).is_none(),
            "legacy script survived: {legacy}"
        );
    }
}

#[test]
fn package_publication_metadata_is_exact_and_dependency_free() {
    let healthy = SyntheticRepository::new();
    let manifest = healthy.load();
    validate_source_authority(&healthy.root, &manifest)
        .expect("approved npm publication metadata should satisfy source authority");

    for (label, configure) in [
        (
            "license drift",
            (|repository: &SyntheticRepository| {
                repository
                    .mutate_package(|package| package["license"] = Value::String("MIT".into()));
            }) as fn(&SyntheticRepository),
        ),
        ("repository drift", |repository: &SyntheticRepository| {
            repository.mutate_package(|package| {
                package["repository"]["url"] =
                    Value::String("https://example.invalid/repo.git".into());
            });
        }),
        ("homepage drift", |repository: &SyntheticRepository| {
            repository.mutate_package(|package| {
                package["homepage"] = Value::String("https://example.invalid".into());
            });
        }),
        ("issue URL drift", |repository: &SyntheticRepository| {
            repository.mutate_package(|package| {
                package["bugs"]["url"] = Value::String("https://example.invalid/issues".into());
            });
        }),
        (
            "publish access drift",
            |repository: &SyntheticRepository| {
                repository.mutate_package(|package| {
                    package["publishConfig"]["access"] = Value::String("restricted".into());
                });
            },
        ),
        (
            "workspace version drift",
            |repository: &SyntheticRepository| {
                repository.mutate_package(|package| {
                    package["version"] = Value::String("9.9.9".into());
                });
            },
        ),
    ] {
        assert_validation_rejected(label, configure, "publication identity");
    }

    assert_validation_rejected(
        "runtime dependency",
        |repository| {
            repository.mutate_package(|package| {
                package["dependencies"] = serde_json::json!({"native-loader": "1.0.0"});
            });
        },
        "dependency or lifecycle",
    );
    assert_validation_rejected(
        "install lifecycle",
        |repository| repository.set_package_script("install", "cargo build --release"),
        "dependency or lifecycle",
    );
}

#[test]
fn rejects_typescript_and_legacy_product_script_routes() {
    for (label, script, command) in [
        ("dev TypeScript CLI", "dev", "tsx src/cli.tsx"),
        ("start TSX CLI", "start", "tsx src/other-cli.tsx"),
        ("compiled legacy alias", "preview", "node dist/cli.js"),
        (
            "named legacy alias",
            "launch:product",
            "minimax-codex-legacy",
        ),
        (
            "equivalent TypeScript alias",
            "serve",
            "tsx src/other-entry.ts",
        ),
    ] {
        assert_validation_rejected(
            label,
            |repository| repository.set_package_script(script, command),
            "only Rust verification and packaging commands",
        );
    }
}

#[test]
fn typescript_build_test_and_smoke_scripts_are_absent() {
    let repository = SyntheticRepository::new();
    let manifest = repository.load();
    validate_source_authority(&repository.root, &manifest)
        .expect("the thin Rust distribution package should satisfy source authority");

    let scripts = package_scripts(&repository.root);
    for name in ["build", "check", "test", "test:launcher", "smoke:provider"] {
        assert!(
            scripts.get(name).is_none(),
            "legacy script survived: {name}"
        );
    }
}

#[test]
fn evaluator_package_scripts_are_rust_only_and_ordered_before_release_builds() {
    let repository = SyntheticRepository::new();
    let scripts = package_scripts(&repository.root);
    assert_eq!(
        scripts.get("eval:provider").and_then(Value::as_str),
        Some("cargo run -p minimax-compat-harness --locked -- provider-eval --format json")
    );
    assert_eq!(
        scripts.get("eval:retrieval").and_then(Value::as_str),
        Some("cargo run -p minimax-compat-harness --locked -- retrieval-eval --format json")
    );
    assert_eq!(
        scripts.get("verify:agent").and_then(Value::as_str),
        Some("npm run verify:rust-contracts && npm run eval:provider && npm run eval:retrieval")
    );
}

#[test]
fn discovered_test_graph_rejects_transitive_typescript_evaluators() {
    let repository = SyntheticRepository::new();
    write_file(
        &repository.root,
        "src/eval/reintroduced.ts",
        "export const evaluator = true;\n",
    );
    write_file(
        &repository.root,
        "test/reintroduced.test.ts",
        "import '../src/eval/reintroduced.js';\n",
    );
    let manifest = repository.load();
    let error = validate_source_authority(&repository.root, &manifest)
        .expect_err("any reintroduced TypeScript evaluator route must fail closed");
    assert!(error.to_string().contains("unclassified TypeScript path"));
}

#[test]
fn rejects_transitional_typescript_evaluator_routes() {
    for (label, script, command) in [
        (
            "retrieval evaluator",
            "eval:retrieval",
            "tsx src/eval/capability-retrieval-report.ts",
        ),
        (
            "Provider evaluator",
            "eval:provider",
            "ts-node src/eval/provider-conformance.ts",
        ),
    ] {
        assert_validation_rejected(
            label,
            |repository| repository.set_package_script(script, command),
            "distribution authority",
        );
    }
}

#[test]
fn rejects_unreviewed_sources_and_javascript_authority() {
    assert_validation_rejected(
        "product import",
        |repository| {
            repository.write_javascript(
                "bin/minimax-codex.cjs",
                "const product = require(\"../src/runtime/application-kernel.ts\");\n",
            );
        },
        "product source import",
    );
    assert_validation_rejected(
        "interpreter fallback",
        |repository| {
            repository.write_javascript(
                "bin/minimax-codex.cjs",
                "spawnSync(\"node\", [\"dist/cli.js\"]);\n",
            );
        },
        "fallback",
    );
    assert_validation_rejected(
        "runtime download",
        |repository| {
            repository.write_javascript(
                "bin/minimax-codex.cjs",
                "const runtime = await fetch(\"https://example.invalid/runtime\");\n",
            );
        },
        "runtime download",
    );
    assert_validation_rejected(
        "domain implementation",
        |repository| {
            repository.write_javascript(
                "bin/minimax-codex.cjs",
                "function providerRequest() { return {session: []}; }\n",
            );
        },
        "product-domain implementation",
    );
    assert_validation_rejected(
        "new TypeScript",
        |repository| write_file(&repository.root, "src/unreviewed.ts", "export {};\n"),
        "unclassified TypeScript path",
    );
    assert_validation_rejected(
        "unknown executable JavaScript",
        |repository| write_file(&repository.root, "bin/unreviewed.cjs", "\"use strict\";\n"),
        "unclassified JavaScript path",
    );
}

#[test]
fn rejects_typescript_compiler_configuration_and_generated_output() {
    for (label, path, contents, expected) in [
        (
            "root TypeScript compiler config",
            "tsconfig.json",
            "{}\n",
            "TypeScript compiler configuration denied",
        ),
        (
            "nested TypeScript compiler config",
            "config/tsconfig.build.json",
            "{}\n",
            "TypeScript compiler configuration denied",
        ),
        (
            "generated legacy output",
            "dist/cli.js",
            "\"use strict\";\n",
            "generated legacy output directory denied",
        ),
    ] {
        assert_validation_rejected(
            label,
            |repository| write_file(&repository.root, path, contents),
            expected,
        );
    }
}

#[test]
fn rejects_typescript_react_and_ink_lock_dependencies() {
    for dependency in ["typescript", "tsx", "ts-node", "react", "ink"] {
        assert_validation_rejected(
            dependency,
            |repository| {
                let path = repository.root.join("package-lock.json");
                let mut lock: Value = serde_json::from_str(
                    &fs::read_to_string(&path).expect("synthetic package lock"),
                )
                .expect("synthetic package lock JSON");
                lock["packages"][""]["dependencies"][dependency] =
                    Value::String("0.0.0-forbidden".to_owned());
                lock["packages"][format!("node_modules/{dependency}")] = serde_json::json!({
                    "version": "0.0.0-forbidden"
                });
                fs::write(
                    path,
                    serde_json::to_vec_pretty(&lock).expect("serialize package lock mutation"),
                )
                .expect("write package lock mutation");
            },
            "package lock must contain only the dependency-free Rust distribution",
        );
    }
}

#[test]
fn rejects_missing_retained_migration_support_and_evaluation_fixtures() {
    for path in [
        "fixtures/compat/evaluations/provider.v1.json",
        "fixtures/compat/evaluations/retrieval.v1.json",
        "fixtures/compat/migration/typescript-v1/manifest.v1.json",
        "fixtures/compat/migration/typescript-v1/support-window.v1.json",
        "fixtures/compat/public-contract.v1.json",
        "fixtures/compat/release/targets.v1.json",
        "fixtures/compat/verification/typescript-responsibilities.v1.json",
    ] {
        let repository = SyntheticRepository::new();
        let manifest = repository.load();
        fs::remove_file(repository.root.join(path))
            .expect("retained fixture should be removed for the mutation");
        let error = validate_source_authority(&repository.root, &manifest)
            .expect_err("missing retained fixture must fail source authority");
        assert!(
            error
                .to_string()
                .contains("required retained compatibility fixture missing"),
            "{path}: unexpected source authority error: {error:?}"
        );
    }
}

#[test]
fn rejects_legacy_fixture_smuggling_and_second_writable_root() {
    let smuggled = SyntheticRepository::new();
    smuggled.mutate_manifest(|manifest| {
        manifest["javascriptAllowlist"][0]["path"] =
            Value::String("test/fixtures/executors/diag-ok.js".to_owned());
    });
    let error = load_source_authority(&smuggled.root)
        .expect_err("legacy fixture must not enter the JavaScript authority class");
    assert!(
        error
            .to_string()
            .contains("unknown JavaScript authority path")
    );

    let second_root = SyntheticRepository::new();
    second_root.mutate_manifest(|manifest| {
        manifest["stateAuthority"]["writableRoots"]
            .as_array_mut()
            .expect("writableRoots should be an array")
            .push(serde_json::json!({
                "path": ".other",
                "owner": "rust",
                "access": "readWrite"
            }));
    });
    let error = load_source_authority(&second_root.root)
        .expect_err("a second writable state root must fail");
    assert!(error.to_string().contains("exactly one writable root"));
}

#[test]
fn rejects_windows_and_posix_absolute_authority_paths() {
    let root = repository_root();
    for absolute_path in [r"C:\outside\authority.mjs", "/outside/authority.mjs"] {
        let json = mutated_manifest(&root, |manifest| {
            manifest["javascriptAllowlist"][0]["path"] = Value::String(absolute_path.to_owned());
        });
        assert_rejected(&root, &json, "unsafe repository-relative path");
    }
}

#[test]
fn source_authority_gate_precedes_compat_loading_for_all_verify_commands() {
    let main_source =
        fs::read_to_string(repository_root().join("crates/compat-harness/src/main.rs"))
            .expect("compat harness main source should be readable");
    let verify_repository = main_source
        .split("fn verify_repository")
        .nth(1)
        .expect("shared repository verifier should exist");
    let authority_gate = verify_repository
        .find("validate_source_authority(root, &source_authority)")
        .expect("source authority gate should run in the shared verifier");
    let compat_load = verify_repository
        .find("match hosted_evidence_mode")
        .expect("compatibility verification should run in the shared verifier");

    assert!(
        authority_gate < compat_load,
        "source authority must be validated before compatibility manifests load"
    );
    assert!(main_source.contains(r#"command == "verify""#));
    assert!(main_source.contains("verify_repository(&root, HostedEvidenceMode::Final)"));
    assert!(main_source.contains(r#"command == "verify-strict-precondition""#));
    assert!(
        main_source.contains("verify_repository(&root, HostedEvidenceMode::CandidatePrecondition)")
    );
    assert!(main_source.contains(r#"command == "verify-candidate""#));
    assert!(main_source.contains("verify_repository(&root, HostedEvidenceMode::None)"));
}

#[test]
fn ci_keeps_rust_authority_ahead_of_packaging_and_fails_closed() {
    let source = fs::read_to_string(repository_root().join(CI_WORKFLOW))
        .expect("CI workflow should be readable");
    assert!(
        source.contains(
            "RUSTFLAGS: ${{ matrix.os == 'windows-latest' && '-C link-arg=/Brepro' || '' }}"
        ),
        "Windows MSVC hosted builds must opt into reproducible linker output"
    );
    validate_ci_workflow_text(&source).expect("committed CI workflow should preserve authority");

    let native_io_step = "      - name: Run native Shell I/O integration\n        run: cargo test -p minimax-tools --test shell_io --locked -- --nocapture\n";
    let missing_native_io = source.replace(native_io_step, "");
    assert_ci_rejected(
        &missing_native_io,
        "CI must run native Shell I/O integration on every hosted platform",
    );

    let obsolete_native_pty = source.replace(
        native_io_step,
        "      - name: Run native PTY Shell integration\n        run: cargo test -p minimax-tools --test shell_pty --locked -- --nocapture\n",
    );
    assert_ci_rejected(
        &obsolete_native_pty,
        "CI must run native Shell I/O integration on every hosted platform",
    );

    let linux_only_native_io = source.replace(
        native_io_step,
        "      - name: Run native Shell I/O integration\n        if: runner.os == 'Linux'\n        run: cargo test -p minimax-tools --test shell_io --locked -- --nocapture\n",
    );
    assert_ci_rejected(
        &linux_only_native_io,
        "CI must run native Shell I/O integration on every hosted platform",
    );

    let post_run_linux_only_native_io = source.replace(
        native_io_step,
        "      - name: Run native Shell I/O integration\n        run: cargo test -p minimax-tools --test shell_io --locked -- --nocapture\n        if: runner.os == 'Linux'\n",
    );
    assert_ci_rejected(
        &post_run_linux_only_native_io,
        "CI must run native Shell I/O integration on every hosted platform",
    );

    for forbidden_key in [
        "        \"if\": runner.os == 'Linux'\n",
        "        'if' : runner.os == 'Linux'\n",
        "        if : runner.os == 'Linux'\n",
        "        \"continue-on-error\": true\n",
        "        continue-on-error : true\n",
        "        shell: bash -c 'exit 0' -- {0}\n",
    ] {
        let disguised_native_io_control = source.replace(
            native_io_step,
            &format!(
                "      - name: Run native Shell I/O integration\n        run: cargo test -p minimax-tools --test shell_io --locked -- --nocapture\n{forbidden_key}"
            ),
        );
        assert_ci_rejected(
            &disguised_native_io_control,
            if forbidden_key.trim_start().starts_with(['\'', '"']) {
                "CI steps mapping keys must be unambiguous"
            } else if forbidden_key.trim_start().starts_with("shell") {
                "CI authority execution shell must not be overridden"
            } else {
                "CI must run native Shell I/O integration on every hosted platform"
            },
        );
    }

    let non_blocking_native_io = source.replace(
        native_io_step,
        "      - name: Run native Shell I/O integration\n        continue-on-error: true\n        run: cargo test -p minimax-tools --test shell_io --locked -- --nocapture\n",
    );
    assert_ci_rejected(
        &non_blocking_native_io,
        "CI must run native Shell I/O integration on every hosted platform",
    );

    let environment_override_native_io = source.replace(
        native_io_step,
        "      - name: Run native Shell I/O integration\n        env:\n          SHELL: forged\n        run: cargo test -p minimax-tools --test shell_io --locked -- --nocapture\n",
    );
    assert_ci_rejected(
        &environment_override_native_io,
        "CI authority steps must not inject credentials or override the job environment",
    );

    let skipped_contract = source.replace(
        "run: npm run verify:rust-contracts:strict-precondition\n",
        "run: npm run verify:rust-contracts:candidate\n",
    );
    assert_ci_rejected(
        &skipped_contract,
        "verify:rust-contracts:strict-precondition exactly once",
    );

    let package_line = r#"      - run: npm run package:rust -- --binary "target/phase14-ci/cargo/release/minimax-cli${{ runner.os == 'Windows' && '.exe' || '' }}" --output target/phase14-ci/artifacts --fingerprint-file target/phase14-ci/fingerprint.json
"#;
    let reversed = source.replace(package_line, "").replace(
        "      - run: npm ci\n",
        &format!("      - run: npm ci\n{package_line}"),
    );
    assert_ci_rejected(&reversed, "strict order");

    let package_test = "      - name: Reject corrupt release package candidates\n        run: npm run test:package\n";
    let corruption_after_build = source.replace(package_test, "").replace(
        "      - run: npm run build:rust:release\n",
        &format!("      - run: npm run build:rust:release\n{package_test}"),
    );
    assert_ci_rejected(&corruption_after_build, "strict order");

    let typescript_product = source.replace(
        "      - run: npm run check:rust\n",
        "      - run: npm run build\n      - run: npm run check:rust\n",
    );
    assert_ci_rejected(&typescript_product, "transitional TypeScript product");

    let credential = source.replace(
        package_line,
        &format!("{package_line}        env:\n          OPENAI_API_KEY: ${{{{ secrets.OPENAI_API_KEY }}}}\n"),
    );
    assert_ci_rejected(&credential, "must not inject credentials");

    let implicit_github_token = source.replace(
        "      CARGO_TARGET_DIR: target/phase14-ci/cargo\n",
        "      CARGO_TARGET_DIR: target/phase14-ci/cargo\n      FORGED_TOKEN: ${{ github.token }}\n",
    );
    assert_ci_rejected(&implicit_github_token, "must not inject credentials");

    let write_permission = source.replace("contents: read", "contents: write");
    assert_ci_rejected(&write_permission, "exactly contents: read");

    let non_blocking = source.replace(
        package_line,
        &format!("{package_line}        continue-on-error: true\n"),
    );
    assert_ci_rejected(&non_blocking, "must fail closed");

    let expanded_matrix = source.replace(
        "os: [ubuntu-latest, windows-latest]",
        "os: [ubuntu-latest, windows-latest, macos-latest]",
    );
    assert_ci_rejected(&expanded_matrix, "matrix must remain Ubuntu and Windows");

    let windows_excluded = source.replace(
        "        os: [ubuntu-latest, windows-latest]\n",
        "        os: [ubuntu-latest, windows-latest]\n        exclude:\n          - os: windows-latest\n",
    );
    assert_ci_rejected(&windows_excluded, "must not include or exclude matrix jobs");

    let matrix_included = source.replace(
        "        os: [ubuntu-latest, windows-latest]\n",
        "        os: [ubuntu-latest, windows-latest]\n        include:\n          - os: ubuntu-latest\n",
    );
    assert_ci_rejected(&matrix_included, "must not include or exclude matrix jobs");

    let conditional_job = source.replace(
        "  verify:\n    runs-on: ${{ matrix.os }}\n",
        "  verify:\n    if: runner.os == 'Linux'\n    runs-on: ${{ matrix.os }}\n",
    );
    assert_ci_rejected(
        &conditional_job,
        "Shell I/O authority job must be unconditional",
    );

    let dependent_authority_job = source.replace(
        "jobs:\n  verify:\n",
        "jobs:\n  skipped:\n    if: false\n    runs-on: ubuntu-latest\n    steps:\n      - run: /bin/true\n  verify:\n    needs: skipped\n",
    );
    assert_ci_rejected(
        &dependent_authority_job,
        "Shell I/O authority job must not depend on other jobs",
    );

    let workflow_default_shell = source.replace(
        "permissions:\n  contents: read\n",
        "defaults:\n  run:\n    shell: bash -c 'exit 0' -- {0}\n\npermissions:\n  contents: read\n",
    );
    assert_ci_rejected(
        &workflow_default_shell,
        "CI authority execution shell must not be overridden",
    );

    let job_default_shell = source.replace(
        "  verify:\n    runs-on: ${{ matrix.os }}\n",
        "  verify:\n    defaults:\n      run:\n        shell: bash -c 'exit 0' -- {0}\n    runs-on: ${{ matrix.os }}\n",
    );
    assert_ci_rejected(
        &job_default_shell,
        "CI authority execution shell must not be overridden",
    );

    let nested_runs_on = source
        .replace(
            "    runs-on: ${{ matrix.os }}\n",
            "    runs-on: ubuntu-latest\n",
        )
        .replace("    env:\n", "    env:\n      runs-on: ${{ matrix.os }}\n");
    assert_ci_rejected(
        &nested_runs_on,
        "Shell I/O authority step must remain in the authoritative matrix job",
    );

    let nested_matrix_os = source.replace(
        "        os: [ubuntu-latest, windows-latest]\n",
        "        metadata:\n          os: [ubuntu-latest, windows-latest]\n",
    );
    assert_ci_rejected(&nested_matrix_os, "matrix must remain Ubuntu and Windows");

    let conditional_check = source.replace(
        "      - run: npm run check:rust\n",
        "      - run: npm run check:rust\n        if: false\n",
    );
    assert_ci_rejected(
        &conditional_check,
        "check:rust must be an unconditional step in the authoritative matrix job",
    );

    let custom_shell_check = source.replace(
        "      - run: npm run check:rust\n",
        "      - run: npm run check:rust\n        shell: bash -c 'exit 0' -- {0}\n",
    );
    assert_ci_rejected(
        &custom_shell_check,
        "CI authority execution shell must not be overridden",
    );

    let forged_native_io = source.replace(
        native_io_step,
        "      - name: Run native Shell I/O integration\n        shell: bash -c 'exit 0' -- {0}\n        run: cargo test -p minimax-tools --test shell_io --locked -- --nocapture\n",
    );
    let release_tail_start = forged_native_io
        .find("      - name: Generate explicit release product fingerprint\n")
        .expect("release tail should exist");
    let moved_release_tail = format!(
        "{}  deferred-release:\n    runs-on: ubuntu-latest\n    steps:\n{}",
        &forged_native_io[..release_tail_start],
        &forged_native_io[release_tail_start..]
    );
    assert_ci_rejected(
        &moved_release_tail,
        "CI authority execution shell must not be overridden",
    );

    let nested_check = source.replace(
        "      - run: npm run check:rust\n",
        "      - name: Retain check command only as inert metadata\n        env:\n          COMMAND: npm run check:rust\n",
    );
    assert_ci_rejected(
        &nested_check,
        "check:rust must be an unconditional step in the authoritative matrix job",
    );

    let split_native_io_step = source.replace(
        native_io_step,
        "      - name: Run native Shell I/O integration\n        run: echo skipped\n      - run: cargo test -p minimax-tools --test shell_io --locked -- --nocapture\n",
    );
    assert_ci_rejected(
        &split_native_io_step,
        "must run native Shell I/O integration on every hosted platform",
    );

    let nested_native_io_step = source.replace(
        native_io_step,
        "      io-container:\n        - name: Run native Shell I/O integration\n          run: cargo test -p minimax-tools --test shell_io --locked -- --nocapture\n",
    );
    assert_ci_rejected(
        &nested_native_io_step,
        "must run native Shell I/O integration on every hosted platform",
    );

    let linux_only_job = source.replace(
        native_io_step,
        "  linux-only:\n    runs-on: ubuntu-latest\n    steps:\n      - name: Run native Shell I/O integration\n        run: cargo test -p minimax-tools --test shell_io --locked -- --nocapture\n",
    );
    assert_ci_rejected(
        &linux_only_job,
        "Shell I/O authority step must remain in the authoritative matrix job",
    );

    let missing_canary = source.replace(
        "run: bash scripts/ci-linux-sandbox-canary.sh",
        "run: echo skipped",
    );
    assert_ci_rejected(
        &missing_canary,
        "retain the Linux adversarial sandbox canary",
    );

    let non_reproducible_windows_link = source.replace(
        "RUSTFLAGS: ${{ matrix.os == 'windows-latest' && '-C link-arg=/Brepro' || '' }}",
        "RUSTFLAGS: ''",
    );
    assert_ci_rejected(
        &non_reproducible_windows_link,
        "reproducible /Brepro linking",
    );

    let implicit_fingerprint =
        source.replace(" --fingerprint-file target/phase14-ci/fingerprint.json", "");
    assert_ci_rejected(&implicit_fingerprint, "package:rust");

    let upload = "        uses: actions/upload-artifact@v4\n";
    let milestone = "      - run: npm run verify:milestone-flow -- --artifacts target/phase14-ci/artifacts --evidence-dir target/phase14-ci/evidence --fingerprint-file target/phase14-ci/fingerprint.json\n";
    let upload_before_milestone = source
        .replace(upload, "")
        .replace(milestone, &format!("{upload}{milestone}"));
    assert_ci_rejected(&upload_before_milestone, "upload must follow");

    let candidate_only_upload = source.replace(
        "      - name: Upload hosted release evidence\n        uses: actions/upload-artifact@v4\n",
        "      - name: Upload hosted release evidence\n        if: github.event_name == 'workflow_dispatch'\n        uses: actions/upload-artifact@v4\n",
    );
    assert_ci_rejected(&candidate_only_upload, "candidate and strict runs");
}

fn assert_ci_rejected(source: &str, expected: &str) {
    let error = validate_ci_workflow_text(source).expect_err("CI mutation must fail");
    assert!(
        error.to_string().contains(expected),
        "expected {expected:?} in {error:?}"
    );
}

#[test]
fn ci_rejects_non_executing_authority_text_forgery() {
    let source = fs::read_to_string(repository_root().join(CI_WORKFLOW))
        .expect("CI workflow should be readable");
    let reproducible_msvc_link =
        "RUSTFLAGS: ${{ matrix.os == 'windows-latest' && '-C link-arg=/Brepro' || '' }}";
    let commented_rustflags = source.replace(
        &format!("      {reproducible_msvc_link}\n"),
        &format!("      RUSTFLAGS: ''\n      # {reproducible_msvc_link}\n"),
    );
    let scalar_rustflags = source.replace(
        &format!("      {reproducible_msvc_link}\n"),
        &format!("      RUSTFLAGS: ''\n      PROOF: |\n        {reproducible_msvc_link}\n"),
    );
    let step_rustflags_override = source.replace(
        "      - run: npm run build:rust:release\n",
        "      - run: npm run build:rust:release\n        env:\n          RUSTFLAGS: ''\n",
    );
    let conditional_canary = source.replace(
        "      - name: Run Linux adversarial sandbox canary\n        if: runner.os == 'Linux'\n        run: bash scripts/ci-linux-sandbox-canary.sh\n",
        "      - name: Run Linux adversarial sandbox canary\n        if: false\n        run: bash scripts/ci-linux-sandbox-canary.sh\n",
    );
    let conditional_upload = source.replace(
        "          retention-days: 7\n",
        "          retention-days: 7\n        if: false\n",
    );
    let scalar_provider_command = source.replace(
        "      - name: Run Rust Provider evaluation\n        run: npm run eval:provider\n",
        "      - name: Run Rust Provider evaluation\n        if: false\n        run: |\n          run: npm run eval:provider\n",
    );
    let conditional_provider_command = source.replace(
        "      - name: Run Rust Provider evaluation\n        run: npm run eval:provider\n",
        "      - name: Run Rust Provider evaluation\n        if: false\n        run: npm run eval:provider\n",
    );

    let mutations = [
        (
            "commented RUSTFLAGS",
            commented_rustflags,
            "reproducible /Brepro linking",
        ),
        (
            "block scalar RUSTFLAGS",
            scalar_rustflags,
            "reproducible /Brepro linking",
        ),
        (
            "step RUSTFLAGS override",
            step_rustflags_override,
            "must not inject credentials or override the job environment",
        ),
        (
            "conditional Linux canary",
            conditional_canary,
            "Linux adversarial sandbox canary",
        ),
        (
            "conditional evidence upload",
            conditional_upload,
            "hosted evidence upload",
        ),
        (
            "block scalar required command",
            scalar_provider_command,
            "required commands must use executable one-line run mappings",
        ),
        (
            "conditional required command",
            conditional_provider_command,
            "unconditional required commands must not be skipped",
        ),
    ];
    let mut accepted = Vec::new();
    for (label, mutation, expected) in mutations {
        match validate_ci_workflow_text(&mutation) {
            Ok(()) => accepted.push(label),
            Err(error) => assert!(
                error.to_string().contains(expected),
                "{label}: expected {expected:?} in {error:?}"
            ),
        }
    }
    assert!(accepted.is_empty(), "accepted CI forgeries: {accepted:?}");
}

#[test]
fn ci_rejects_escaped_keys_in_jobs_and_job_mappings() {
    let source = fs::read_to_string(repository_root().join(CI_WORKFLOW))
        .expect("CI workflow should be readable");
    let escaped_job = source.replace(
        "jobs:\n",
        "jobs:\n  \"ev\\u0061l\":\n    runs-on: ubuntu-latest\n",
    );
    assert_ci_rejected(&escaped_job, "jobs mapping keys must be unambiguous");

    let escaped_if = source.replace(
        "    runs-on: ${{ matrix.os }}\n",
        "    \"\\u0069f\": false\n    runs-on: ${{ matrix.os }}\n",
    );
    assert_ci_rejected(&escaped_if, "job mapping keys must be unambiguous");
}

#[test]
fn ci_rejects_escaped_keys_in_strategy_and_matrix_mappings() {
    let source = fs::read_to_string(repository_root().join(CI_WORKFLOW))
        .expect("CI workflow should be readable");
    let escaped_strategy_key = source.replace(
        "    strategy:\n",
        "    strategy:\n      \"met\\u0061data\": true\n",
    );
    assert_ci_rejected(
        &escaped_strategy_key,
        "strategy mapping keys must be unambiguous",
    );

    for escaped_key in ["\\u0069nclude", "\\u0065xclude"] {
        let escaped_matrix_key = source.replace(
            "        os: [ubuntu-latest, windows-latest]\n",
            &format!(
                "        os: [ubuntu-latest, windows-latest]\n        \"{escaped_key}\":\n          - os: windows-latest\n"
            ),
        );
        assert_ci_rejected(
            &escaped_matrix_key,
            "matrix mapping keys must be unambiguous",
        );
    }
}

#[test]
fn ci_rejects_escaped_keys_in_steps_mapping() {
    let source = fs::read_to_string(repository_root().join(CI_WORKFLOW))
        .expect("CI workflow should be readable");
    let escaped_step = source.replace(
        "      - run: npm run check:rust\n",
        "      - run: npm run check:rust\n      - \"r\\u0075n\": echo forged\n",
    );
    assert_ci_rejected(&escaped_step, "steps mapping keys must be unambiguous");

    let escaped_check_if = source.replace(
        "      - run: npm run check:rust\n",
        "      - run: npm run check:rust\n        \"\\u0069f\": false\n",
    );
    assert_ci_rejected(&escaped_check_if, "steps mapping keys must be unambiguous");

    let escaped_native_io_if = source.replace(
        "      - name: Run native Shell I/O integration\n",
        "      - name: Run native Shell I/O integration\n        \"\\u0069f\": false\n",
    );
    assert_ci_rejected(
        &escaped_native_io_if,
        "steps mapping keys must be unambiguous",
    );
}

#[test]
fn ci_rejects_escaped_keys_in_top_level_mapping() {
    let source = fs::read_to_string(repository_root().join(CI_WORKFLOW))
        .expect("CI workflow should be readable");
    let escaped_permissions = source.replace(
        "permissions:\n  contents: read\n",
        "permissions:\n  contents: read\n\"\\u0070ermissions\": write-all\n",
    );
    assert_ci_rejected(
        &escaped_permissions,
        "top-level mapping keys must be unambiguous",
    );

    let escaped_jobs = format!("{source}\n\"j\\u006fbs\": {{}}\n");
    assert_ci_rejected(&escaped_jobs, "top-level mapping keys must be unambiguous");
}

#[test]
fn ci_rejects_duplicate_authority_mapping_keys() {
    let source = fs::read_to_string(repository_root().join(CI_WORKFLOW))
        .expect("CI workflow should be readable");
    let duplicate_runs_on = source.replace(
        "    runs-on: ${{ matrix.os }}\n",
        "    runs-on: ${{ matrix.os }}\n    runs-on: ubuntu-latest\n",
    );
    assert_ci_rejected(&duplicate_runs_on, "job mapping keys must be unambiguous");

    let duplicate_step_name = source.replace(
        "      - name: Run native Shell I/O integration\n",
        "      - name: Run native Shell I/O integration\n        name: Forged duplicate\n",
    );
    assert_ci_rejected(
        &duplicate_step_name,
        "steps mapping keys must be unambiguous",
    );
}

#[test]
fn ci_rejects_yaml_merge_anchor_alias_and_tag_authority() {
    let source = fs::read_to_string(repository_root().join(CI_WORKFLOW))
        .expect("CI workflow should be readable");
    let merge_key = source.replace(
        "    runs-on: ${{ matrix.os }}\n",
        "    <<: {if: false}\n    runs-on: ${{ matrix.os }}\n",
    );
    assert_ci_rejected(&merge_key, "job mapping keys must be unambiguous");

    let anchored_strategy = source.replace("    strategy:\n", "    strategy: &authority\n");
    assert_ci_rejected(&anchored_strategy, "job mapping keys must be unambiguous");

    let anchored_if = source.replace(
        "    runs-on: ${{ matrix.os }}\n",
        "    &condition if: false\n    runs-on: ${{ matrix.os }}\n",
    );
    assert_ci_rejected(&anchored_if, "job mapping keys must be unambiguous");

    let tagged_include = source.replace(
        "        os: [ubuntu-latest, windows-latest]\n",
        "        os: [ubuntu-latest, windows-latest]\n        !!str include:\n          - os: windows-latest\n",
    );
    assert_ci_rejected(&tagged_include, "matrix mapping keys must be unambiguous");

    let alias_strategy = source.replace("    strategy:\n", "    strategy: *authority\n");
    assert_ci_rejected(&alias_strategy, "job mapping keys must be unambiguous");
}

#[test]
fn ci_rejects_permissions_and_secrets_on_every_job() {
    let source = fs::read_to_string(repository_root().join(CI_WORKFLOW))
        .expect("CI workflow should be readable");
    let job_permissions = format!(
        "{source}\n  privileged:\n    runs-on: ubuntu-latest\n    permissions: write-all\n    steps:\n      - run: echo forged\n"
    );
    assert_ci_rejected(
        &job_permissions,
        "CI jobs must not override permissions or inherit secrets",
    );

    let escaped_job_permissions = format!(
        "{source}\n  privileged:\n    runs-on: ubuntu-latest\n    \"\\u0070ermissions\": write-all\n    steps:\n      - run: echo forged\n"
    );
    assert_ci_rejected(
        &escaped_job_permissions,
        "job mapping keys must be unambiguous",
    );

    let inherited_secrets = format!(
        "{source}\n  reusable:\n    uses: owner/repository/.github/workflows/reusable.yml@main\n    secrets: inherit\n"
    );
    assert_ci_rejected(
        &inherited_secrets,
        "CI jobs must not override permissions or inherit secrets",
    );
}

#[test]
fn ci_rejects_flow_style_jobs() {
    let source = fs::read_to_string(repository_root().join(CI_WORKFLOW))
        .expect("CI workflow should be readable");
    let flow_job = format!(
        "{source}\n  privileged: {{runs-on: ubuntu-latest, permissions: write-all, steps: []}}\n"
    );

    assert_ci_rejected(&flow_job, "CI jobs must use block mappings");
}

#[test]
fn ci_rejects_quoted_and_escaped_job_control_keys() {
    let source = fs::read_to_string(repository_root().join(CI_WORKFLOW))
        .expect("CI workflow should be readable");
    for key in [
        "\"continue-on-error\"",
        "\"permissions\"",
        "\"secrets\"",
        "\"continue-on-\\u0065rror\"",
        "\"p\\u0065rmissions\"",
        "\"s\\u0065crets\"",
    ] {
        let mutation = format!(
            "{source}\n  privileged:\n    runs-on: ubuntu-latest\n    {key}: inherit\n    steps:\n      - run: echo forged\n"
        );
        assert_ci_rejected(&mutation, "job mapping keys must be unambiguous");
    }
}

#[test]
fn ci_rejects_bracketed_and_whitespace_secret_expressions() {
    let source = fs::read_to_string(repository_root().join(CI_WORKFLOW))
        .expect("CI workflow should be readable");
    for expression in [
        "${{ secrets['TOKEN'] }}",
        "${{ secrets [ 'TOKEN' ] }}",
        "${{ SECRETS[\"TOKEN\"] }}",
    ] {
        let mutation = source.replace(
            "      CARGO_TARGET_DIR: target/phase14-ci/cargo\n",
            &format!(
                "      CARGO_TARGET_DIR: target/phase14-ci/cargo\n      FORGED_TOKEN: {expression}\n"
            ),
        );
        assert_ci_rejected(
            &mutation,
            "CI must not inject credentials into authority or package gates",
        );
    }
}

#[test]
fn npm_release_workflow_is_tag_only_ordered_and_secret_isolated() {
    let source = fs::read_to_string(repository_root().join(NPM_RELEASE_WORKFLOW))
        .expect("npm release workflow should be readable");
    validate_npm_release_workflow_text(&source)
        .expect("committed npm release workflow should preserve publication authority");

    for (label, mutation, expected) in [
        (
            "manual trigger",
            source.replace("on:\n  push:", "on:\n  workflow_dispatch:\n  push:"),
            "triggered only by v* tag pushes",
        ),
        (
            "untrusted ancestry",
            source.replace(
                "git merge-base --is-ancestor \"$GITHUB_SHA\" origin/main",
                "echo skipped-main-ancestry",
            ),
            "preflight must retain git merge-base",
        ),
        (
            "ambiguous registry error",
            source.replace("E404|404 Not Found", "any-error"),
            "preflight must retain grep -Eq 'E404|404 Not Found'",
        ),
        (
            "publish before smoke",
            source.replace(
                "  publish:\n    needs: [preflight, assemble, smoke]",
                "  publish:\n    needs: [preflight, assemble]",
            ),
            "preflight, build, assemble, smoke, then publish",
        ),
        (
            "stale hosted evidence accepted",
            source.replace("      - run: npm run verify:rust-contracts\n", ""),
            "build must retain npm run verify:rust-contracts",
        ),
        (
            "consumer Rust install",
            source.replace(
                "      - name: Smoke global and project-local installs without lifecycle scripts",
                "      - run: rustup toolchain install 1.97.0\n      - name: Smoke global and project-local installs without lifecycle scripts",
            ),
            "consumer smoke must not build or publish",
        ),
        (
            "lifecycle execution",
            source.replace(
                "npm install --global --ignore-scripts --prefix",
                "npm install --global --prefix",
            ),
            "smoke must retain npm install --global --ignore-scripts",
        ),
        (
            "unprotected publish",
            source.replace("    environment: npm-production\n", ""),
            "permissions must be read-only except publish OIDC",
        ),
        (
            "broad OIDC",
            source.replace("      id-token: write", "      id-token: read"),
            "permissions must be read-only except publish OIDC",
        ),
        (
            "non-blocking release gate",
            source.replace(
                "    timeout-minutes: 10\n    outputs:",
                "    timeout-minutes: 10\n    continue-on-error: true\n    outputs:",
            ),
            "gates must fail closed",
        ),
        (
            "unpinned npm client",
            source.replace(
                "npm install --global npm@11.5.1",
                "npm install --global npm@latest",
            ),
            "publish must retain npm install --global npm@11.5.1",
        ),
        (
            "manifest digest bypass",
            source.replace(
                "archiveSha256 !== manifest.npmPackage.sha256",
                "archiveSha256.length === 0",
            ),
            "publish must retain archiveSha256 !== manifest.npmPackage.sha256",
        ),
        (
            "rebuilt publish artifact",
            source.replace(
                "npm publish \"$ARCHIVE\" --access public --provenance",
                "npm publish \"$ARCHIVE.rebuilt\" --access public --provenance",
            ),
            "publish must retain npm publish \"$ARCHIVE\" --access public --provenance",
        ),
        (
            "missing dry run",
            source.replace(
                "npm publish \"$ARCHIVE\" --dry-run --json --access public",
                "echo skipped-dry-run",
            ),
            "publish must retain npm publish \"$ARCHIVE\" --dry-run --json --access public",
        ),
        (
            "early credential",
            source.replace(
                "      - name: Validate tag, versions, main ancestry, and npm availability",
                "      - name: Expose secret\n        run: echo ${{ secrets.NPM_TOKEN }}\n      - name: Validate tag, versions, main ancestry, and npm availability",
            ),
            "credentials and publication must exist only in the final publish job",
        ),
    ] {
        assert_npm_release_rejected(&mutation, expected, label);
    }
}

fn assert_npm_release_rejected(source: &str, expected: &str, label: &str) {
    let error = validate_npm_release_workflow_text(source)
        .expect_err("npm release workflow mutation must fail");
    assert!(
        error.to_string().contains(expected),
        "{label}: expected {expected:?} in {error:?}"
    );
}
