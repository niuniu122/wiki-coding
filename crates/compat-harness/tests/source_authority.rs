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
    SourceAuthorityError, load_source_authority, parse_source_authority, validate_ci_workflow_text,
    validate_source_authority,
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
            r#"{
  "bin": {"minimax-codex": "bin/minimax-codex.cjs"},
  "scripts": {
    "dev": "cargo run -p minimax-cli --locked --",
    "start": "node bin/minimax-codex.cjs",
    "build": "tsc -p tsconfig.json",
    "check": "tsc -p tsconfig.json --noEmit",
    "test": "tsx test/run-tests.ts",
    "test:launcher": "tsx --test test/launcher.test.ts",
    "eval:retrieval": "cargo run -p minimax-compat-harness --locked -- retrieval-eval --format json",
    "eval:provider": "cargo run -p minimax-compat-harness --locked -- provider-eval --format json",
    "verify:rust-contracts": "cargo run -p minimax-compat-harness --locked -- verify",
    "verify:agent": "npm run verify:rust-contracts && npm run eval:provider && npm run eval:retrieval",
    "verify:release": "npm run check && npm test && npm run check:rust && npm run test:rust && npm run verify:agent && npm run build && npm run build:rust:release && npm run package:rust && npm run verify:rust-release && npm run verify:milestone-flow",
    "smoke:provider": "tsx src/smoke/provider-smoke.ts"
  }
}"#,
        );
        for path in [
            "bin/minimax-codex.cjs",
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
        write_file(&root, "src/legacy.ts", "export {};\n");
        for path in [
            "test/fixtures/executors/diag-large.js",
            "test/fixtures/executors/diag-ok.js",
            "test/fixtures/executors/diag-slow.js",
        ] {
            write_file(&root, path, "process.stdout.write(\"diagnostic\");\n");
        }

        let committed = manifest_json(&repository_root());
        let mut manifest: Value =
            serde_json::from_str(&committed).expect("committed manifest should parse");
        manifest["transitionalTypeScript"]["entries"] = serde_json::json!([{
            "path": "src/legacy.ts",
            "sha256": sha256_file(&root.join("src/legacy.ts")),
            "purpose": "inertShrinkingEvidence"
        }]);
        for class in ["javascriptAllowlist", "transitionalLegacyTestFixtures"] {
            let entries = if class == "javascriptAllowlist" {
                manifest[class]
                    .as_array_mut()
                    .expect("JavaScript allowlist should be an array")
            } else {
                manifest[class]["entries"]
                    .as_array_mut()
                    .expect("legacy fixtures should be an array")
            };
            for entry in entries {
                let path = entry["path"]
                    .as_str()
                    .expect("authority entry should contain a path");
                entry["sha256"] = Value::String(sha256_file(&root.join(path)));
            }
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

    fn replace_typescript_sources(&self, sources: &[(&str, &str)]) {
        for (path, contents) in sources {
            write_file(&self.root, path, contents);
        }
        let mut manifest: Value = serde_json::from_str(&manifest_json(&self.root))
            .expect("synthetic manifest should parse");
        let mut entries = sources
            .iter()
            .map(|(path, _)| {
                serde_json::json!({
                    "path": path,
                    "sha256": sha256_file(&self.root.join(path)),
                    "purpose": "inertShrinkingEvidence"
                })
            })
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| {
            left["path"]
                .as_str()
                .expect("entry path should be text")
                .cmp(right["path"].as_str().expect("entry path should be text"))
        });
        manifest["transitionalTypeScript"]["entries"] = Value::Array(entries);
        write_manifest(&self.root, &manifest);
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
    assert_eq!(
        scripts.get("dev").and_then(Value::as_str),
        Some("cargo run -p minimax-cli --locked --"),
        "package dev route must execute the Rust CLI source with npm argv forwarding"
    );
    assert_eq!(
        scripts.get("start").and_then(Value::as_str),
        Some("node bin/minimax-codex.cjs"),
        "package start route must remain the thin Rust launcher"
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
            "TypeScript/legacy product entry",
        );
    }
}

#[test]
fn transitional_tests_static_checks_build_and_smoke_remain_allowed() {
    let repository = SyntheticRepository::new();
    let manifest = repository.load();
    validate_source_authority(&repository.root, &manifest)
        .expect("tests, static checks, build, and smoke remain transitional through Phase 11");

    let scripts = package_scripts(&repository.root);
    for (name, expected) in [
        ("build", "tsc -p tsconfig.json"),
        ("check", "tsc -p tsconfig.json --noEmit"),
        ("test", "tsx test/run-tests.ts"),
        ("test:launcher", "tsx --test test/launcher.test.ts"),
        ("smoke:provider", "tsx src/smoke/provider-smoke.ts"),
    ] {
        assert_eq!(scripts.get(name).and_then(Value::as_str), Some(expected));
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
    let root = repository_root();
    let current_test_hash = sha256_file(&root.join("test/test-discovery.test.ts"));
    let current_manifest = mutated_manifest(&root, |manifest| {
        let entry = manifest["transitionalTypeScript"]["entries"]
            .as_array_mut()
            .expect("TypeScript authority entries should be an array")
            .iter_mut()
            .find(|entry| entry["path"] == "test/test-discovery.test.ts")
            .expect("test discovery regression source should be classified");
        entry["sha256"] = Value::String(current_test_hash);
    });
    let manifest = parse_source_authority(&root, &current_manifest)
        .expect("the RED test hash adjustment should preserve source authority shape");
    validate_source_authority(&root, &manifest).expect_err(
        "discovered test/provider-conformance.test.ts and \
         test/capability-retrieval-report.test.ts reach forbidden src/eval TypeScript evaluators",
    );

    let cases: &[(&str, &[(&str, &str)])] = &[
        (
            "direct static import with emitted JavaScript mapping",
            &[
                (
                    "test/direct.test.ts",
                    "import '../src/eval/provider-conformance.js';\n",
                ),
                (
                    "src/eval/provider-conformance.ts",
                    "export const evaluator = true;\n",
                ),
            ],
        ),
        (
            "indirect import with normalized dot segments",
            &[
                ("test/indirect.test.ts", "import './support/./bridge.js';\n"),
                (
                    "test/support/bridge.ts",
                    "import '../../src/other/../eval/capability-retrieval-report.js';\n",
                ),
                (
                    "src/eval/capability-retrieval-report.ts",
                    "export const evaluator = true;\n",
                ),
            ],
        ),
        (
            "re-export from TypeScript evaluator",
            &[
                (
                    "test/reexport.test.ts",
                    "export {runProviderConformanceReport} from '../src/eval/provider-conformance.js';\n",
                ),
                (
                    "src/eval/provider-conformance.ts",
                    "export const runProviderConformanceReport = true;\n",
                ),
            ],
        ),
        (
            "literal dynamic import through a cycle",
            &[
                ("test/dynamic.test.ts", "await import('./cycle-a.js');\n"),
                ("test/cycle-a.ts", "export * from './cycle-b.js';\n"),
                (
                    "test/cycle-b.ts",
                    "import './cycle-a.js';\nawait import('../src/eval/provider-conformance.js');\n",
                ),
                (
                    "src/eval/provider-conformance.ts",
                    "export const evaluator = true;\n",
                ),
            ],
        ),
        (
            "Windows separator resolving to TSX",
            &[
                (
                    "test/windows.test.ts",
                    "import '..\\\\src\\\\eval\\\\provider-conformance.js';\n",
                ),
                (
                    "src/eval/provider-conformance.tsx",
                    "export const evaluator = true;\n",
                ),
            ],
        ),
    ];
    for (label, sources) in cases {
        let repository = SyntheticRepository::new();
        repository.replace_typescript_sources(sources);
        let manifest = repository.load();
        let error = validate_source_authority(&repository.root, &manifest)
            .expect_err("every transitive evaluator route must fail closed");
        assert!(
            error.to_string().contains("TypeScript evaluator"),
            "{label}: expected evaluator reachability rejection, got {error:?}"
        );
    }
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
            "evaluation authority",
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
fn rejects_legacy_fixture_smuggling_and_second_writable_root() {
    let smuggled = SyntheticRepository::new();
    smuggled.mutate_manifest(|manifest| {
        let fixture_path = manifest["transitionalLegacyTestFixtures"]["entries"][0]["path"].clone();
        let fixture_hash =
            manifest["transitionalLegacyTestFixtures"]["entries"][0]["sha256"].clone();
        manifest["javascriptAllowlist"][0]["path"] = fixture_path;
        manifest["javascriptAllowlist"][0]["sha256"] = fixture_hash;
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
    for absolute_path in [r"C:\outside\authority.ts", "/outside/authority.ts"] {
        let json = mutated_manifest(&root, |manifest| {
            manifest["transitionalTypeScript"]["entries"][0]["path"] =
                Value::String(absolute_path.to_owned());
        });
        assert_rejected(&root, &json, "unsafe repository-relative path");
    }
}

#[test]
fn source_authority_gate_precedes_compat_loading_for_both_verify_commands() {
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
        .find("load_compat_manifests(root)")
        .expect("compat manifests should load in the shared verifier");

    assert!(
        authority_gate < compat_load,
        "source authority must be validated before compatibility manifests load"
    );
    assert!(main_source.contains(r#"command == "verify""#));
    assert!(main_source.contains("verify_repository(&root, true)"));
    assert!(main_source.contains(r#"command == "verify-candidate""#));
    assert!(main_source.contains("verify_repository(&root, false)"));
}

#[test]
fn ci_keeps_rust_authority_ahead_of_packaging_and_fails_closed() {
    let source = fs::read_to_string(repository_root().join(CI_WORKFLOW))
        .expect("CI workflow should be readable");
    validate_ci_workflow_text(&source).expect("committed CI workflow should preserve authority");

    let skipped_contract = source.replace(
        "run: npm run verify:rust-contracts\n",
        "run: npm run check\n",
    );
    assert_ci_rejected(&skipped_contract, "verify:rust-contracts exactly once");

    let package_line = "      - run: npm run package:rust\n";
    let reversed = source.replace(package_line, "").replace(
        "      - run: npm ci\n",
        &format!("      - run: npm ci\n{package_line}"),
    );
    assert_ci_rejected(
        &reversed,
        "coverage, Provider, and retrieval evaluation must pass before build, package, and evidence",
    );

    let provider =
        "      - name: Run Rust Provider evaluation\n        run: npm run eval:provider\n";
    let retrieval =
        "      - name: Run Rust retrieval evaluation\n        run: npm run eval:retrieval\n";
    let evaluations_after_package = source.replace(provider, "").replace(retrieval, "").replace(
        "      - run: npm run verify:rust-release\n",
        &format!("      - run: npm run verify:rust-release\n{provider}{retrieval}"),
    );
    assert_ci_rejected(
        &evaluations_after_package,
        "coverage, Provider, and retrieval evaluation must pass before build, package, and evidence",
    );

    let typescript_product = source.replace(
        "      - name: Run transitional TypeScript static checks\n",
        "      - run: npm run build\n      - name: Run transitional TypeScript static checks\n",
    );
    assert_ci_rejected(&typescript_product, "transitional TypeScript product");

    let credential = source.replace(
        "      - run: npm run package:rust\n",
        "      - run: npm run package:rust\n        env:\n          OPENAI_API_KEY: ${{ secrets.OPENAI_API_KEY }}\n",
    );
    assert_ci_rejected(&credential, "must not inject credentials");

    let write_permission = source.replace("contents: read", "contents: write");
    assert_ci_rejected(&write_permission, "exactly contents: read");

    let non_blocking = source.replace(
        "      - run: npm run package:rust\n",
        "      - run: npm run package:rust\n        continue-on-error: true\n",
    );
    assert_ci_rejected(&non_blocking, "must fail closed");

    let expanded_matrix = source.replace(
        "os: [ubuntu-latest, windows-latest]",
        "os: [ubuntu-latest, windows-latest, macos-latest]",
    );
    assert_ci_rejected(&expanded_matrix, "matrix must remain Ubuntu and Windows");

    let missing_canary = source.replace(
        "run: bash scripts/ci-linux-sandbox-canary.sh",
        "run: echo skipped",
    );
    assert_ci_rejected(
        &missing_canary,
        "retain the Linux adversarial sandbox canary",
    );
}

fn assert_ci_rejected(source: &str, expected: &str) {
    let error = validate_ci_workflow_text(source).expect_err("CI mutation must fail");
    assert!(
        error.to_string().contains(expected),
        "expected {expected:?} in {error:?}"
    );
}
