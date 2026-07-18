use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use minimax_compat_harness::{
    ArchitectureError, ArchitectureGraph, ArchitecturePackage, BaselineError, ManifestError,
    ParityStatus, build_report, compute_product_fingerprint, load_cargo_architecture,
    load_compat_manifests, report_json, repository_root, validate_architecture,
    validate_cli_tui_markdown_boundary, validate_core_source_boundary,
    validate_core_source_directory, validate_core_source_text, validate_cutover_candidate,
    validate_cutover_evidence, validate_hosted_candidate_gate,
    validate_hosted_candidate_gate_document, validate_hosted_release_gate_document,
    validate_migration_source_boundary, validate_migration_source_text, validate_product_entry,
    validate_report, validate_retrieval_source_boundary, validate_retrieval_source_text,
    validate_rust_command_surface, validate_rust_provider_profiles,
    validate_rust_retrieval_evidence, validate_rust_tool_evidence, validate_rust_vault_evidence,
    validate_ui_source_text, validate_vault_source_boundary, validate_vault_source_text,
    verify_fixture_compatibility,
};

#[test]
fn compat_report_matches_golden_and_is_byte_identical_on_second_run() {
    let root = repository_root();
    let first_manifests = load_compat_manifests(&root).expect("strict manifests");
    let second_manifests = load_compat_manifests(&root).expect("strict manifests on second load");
    let first = build_report(&first_manifests, &root).expect("first report");
    let second = build_report(&second_manifests, &root).expect("second report");
    validate_report(&first, &first_manifests, &root).expect("valid report");
    validate_report(&second, &second_manifests, &root).expect("valid report on second run");

    assert_eq!(first.contract_version, "v1");
    assert_eq!(
        first.contract_fingerprint,
        first_manifests.public_contract.content_fingerprint
    );

    let first_json = report_json(&first).expect("first JSON");
    let second_json = report_json(&second).expect("second JSON");
    assert_eq!(first_json, second_json);
    let expected = fs::read_to_string(root.join("fixtures/compat/report.expected.json"))
        .expect("golden report");
    assert_eq!(first_json, normalize_golden_newlines(&expected));
}

#[test]
fn compat_report_golden_accepts_windows_checkout_newlines() {
    assert_eq!(
        normalize_golden_newlines("{\r\n  \"status\": \"matched\"\r\n}\r\n"),
        "{\n  \"status\": \"matched\"\n}\n"
    );
}

#[test]
fn compat_report_contains_every_contract_item_exactly_once() {
    let root = repository_root();
    let manifests = load_compat_manifests(&root).expect("strict manifests");
    let report = build_report(&manifests, &root).expect("contract report");
    let expected_ids = manifests
        .public_contract
        .items
        .iter()
        .map(|item| item.id.clone())
        .collect::<BTreeSet<_>>();
    let report_ids = report
        .entries
        .iter()
        .map(|item| item.id.clone())
        .collect::<BTreeSet<_>>();

    assert_eq!(report.entries.len(), expected_ids.len());
    assert_eq!(report_ids, expected_ids);
    assert_eq!(manifests.public_contract.contract_version, "v1");
    assert!(
        report
            .entries
            .iter()
            .all(|entry| entry.id.starts_with("contract."))
    );
    assert!(
        report
            .entries
            .iter()
            .all(|entry| !entry.id.starts_with("typescript."))
    );
    assert_eq!(manifests.commands.commands.len(), 17);
    assert_eq!(manifests.providers.profile_classes.len(), 3);
}

#[test]
fn public_contract_manifest_fails_closed_on_schema_identity_and_evidence_drift() {
    let root = repository_root();

    let unknown = CompatManifestFixture::new(&root);
    unknown.rewrite_contract(|contract| {
        contract["unexpected"] = serde_json::json!(true);
    });
    assert!(load_compat_manifests(&unknown.root).is_err());

    let fingerprint = CompatManifestFixture::new(&root);
    fingerprint.rewrite_contract(|contract| {
        contract["contentFingerprint"] = serde_json::json!(format!("sha256:{}", "0".repeat(64)));
    });
    assert!(load_compat_manifests(&fingerprint.root).is_err());

    let duplicate = CompatManifestFixture::new(&root);
    duplicate.rewrite_contract(|contract| {
        let items = contract["items"].as_array_mut().expect("contract items");
        items.push(items[0].clone());
    });
    assert!(load_compat_manifests(&duplicate.root).is_err());

    let lost = CompatManifestFixture::new(&root);
    lost.rewrite_contract(|contract| {
        contract["items"]
            .as_array_mut()
            .expect("contract items")
            .pop();
    });
    assert!(load_compat_manifests(&lost.root).is_err());

    let missing_evidence = CompatManifestFixture::new(&root);
    fs::remove_file(missing_evidence.root.join("crates/cli/src/migration.rs"))
        .expect("remove fixture evidence");
    assert!(load_compat_manifests(&missing_evidence.root).is_err());
}

#[test]
fn compatibility_report_and_verify_are_hermetic_without_typescript_runtime() {
    let repository = repository_root();
    let fixture = HermeticCompatibilityFixture::new(&repository);
    assert!(!fixture.root.join("src").exists());
    assert!(!fixture.root.join("dist").exists());
    assert!(!fixture.root.join("test").exists());

    verify_fixture_compatibility(&fixture.root, false).expect("fixture-only Rust verification");

    let manifests = load_compat_manifests(&fixture.root).expect("hermetic manifests");
    let report = build_report(&manifests, &fixture.root).expect("hermetic report");
    validate_report(&report, &manifests, &fixture.root).expect("hermetic report validation");
}

#[test]
fn compatibility_rejects_unknown_differences_live_rows_and_typescript_execution_links() {
    let repository = repository_root();

    let unknown_difference = HermeticCompatibilityFixture::new(&repository);
    unknown_difference.rewrite_json("fixtures/compat/command-differences.v1.json", |fixture| {
        let differences = fixture["differences"]
            .as_array_mut()
            .expect("command differences");
        let mut unknown = differences[0].clone();
        unknown["id"] = serde_json::json!("difference.command.unknown");
        unknown["command"] = serde_json::json!("/unknown");
        differences.push(unknown);
    });
    let manifests = load_compat_manifests(&unknown_difference.root).expect("strict manifests");
    assert!(build_report(&manifests, &unknown_difference.root).is_err());

    let live_row = HermeticCompatibilityFixture::new(&repository);
    let manifests = load_compat_manifests(&live_row.root).expect("strict manifests");
    let mut report = build_report(&manifests, &live_row.root).expect("contract report");
    let mut row = report.entries[0].clone();
    row.id = "typescript.product_entry".to_owned();
    report.entries.push(row);
    assert!(validate_report(&report, &manifests, &live_row.root).is_err());

    for forbidden in [
        "\nfn legacy_source_read() { let _ = std::fs::read_to_string(\"src/cli.tsx\"); }\n",
        "\nfn legacy_build() { let _ = std::process::Command::new(\"npm\").args([\"run\", \"build\"]); }\n",
        "\nfn legacy_process() { let _ = std::process::Command::new(\"node\").arg(\"dist/cli.js\"); }\n",
    ] {
        let fixture = HermeticCompatibilityFixture::new(&repository);
        fixture.append_compat_source("crates/compat-harness/src/report.rs", forbidden);
        assert!(verify_fixture_compatibility(&fixture.root, false).is_err());
    }
}

#[test]
fn compatibility_source_boundary_rejects_forbidden_references_in_derived_module_closure() {
    let repository = repository_root();

    let control = HermeticCompatibilityFixture::new(&repository);
    verify_fixture_compatibility(&control.root, false)
        .expect("static compatibility authority literals remain inert");

    for (relative, source, reference_class) in [
        (
            "crates/compat-harness/src/provider_eval.rs",
            r#"
#[allow(dead_code)]
fn compatibility_boundary_node_process_probe() {
    let _command = std::process::Command::new("node").arg("dist/cli.js");
}
"#,
            "process",
        ),
        (
            "crates/compat-harness/src/migration_support.rs",
            r#"
#[allow(dead_code)]
fn compatibility_boundary_typescript_read_probe() {
    let _source = std::fs::read_to_string("src/cli.tsx");
}
"#,
            "source",
        ),
        (
            "crates/compat-harness/src/migration_support.rs",
            r#"
#[allow(dead_code)]
fn compatibility_boundary_typescript_build_probe() {
    let _command = std::process::Command::new("npm").args(["run", "build"]);
}
"#,
            "process",
        ),
        (
            "crates/compat-harness/src/provider_eval.rs",
            r#"
const COMPATIBILITY_BOUNDARY_RUNTIME: &str = concat!("no", "de");
#[allow(dead_code)]
fn compatibility_boundary_constant_process_probe() {
    let _command = std::process::Command::new(COMPATIBILITY_BOUNDARY_RUNTIME)
        .arg(concat!("dist/", "cli.js"));
}
"#,
            "process",
        ),
        (
            "crates/compat-harness/src/migration_support.rs",
            r#"
#[allow(dead_code)]
fn compatibility_boundary_shell_build_probe() {
    let _command = std::process::Command::new("sh")
        .arg("-c")
        .arg(concat!("n", "px tsc -p tsconfig.json"));
}
"#,
            "process",
        ),
        (
            "crates/compat-harness/src/migration_support.rs",
            r#"
#[allow(dead_code)]
fn compatibility_boundary_include_probe() {
    let _source = include_str!(concat!("../../../", "src/cli.tsx"));
}
"#,
            "source",
        ),
    ] {
        let fixture = HermeticCompatibilityFixture::new(&repository);
        fixture.append_compat_source(relative, source);
        let error = verify_fixture_compatibility(&fixture.root, false)
            .expect_err("executable legacy reference must fail closed");
        assert!(error.contains(relative), "missing mutated path: {error}");
        assert!(
            error.contains(reference_class),
            "missing reference class {reference_class}: {error}"
        );
    }

    let unresolved = HermeticCompatibilityFixture::new(&repository);
    unresolved.append_compat_source(
        "crates/compat-harness/src/lib.rs",
        "\npub mod missing_compatibility_boundary_probe;\n",
    );
    let error = verify_fixture_compatibility(&unresolved.root, false)
        .expect_err("unresolved module declaration must fail closed");
    assert!(
        error.contains("missing_compatibility_boundary_probe"),
        "{error}"
    );

    let orphan = HermeticCompatibilityFixture::new(&repository);
    orphan.write_compat_source(
        "crates/compat-harness/src/orphan_compatibility_boundary_probe.rs",
        "pub const ORPHAN_COMPATIBILITY_BOUNDARY_PROBE: bool = true;\n",
    );
    let error = verify_fixture_compatibility(&orphan.root, false)
        .expect_err("orphan Rust source must fail closed");
    assert!(
        error.contains("orphan_compatibility_boundary_probe.rs"),
        "{error}"
    );

    let linked = HermeticCompatibilityFixture::new(&repository);
    linked.append_compat_source(
        "crates/compat-harness/src/lib.rs",
        "\npub mod linked_compatibility_boundary_probe;\n",
    );
    linked.symlink_compat_source(
        "crates/compat-harness/src/report.rs",
        "crates/compat-harness/src/linked_compatibility_boundary_probe.rs",
    );
    let error = verify_fixture_compatibility(&linked.root, false)
        .expect_err("symlinked Rust module must fail closed");
    assert!(
        error.contains("linked_compatibility_boundary_probe.rs"),
        "{error}"
    );

    let ambiguous = HermeticCompatibilityFixture::new(&repository);
    ambiguous.append_compat_source(
        "crates/compat-harness/src/lib.rs",
        "\npub mod ambiguous_compatibility_boundary_probe;\n",
    );
    ambiguous.write_compat_source(
        "crates/compat-harness/src/ambiguous_compatibility_boundary_probe.rs",
        "pub const AMBIGUOUS_COMPATIBILITY_BOUNDARY_PROBE: bool = true;\n",
    );
    ambiguous.write_compat_source(
        "crates/compat-harness/src/ambiguous_compatibility_boundary_probe/mod.rs",
        "pub const AMBIGUOUS_COMPATIBILITY_BOUNDARY_PROBE: bool = true;\n",
    );
    let error = verify_fixture_compatibility(&ambiguous.root, false)
        .expect_err("ambiguous Rust module must fail closed");
    assert!(error.contains("ambiguous compatibility module"), "{error}");

    let duplicate = HermeticCompatibilityFixture::new(&repository);
    duplicate.append_compat_source("crates/compat-harness/src/lib.rs", "\npub mod report;\n");
    let error = verify_fixture_compatibility(&duplicate.root, false)
        .expect_err("duplicate module declaration must fail closed");
    assert!(
        error.contains("duplicate compatibility module declaration"),
        "{error}"
    );

    let unsafe_path = HermeticCompatibilityFixture::new(&repository);
    unsafe_path.append_compat_source(
        "crates/compat-harness/src/lib.rs",
        "\n#[path = \"../outside.rs\"]\npub mod unsafe_compatibility_boundary_probe;\n",
    );
    let error = verify_fixture_compatibility(&unsafe_path.root, false)
        .expect_err("module path attribute must fail closed");
    assert!(error.contains("path attributes are forbidden"), "{error}");

    let nested = HermeticCompatibilityFixture::new(&repository);
    nested.append_compat_source(
        "crates/compat-harness/src/lib.rs",
        "\npub mod nested_compatibility_boundary_probe;\n",
    );
    nested.write_compat_source(
        "crates/compat-harness/src/nested_compatibility_boundary_probe.rs",
        "pub mod child;\n",
    );
    nested.write_compat_source(
        "crates/compat-harness/src/nested_compatibility_boundary_probe/child.rs",
        r#"
#[allow(dead_code)]
fn nested_legacy_process_probe() {
    let _command = std::process::Command::new("tsc").arg("-p");
}
"#,
    );
    let error = verify_fixture_compatibility(&nested.root, false)
        .expect_err("nested executable legacy reference must fail closed");
    assert!(
        error.contains("nested_compatibility_boundary_probe/child.rs"),
        "{error}"
    );
}

#[test]
fn rust_command_permission_provider_and_product_baselines_are_executable() {
    let root = repository_root();
    let manifests = load_compat_manifests(&root).expect("strict manifests");
    validate_rust_command_surface(&manifests.commands).expect("complete Rust command surface");
    validate_rust_tool_evidence(&root, &manifests.public_contract)
        .expect("executable Rust tool evidence");
    validate_rust_vault_evidence(&root).expect("executable Rust Vault evidence");
    validate_rust_retrieval_evidence(&root).expect("executable Rust retrieval evidence");
    validate_rust_provider_profiles(&manifests.providers)
        .expect("executable Rust Provider profile evidence");
    validate_product_entry(&root).expect("Rust npm product entry");
    assert_launcher_contract(&root);
    validate_cutover_candidate(&root, &manifests.public_contract)
        .expect("hosted cutover candidate prerequisites");
}

#[test]
fn thin_npm_manifest_and_lock_are_distribution_only() {
    let root = repository_root();
    let package: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.join("package.json")).expect("package manifest"),
    )
    .expect("package JSON");
    let object = package.as_object().expect("package object");
    let actual_keys = object.keys().map(String::as_str).collect::<Vec<_>>();
    assert_eq!(
        actual_keys,
        [
            "bin",
            "description",
            "engines",
            "files",
            "name",
            "scripts",
            "type",
            "version",
        ],
        "package metadata must contain only the distribution contract"
    );
    assert_eq!(
        package["bin"],
        serde_json::json!({"minimax-codex": "bin/minimax-codex.cjs"})
    );
    assert_eq!(
        package["files"],
        serde_json::json!([
            "bin/minimax-codex.cjs",
            "docs/release",
            "LICENSE-APACHE",
            "LICENSE-MIT",
            "README.md",
            "minimax-codex",
            "minimax-codex.exe"
        ])
    );
    assert_eq!(
        package["scripts"],
        serde_json::json!({
            "check:rust": "cargo fmt --all -- --check && cargo clippy --workspace --all-targets --locked -- -D warnings",
            "test:rust": "cargo test --workspace --locked",
            "test:rust:strict-precondition": "cargo test --workspace --locked -- --skip hosted_cutover_evidence_matches_current_product",
            "test:rust:candidate": "cargo test --workspace --locked -- --skip hosted_cutover_evidence_matches_current_product --skip hosted_candidate_evidence_matches_current_product",
            "eval:retrieval": "cargo run -p minimax-compat-harness --locked -- retrieval-eval --format json",
            "eval:provider": "cargo run -p minimax-compat-harness --locked -- provider-eval --format json",
            "verify:agent": "npm run verify:rust-contracts && npm run eval:provider && npm run eval:retrieval",
            "verify:rust-contracts": "cargo run -p minimax-compat-harness --locked -- verify",
            "verify:rust-contracts:strict-precondition": "cargo run -p minimax-compat-harness --locked -- verify-strict-precondition",
            "verify:rust-contracts:candidate": "cargo run -p minimax-compat-harness --locked -- verify-candidate",
            "build:rust:release": "cargo build -p minimax-cli --release --locked",
            "package:rust": "node scripts/release/package-rust.mjs",
            "test:package": "node --test scripts/release/package-contract.test.mjs",
            "verify:rust-release": "node scripts/release/verify-rust-release.mjs",
            "verify:milestone-flow": "node scripts/release/verify-milestone-flow.mjs",
            "verify:release": "npm run check:rust && npm run test:rust && npm run verify:agent && npm run test:package"
        })
    );
    for dependency_class in ["dependencies", "devDependencies", "optionalDependencies"] {
        assert!(
            package.get(dependency_class).is_none(),
            "{dependency_class} must be absent"
        );
    }

    let lock: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.join("package-lock.json")).expect("package lock"),
    )
    .expect("package-lock JSON");
    assert_eq!(lock["lockfileVersion"], 3);
    assert_eq!(
        lock["packages"]
            .as_object()
            .expect("lock packages")
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        [""],
        "dependency-free package lock must contain only the root package"
    );
    let lock_root = &lock["packages"][""];
    assert_eq!(lock_root["bin"], package["bin"]);
    for dependency_class in ["dependencies", "devDependencies", "optionalDependencies"] {
        assert!(
            lock_root.get(dependency_class).is_none(),
            "lock root {dependency_class} must be absent"
        );
    }

    let legacy_file = HermeticCompatibilityFixture::new(&root);
    legacy_file.rewrite_json("package.json", |value| {
        value["files"]
            .as_array_mut()
            .expect("package files")
            .push(serde_json::json!("dist"));
    });
    assert!(validate_product_entry(&legacy_file.root).is_err());

    let runtime_dependency = HermeticCompatibilityFixture::new(&root);
    runtime_dependency.rewrite_json("package.json", |value| {
        value["dependencies"] = serde_json::json!({"react": "^18.3.1"});
    });
    assert!(validate_product_entry(&runtime_dependency.root).is_err());

    let install_download = HermeticCompatibilityFixture::new(&root);
    install_download.rewrite_json("package.json", |value| {
        value["scripts"]["install"] =
            serde_json::json!("node -e \"fetch('https://example.invalid/runtime')\"");
    });
    assert!(validate_product_entry(&install_download.root).is_err());

    let stale_lock = HermeticCompatibilityFixture::new(&root);
    stale_lock.rewrite_json("package-lock.json", |value| {
        value["packages"][""]["bin"]["minimax-codex-legacy"] = serde_json::json!("dist/cli.js");
    });
    assert!(validate_product_entry(&stale_lock.root).is_err());
}

#[test]
fn hosted_cutover_evidence_matches_current_product() {
    let root = repository_root();
    let manifests = load_compat_manifests(&root).expect("strict manifests");
    validate_cutover_evidence(&root, &manifests.public_contract).expect("hosted cutover evidence");
}

#[test]
fn hosted_candidate_evidence_matches_current_product() {
    validate_hosted_candidate_gate(&repository_root()).expect("hosted candidate evidence");
}

#[test]
fn stale_hosted_evidence_is_rejected_for_freshness_or_fingerprint() {
    let root = repository_root();
    let mut gate = synthetic_hosted_gate(&root, "pending");
    gate["productFingerprint"] = serde_json::json!("0".repeat(64));
    let source = serde_json::to_string(&gate).expect("serialize stale hosted evidence");

    assert_eq!(
        validate_hosted_release_gate_document(&root, &source),
        Err(BaselineError::HostedEvidenceFingerprintStale)
    );
}

#[test]
fn gnullvm_development_evidence_cannot_satisfy_hosted_msvc() {
    let root = repository_root();
    let mut gate = synthetic_hosted_gate(&root, "passed");
    let windows = gate["candidate"]["jobs"]
        .as_array_mut()
        .expect("hosted jobs")
        .iter_mut()
        .find(|job| job["platform"] == "windows-x86_64-msvc")
        .expect("Windows MSVC hosted job");
    windows["environment"]["rustcHost"] = serde_json::json!("x86_64-pc-windows-gnullvm");
    let source = serde_json::to_string(&gate).expect("serialize tier-confused hosted evidence");

    assert_eq!(
        validate_hosted_release_gate_document(&root, &source),
        Err(BaselineError::CutoverEvidence)
    );
}

#[test]
fn pending_candidate_is_a_strict_precondition_but_not_final_closure() {
    let root = repository_root();
    let pending = serde_json::to_string(&synthetic_hosted_gate(&root, "pending"))
        .expect("serialize pending hosted evidence");
    assert_eq!(
        validate_hosted_candidate_gate_document(&root, &pending),
        Ok(())
    );
    assert_eq!(
        validate_hosted_release_gate_document(&root, &pending),
        Err(BaselineError::HostedEvidenceStrictPending)
    );

    let passed = serde_json::to_string(&synthetic_hosted_gate(&root, "passed"))
        .expect("serialize combined hosted evidence");
    assert_eq!(
        validate_hosted_release_gate_document(&root, &passed),
        Ok(())
    );
}

#[test]
fn combined_hosted_evidence_rejects_wrong_job_urls_and_unordered_runs() {
    let root = repository_root();
    let mut wrong_url = synthetic_hosted_gate(&root, "passed");
    wrong_url["strict"]["jobs"][0]["jobUrl"] =
        serde_json::json!("https://github.com/niuniu122/wiki-coding/actions/runs/202/job/9999");
    assert_eq!(
        validate_hosted_release_gate_document(
            &root,
            &serde_json::to_string(&wrong_url).expect("wrong URL evidence")
        ),
        Err(BaselineError::CutoverEvidence)
    );

    let mut unordered = synthetic_hosted_gate(&root, "passed");
    unordered["strict"]["runId"] = serde_json::json!(99);
    unordered["strict"]["runUrl"] =
        serde_json::json!("https://github.com/niuniu122/wiki-coding/actions/runs/99");
    assert_eq!(
        validate_hosted_release_gate_document(
            &root,
            &serde_json::to_string(&unordered).expect("unordered evidence")
        ),
        Err(BaselineError::CutoverEvidence)
    );
}

fn synthetic_hosted_gate(root: &Path, strict_status: &str) -> serde_json::Value {
    let (fingerprint, file_count) =
        compute_product_fingerprint(root).expect("current product fingerprint should compute");
    let candidate = synthetic_hosted_run(101, "workflow_dispatch", '1', '2', &fingerprint);
    let strict = (strict_status == "passed")
        .then(|| synthetic_hosted_run(202, "push", '3', '4', &fingerprint));
    serde_json::json!({
        "schemaVersion": 2,
        "evidenceClass": "hosted_release_gate",
        "workflow": "CI",
        "branch": "codex/rust-convergence-v3",
        "productFingerprint": fingerprint,
        "productFileCount": file_count,
        "candidate": candidate,
        "strictStatus": strict_status,
        "strict": strict
    })
}

fn synthetic_hosted_run(
    run_id: u64,
    event: &str,
    head: char,
    tree: char,
    fingerprint: &str,
) -> serde_json::Value {
    serde_json::json!({
        "runId": run_id,
        "runUrl": format!("https://github.com/niuniu122/wiki-coding/actions/runs/{run_id}"),
        "event": event,
        "branch": "codex/rust-convergence-v3",
        "headSha": head.to_string().repeat(40),
        "treeSha": tree.to_string().repeat(40),
        "conclusion": "success",
        "jobs": [
            synthetic_hosted_job(run_id, run_id * 10 + 1, "windows-x86_64-msvc", fingerprint),
            synthetic_hosted_job(run_id, run_id * 10 + 2, "linux-x86_64-gnu", fingerprint)
        ]
    })
}

fn synthetic_hosted_job(
    run_id: u64,
    job_id: u64,
    platform: &str,
    fingerprint: &str,
) -> serde_json::Value {
    let windows = platform == "windows-x86_64-msvc";
    let (os, rustc_host, job_name, artifact_name, native_hash, npm_hash, binary_hash) = if windows {
        (
            "win32",
            "x86_64-pc-windows-msvc",
            "verify (windows-latest)",
            "hosted-release-evidence-Windows",
            "a".repeat(64),
            "b".repeat(64),
            "c".repeat(64),
        )
    } else {
        (
            "linux",
            "x86_64-unknown-linux-gnu",
            "verify (ubuntu-latest)",
            "hosted-release-evidence-Linux",
            "d".repeat(64),
            "e".repeat(64),
            "f".repeat(64),
        )
    };
    let native = serde_json::json!({
        "installedVersionOutput": "minimax-codex-rust 0.1.0",
        "packagedBinarySha256": binary_hash,
        "capabilityStatusSmoke": true,
        "capabilityStatusOutputSha256": "7".repeat(64),
        "productFingerprint": fingerprint,
        "offline": true,
        "providerCalls": 0,
        "credentialsRead": 0,
        "modelDownloads": 0,
        "credentialsExcluded": true,
        "pathLookupExcluded": true,
        "developmentRuntimeAugmented": false
    });
    let mut npm = native.clone();
    npm["missingSiblingRejected"] = serde_json::json!(true);
    npm["unsafeSiblingRejected"] = serde_json::json!(true);
    serde_json::json!({
        "jobId": job_id,
        "jobUrl": format!("https://github.com/niuniu122/wiki-coding/actions/runs/{run_id}/job/{job_id}"),
        "jobName": job_name,
        "artifactName": artifact_name,
        "evidenceFileSha256": "9".repeat(64),
        "platform": platform,
        "conclusion": "success",
        "linuxSandboxCanary": !windows,
        "environment": {
            "os": os,
            "osRelease": "test-release",
            "architecture": "x64",
            "cpuModel": "test cpu",
            "logicalCpuCount": 4,
            "totalMemoryBytes": 16_000_000_000_u64,
            "node": "v20.20.2",
            "rustcRelease": "1.97.0",
            "rustcHost": rustc_host
        },
        "package": {
            "nativeArchiveSha256": native_hash,
            "npmArchiveSha256": npm_hash,
            "binarySha256": binary_hash,
            "nativeCompressedBytes": 4_000_000,
            "npmCompressedBytes": 4_100_000,
            "embeddingIncluded": false,
            "supportTier": "hosted_release"
        },
        "installedNative": native,
        "installedNpm": npm,
        "licenses": {"packagesChecked": 234, "invalid": 0},
        "security": {
            "unsafeFiles": 0,
            "unsafeWorkspaceLint": "forbid",
            "databasePackages": 0,
            "migrationNetworkOrCredentialPaths": 0
        },
        "performance": {
            "coldStartSamplesMs": vec![10.0; 9],
            "coldStartP95Ms": 10.0,
            "idleRssSamplesBytes": vec![5_000_000_u64; 5],
            "idleRssMaximumBytes": 5_000_000,
            "baseCompressedBytes": 4_000_000,
            "wikiBm25P95Ms": 1.0
        },
        "offline": true,
        "providerCalls": 0,
        "credentialsRead": 0,
        "modelDownloads": 0
    })
}

#[test]
fn product_fingerprint_v3_tracks_working_tree_and_excludes_only_planning_and_hosted_record() {
    let repository = FingerprintRepository::new();
    let first = compute_product_fingerprint(&repository.root).expect("first Rust fingerprint");
    assert_eq!(first.1, 2);
    assert_eq!(node_product_fingerprint(&repository.root), first);

    fs::write(
        repository.root.join("product.txt"),
        "tracked-working-tree-v2\n",
    )
    .expect("edit tracked product input");
    let tracked_edit =
        compute_product_fingerprint(&repository.root).expect("tracked edit fingerprint");
    assert_ne!(tracked_edit.0, first.0);

    fs::write(
        repository.root.join(".planning/note.md"),
        "excluded-planning-v2\n",
    )
    .expect("edit excluded planning input");
    let planning_edit =
        compute_product_fingerprint(&repository.root).expect("planning edit fingerprint");
    assert_eq!(planning_edit, tracked_edit);

    fs::write(
        repository
            .root
            .join("fixtures/compat/release/hosted-gates.v1.json"),
        "{\"excluded\":2}\n",
    )
    .expect("edit excluded hosted evidence input");
    let evidence_edit =
        compute_product_fingerprint(&repository.root).expect("evidence edit fingerprint");
    assert_eq!(evidence_edit, planning_edit);

    fs::write(repository.root.join("untracked.txt"), "untracked-v2\n")
        .expect("edit untracked product input");
    let untracked_edit =
        compute_product_fingerprint(&repository.root).expect("untracked edit fingerprint");
    assert_ne!(untracked_edit.0, evidence_edit.0);
    assert_eq!(node_product_fingerprint(&repository.root), untracked_edit);
}

#[test]
fn cutover_rejects_a_pending_mandatory_rust_item() {
    let root = repository_root();
    let manifests = load_compat_manifests(&root).expect("strict manifests");
    let mut public_contract = manifests.public_contract;
    let release = public_contract
        .items
        .iter_mut()
        .find(|item| item.id == "contract.release_gate")
        .expect("release item");
    release.status = ParityStatus::Pending;
    release.evidence.clear();
    assert!(validate_cutover_candidate(&root, &public_contract).is_err());
}

#[test]
fn compat_report_rejects_matched_item_without_evidence() {
    let root = repository_root();
    let manifests = load_compat_manifests(&root).expect("strict manifests");
    let mut report = build_report(&manifests, &root).expect("contract report");
    let matched = report
        .entries
        .iter_mut()
        .find(|item| item.rust_status == ParityStatus::Matched)
        .expect("matched item");
    let id = matched.id.clone();
    matched.rust_evidence.clear();

    assert_eq!(
        validate_report(&report, &manifests, &root),
        Err(ManifestError::Validation(format!(
            "matched item requires evidence: {id}"
        )))
    );
}

#[test]
fn architecture_real_cargo_metadata_passes() {
    let root = repository_root();
    let graph = load_cargo_architecture(&root).expect("locked Cargo metadata");
    validate_architecture(&graph).expect("valid workspace architecture");
    validate_core_source_boundary(&root).expect("abstract core source boundary");
    validate_vault_source_boundary(&root).expect("Provider-free Vault source boundary");
    validate_cli_tui_markdown_boundary(&root).expect("Vault-owned Markdown parsing");
    validate_retrieval_source_boundary(&root).expect("offline retrieval boundary");
    validate_migration_source_boundary(&root).expect("offline secret-free migration boundary");
}

#[test]
fn architecture_rejects_migration_network_database_credentials_and_downloads() {
    for source in [
        "use reqwest::Client;",
        "use minimax_provider::ProviderPort;",
        "let key = std::env::var(\"API_KEY\");",
        "use rusqlite::Connection;",
        "fn download_resource() {}",
        "let header = \"Authorization\";",
    ] {
        assert!(validate_migration_source_text("bad.rs", source).is_err());
    }
}

#[test]
fn architecture_rejects_retrieval_network_database_credentials_and_downloads() {
    for source in [
        "use reqwest::Client;",
        "use rusqlite::Connection;",
        "let key = std::env::var(\"API_KEY\");",
        "fn download_model() {}",
        "use minimax_provider::ProviderPort;",
    ] {
        assert!(validate_retrieval_source_text("bad.rs", source).is_err());
    }
}

#[test]
fn architecture_rejects_vault_provider_http_and_database_edges() {
    for dependency in ["minimax-provider", "reqwest"] {
        let graph = synthetic_graph(&[("minimax-vault", &[dependency])]);
        assert_eq!(
            validate_architecture(&graph),
            Err(ArchitectureError::Violation(format!(
                "vault dependency denied: minimax-vault -> {dependency}"
            )))
        );
    }
    let graph = synthetic_graph(&[("minimax-vault", &["rusqlite"])]);
    assert_eq!(
        validate_architecture(&graph),
        Err(ArchitectureError::Violation(
            "database dependency denied: rusqlite".to_owned()
        ))
    );
    for source in [
        "use minimax_provider::ProviderPort;",
        "use reqwest::Client;",
        "use rusqlite::Connection;",
    ] {
        assert!(validate_vault_source_text("bad.rs", source).is_err());
    }
}

#[test]
fn architecture_rejects_cli_or_tui_markdown_parsing() {
    for source in [
        "minimax_vault::parse_wiki_page(path, bytes);",
        "let parser = pulldown_cmark::Parser::new(text);",
        "let parts = text.split_once(\"\\n---\\n\");",
    ] {
        assert!(validate_ui_source_text("bad.rs", source).is_err());
    }
}

#[test]
fn architecture_rejects_core_to_vault() {
    let graph = synthetic_graph(&[("minimax-core", &["minimax-vault"])]);
    assert_eq!(
        validate_architecture(&graph),
        Err(ArchitectureError::Violation(
            "core dependency denied: minimax-core -> minimax-vault".to_owned()
        ))
    );
}

#[test]
fn architecture_rejects_production_to_harness() {
    let graph = synthetic_graph(&[("minimax-provider", &["minimax-compat-harness"])]);
    assert_eq!(
        validate_architecture(&graph),
        Err(ArchitectureError::Violation(
            "production package must not depend on compat harness: minimax-provider -> minimax-compat-harness"
                .to_owned()
        ))
    );
}

#[test]
fn architecture_rejects_local_cycle() {
    let graph = synthetic_graph(&[
        ("minimax-core", &["minimax-protocol"]),
        ("minimax-protocol", &["minimax-core"]),
    ]);
    assert_eq!(
        validate_architecture(&graph),
        Err(ArchitectureError::Violation(
            "local dependency cycle involving: minimax-core, minimax-protocol".to_owned()
        ))
    );

    let graph = synthetic_graph(&[
        ("minimax-cli", &["minimax-provider"]),
        ("minimax-provider", &["minimax-core"]),
        ("minimax-core", &["minimax-protocol"]),
        ("minimax-protocol", &[]),
    ]);
    validate_architecture(&graph).expect("acyclic control graph");
}

#[test]
fn architecture_rejects_database_package() {
    for package in ["rusqlite", "sqlx-core", "diesel", "sea-orm"] {
        let mut graph = synthetic_graph(&[("minimax-protocol", &[])]);
        graph.packages.push(ArchitecturePackage {
            name: package.to_owned(),
            local: false,
            dependencies: Vec::new(),
        });
        assert_eq!(
            validate_architecture(&graph),
            Err(ArchitectureError::Violation(format!(
                "database dependency denied: {package}"
            )))
        );
    }
}

#[test]
fn architecture_rejects_database_access_in_core_source() {
    for pattern in ["rusqlite", "sqlx", "diesel", "sea_orm", "seaorm"] {
        let source = format!("use {pattern}::Connection;");
        assert!(matches!(
            validate_core_source_text("storage.rs", &source),
            Err(ArchitectureError::Violation(_))
        ));
    }
}

#[test]
fn architecture_rejects_core_http_dependency() {
    let graph = synthetic_graph(&[("minimax-core", &["minimax-protocol", "reqwest"])]);
    assert_eq!(
        validate_architecture(&graph),
        Err(ArchitectureError::Violation(
            "core dependency denied: minimax-core -> reqwest".to_owned()
        ))
    );
}

#[test]
fn architecture_rejects_markdown_paths_in_core_source() {
    assert_eq!(
        validate_core_source_text("session.rs", "use std::path::PathBuf; // notes.md"),
        Err(ArchitectureError::Violation(
            "core source boundary denied: session.rs contains std::path".to_owned()
        ))
    );
}

#[test]
fn architecture_recurses_into_nested_core_modules() {
    let unique = format!(
        "minimax-core-architecture-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos()
    );
    let root = std::env::temp_dir().join(unique);
    let nested = root.join("nested");
    fs::create_dir_all(&nested).expect("create nested fixture");
    fs::write(nested.join("adapter.rs"), "use tokio::time::sleep;").expect("write nested fixture");

    let result = validate_core_source_directory(&root);
    fs::remove_dir_all(&root).expect("remove nested fixture");

    let Err(ArchitectureError::Violation(message)) = result else {
        panic!("nested forbidden import should fail");
    };
    assert!(message.contains("nested"));
    assert!(message.contains("adapter.rs"));
    assert!(message.contains("tokio::"));
}

fn synthetic_graph(edges: &[(&str, &[&str])]) -> ArchitectureGraph {
    let mut packages = std::collections::BTreeMap::new();
    for (name, dependencies) in edges {
        packages.insert(
            (*name).to_owned(),
            dependencies
                .iter()
                .map(|dependency| (*dependency).to_owned())
                .collect::<Vec<_>>(),
        );
        for dependency in *dependencies {
            packages.entry((*dependency).to_owned()).or_default();
        }
    }
    ArchitectureGraph {
        packages: packages
            .iter()
            .map(|(name, dependencies)| ArchitecturePackage {
                name: name.clone(),
                local: true,
                dependencies: dependencies.clone(),
            })
            .collect(),
    }
}

fn normalize_golden_newlines(value: &str) -> String {
    value.replace("\r\n", "\n")
}

fn assert_launcher_contract(repository_root: &Path) {
    let missing = LauncherFixture::new(repository_root);
    assert_launcher_failure(
        &missing.run(&["--version"]),
        "E_BINARY_MISSING",
        "missing",
        Some(&missing.binary_path()),
    );

    let unsafe_entry = LauncherFixture::new(repository_root);
    fs::create_dir_all(unsafe_entry.binary_path()).expect("unsafe binary directory");
    assert_launcher_failure(
        &unsafe_entry.run(&["--version"]),
        "E_BINARY_UNSAFE",
        "safe regular file",
        Some(&unsafe_entry.binary_path()),
    );

    let non_executable = LauncherFixture::new(repository_root);
    non_executable.write_binary(b"not executable", false);
    non_executable.rewrite_launcher(|source| {
        source.replace("if (process.platform !== \"win32\" &&", "if (true &&")
    });
    assert_launcher_failure(
        &non_executable.run(&["--version"]),
        "E_BINARY_NOT_EXECUTABLE",
        "not executable",
        Some(&non_executable.binary_path()),
    );

    let unsupported = LauncherFixture::new(repository_root);
    unsupported.rewrite_launcher(|source| {
        source
            .replace("\"win32:x64\"", "\"fixture-win32:x64\"")
            .replace("\"linux:x64\"", "\"fixture-linux:x64\"")
    });
    assert_launcher_failure(
        &unsupported.run(&["--version"]),
        "E_UNSUPPORTED_HOST",
        "unsupported host",
        None,
    );

    let cannot_start = LauncherFixture::new(repository_root);
    cannot_start.write_binary(b"not an executable image", true);
    assert_launcher_failure(
        &cannot_start.run(&["--version"]),
        "E_START_FAILED",
        "could not start",
        Some(&cannot_start.binary_path()),
    );

    let forwarding = LauncherFixture::new(repository_root);
    forwarding.install_node_binary();
    let argument_probe = forwarding.write_probe(
        "argument-probe.cjs",
        "process.stdout.write(JSON.stringify(process.argv.slice(2)));\n",
    );
    let output = forwarding.run(&[
        argument_probe.to_str().expect("UTF-8 probe path"),
        "中文 request",
        "$(not-a-shell)",
        "--flag=value",
    ]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let arguments: Vec<String> =
        serde_json::from_slice(&output.stdout).expect("forwarded argv JSON");
    assert_eq!(
        arguments,
        ["中文 request", "$(not-a-shell)", "--flag=value"]
    );

    let exit_probe = forwarding.write_probe("exit-probe.cjs", "process.exit(7);\n");
    let output = forwarding.run(&[exit_probe.to_str().expect("UTF-8 exit probe path")]);
    assert_eq!(output.status.code(), Some(7), "{}", stderr(&output));

    #[cfg(unix)]
    {
        let signal_probe = forwarding.write_probe(
            "signal-probe.cjs",
            "process.kill(process.pid, 'SIGTERM');\n",
        );
        assert_launcher_failure(
            &forwarding.run(&[signal_probe.to_str().expect("UTF-8 signal probe path")]),
            "E_SIGNAL_TERMINATION",
            "ended by signal",
            Some(&forwarding.binary_path()),
        );
    }
}

fn assert_launcher_failure(
    output: &Output,
    expected_code: &str,
    expected_detail: &str,
    expected_path: Option<&Path>,
) {
    assert_eq!(output.status.code(), Some(1), "{}", stderr(output));
    assert!(output.stdout.is_empty());
    let stderr = stderr(output).to_ascii_lowercase();
    assert!(
        stderr.contains(&format!("minimax-codex [{expected_code}]:").to_ascii_lowercase()),
        "missing stable launcher error code: {stderr}"
    );
    assert!(
        stderr.contains(expected_detail),
        "unexpected launcher error: {stderr}"
    );
    if let Some(path) = expected_path {
        assert!(
            stderr.contains("expected path:"),
            "missing expected path: {stderr}"
        );
        assert!(
            stderr.contains(&path.to_string_lossy().to_ascii_lowercase()),
            "missing concrete expected path: {stderr}"
        );
    } else {
        assert!(
            stderr.contains("expected packaged targets: win32/x64, linux/x64"),
            "missing supported target guidance: {stderr}"
        );
    }
    for guidance in ["reinstall", "supported", "windows x64", "linux x64"] {
        assert!(stderr.contains(guidance), "missing {guidance}: {stderr}");
    }
    for fallback in [
        "minimax-codex-legacy",
        "dist/cli.js",
        "src/cli.tsx",
        "download",
    ] {
        assert!(
            !stderr.contains(fallback),
            "fallback guidance leaked: {stderr}"
        );
    }
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

struct LauncherFixture {
    root: PathBuf,
    launcher: PathBuf,
    node: PathBuf,
}

struct CompatManifestFixture {
    root: PathBuf,
}

struct HermeticCompatibilityFixture {
    root: PathBuf,
}

struct FingerprintRepository {
    root: PathBuf,
}

impl FingerprintRepository {
    fn new() -> Self {
        let unique = format!(
            "minimax-fingerprint-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock after epoch")
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        fs::create_dir_all(root.join(".planning")).expect("create planning fixture directory");
        fs::create_dir_all(root.join("fixtures/compat/release"))
            .expect("create hosted evidence fixture directory");
        fs::write(root.join("product.txt"), "tracked-working-tree-v1\n")
            .expect("write tracked product input");
        fs::write(root.join("untracked.txt"), "untracked-v1\n")
            .expect("write untracked product input");
        fs::write(root.join(".planning/note.md"), "excluded-planning-v1\n")
            .expect("write excluded planning input");
        fs::write(
            root.join("fixtures/compat/release/hosted-gates.v1.json"),
            "{\"excluded\":1}\n",
        )
        .expect("write excluded hosted evidence input");
        run_git(&root, &["init", "--quiet"]);
        run_git(
            &root,
            &[
                "add",
                "--",
                "product.txt",
                ".planning/note.md",
                "fixtures/compat/release/hosted-gates.v1.json",
            ],
        );
        Self { root }
    }
}

impl Drop for FingerprintRepository {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).expect("remove fingerprint repository");
    }
}

fn run_git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .expect("run Git for fingerprint fixture");
    assert!(output.status.success(), "{}", stderr(&output));
}

fn node_product_fingerprint(root: &Path) -> (String, u64) {
    let output = Command::new("node")
        .arg(repository_root().join("scripts/release/product-fingerprint.mjs"))
        .arg("--root")
        .arg(root)
        .output()
        .expect("run Node product fingerprint");
    assert!(output.status.success(), "{}", stderr(&output));
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("Node fingerprint JSON");
    (
        value["fingerprint"]
            .as_str()
            .expect("Node fingerprint")
            .to_owned(),
        value["fileCount"].as_u64().expect("Node file count"),
    )
}

impl HermeticCompatibilityFixture {
    fn new(repository_root: &Path) -> Self {
        let unique = format!(
            "minimax-hermetic-compat-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock after epoch")
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        fs::create_dir_all(&root).expect("create hermetic compatibility root");
        for relative in [
            "Cargo.toml",
            "Cargo.lock",
            "rust-toolchain.toml",
            "package.json",
            "package-lock.json",
            "bin",
            "capabilities",
            "crates",
            "fixtures",
            "scripts/release",
            ".github/workflows",
        ] {
            copy_fixture_path(repository_root, &root, relative);
        }
        Self { root }
    }

    fn rewrite_json(&self, relative: &str, mutate: impl FnOnce(&mut serde_json::Value)) {
        let path = self.root.join(relative);
        let mut value: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).expect("fixture JSON"))
                .expect("valid fixture JSON");
        mutate(&mut value);
        fs::write(
            path,
            serde_json::to_vec_pretty(&value).expect("serialize fixture JSON"),
        )
        .expect("rewrite fixture JSON");
    }

    fn append_compat_source(&self, relative: &str, suffix: &str) {
        let path = self.root.join(relative);
        let mut source = fs::read_to_string(&path).expect("compatibility source");
        source.push_str(suffix);
        fs::write(path, source).expect("rewrite compatibility source");
    }

    fn write_compat_source(&self, relative: &str, source: &str) {
        let path = self.root.join(relative);
        fs::create_dir_all(path.parent().expect("compatibility source parent"))
            .expect("create compatibility source parent");
        fs::write(path, source).expect("write compatibility source");
    }

    fn symlink_compat_source(&self, target: &str, link: &str) {
        let target = self.root.join(target);
        let link = self.root.join(link);
        #[cfg(unix)]
        std::os::unix::fs::symlink(target, link).expect("symlink compatibility source");
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(target, link).expect("symlink compatibility source");
    }
}

impl Drop for HermeticCompatibilityFixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).expect("remove hermetic compatibility fixture");
    }
}

fn copy_fixture_path(repository_root: &Path, fixture_root: &Path, relative: &str) {
    let source = repository_root.join(relative);
    let destination = fixture_root.join(relative);
    if source.is_dir() {
        fs::create_dir_all(&destination).expect("create copied fixture directory");
        for entry in fs::read_dir(&source).expect("read copied fixture directory") {
            let entry = entry.expect("fixture directory entry");
            let name = entry.file_name();
            let child = Path::new(relative).join(name);
            copy_fixture_path(
                repository_root,
                fixture_root,
                child.to_str().expect("UTF-8 fixture path"),
            );
        }
    } else {
        fs::create_dir_all(destination.parent().expect("copied fixture parent"))
            .expect("create copied fixture parent");
        fs::copy(source, destination).expect("copy hermetic fixture path");
    }
}

impl CompatManifestFixture {
    fn new(repository_root: &Path) -> Self {
        let unique = format!(
            "minimax-public-contract-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock after epoch")
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        for relative in [
            "fixtures/compat/commands.v1.json",
            "fixtures/compat/providers.v1.json",
            "fixtures/compat/public-contract.v1.json",
        ] {
            let destination = root.join(relative);
            fs::create_dir_all(destination.parent().expect("fixture parent"))
                .expect("create fixture parent");
            fs::copy(repository_root.join(relative), destination).expect("copy manifest fixture");
        }
        let contract: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(root.join("fixtures/compat/public-contract.v1.json"))
                .expect("public contract fixture"),
        )
        .expect("public contract JSON");
        for evidence in contract["items"]
            .as_array()
            .expect("contract items")
            .iter()
            .flat_map(|item| item["evidence"].as_array().expect("contract evidence"))
            .map(|evidence| evidence.as_str().expect("evidence path"))
        {
            let path = root.join(evidence);
            if !path.exists() {
                fs::create_dir_all(path.parent().expect("evidence parent"))
                    .expect("create evidence parent");
                fs::write(path, []).expect("write evidence placeholder");
            }
        }
        Self { root }
    }

    fn rewrite_contract(&self, mutate: impl FnOnce(&mut serde_json::Value)) {
        let path = self.root.join("fixtures/compat/public-contract.v1.json");
        let mut contract: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).expect("public contract fixture"))
                .expect("public contract JSON");
        mutate(&mut contract);
        fs::write(
            path,
            serde_json::to_vec_pretty(&contract).expect("serialize public contract"),
        )
        .expect("rewrite public contract");
    }
}

impl Drop for CompatManifestFixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).expect("remove public contract fixture");
    }
}

impl LauncherFixture {
    fn new(repository_root: &Path) -> Self {
        let unique = format!(
            "minimax-launcher-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock after epoch")
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        let launcher = root.join("bin/minimax-codex.cjs");
        fs::create_dir_all(launcher.parent().expect("launcher parent"))
            .expect("launcher fixture directory");
        fs::copy(repository_root.join("bin/minimax-codex.cjs"), &launcher)
            .expect("launcher fixture source");
        Self {
            root,
            launcher,
            node: node_executable(),
        }
    }

    fn binary_path(&self) -> PathBuf {
        self.root.join(if cfg!(windows) {
            "minimax-codex.exe"
        } else {
            "minimax-codex"
        })
    }

    fn install_node_binary(&self) {
        fs::copy(&self.node, self.binary_path()).expect("fixture executable");
        set_executable(&self.binary_path(), true);
    }

    fn write_binary(&self, bytes: &[u8], executable: bool) {
        fs::write(self.binary_path(), bytes).expect("fixture binary bytes");
        set_executable(&self.binary_path(), executable);
    }

    fn write_probe(&self, name: &str, source: &str) -> PathBuf {
        let path = self.root.join(name);
        fs::write(&path, source).expect("launcher probe");
        path
    }

    fn rewrite_launcher(&self, transform: impl FnOnce(String) -> String) {
        let source = fs::read_to_string(&self.launcher).expect("launcher fixture");
        let transformed = transform(source.clone());
        assert_ne!(
            transformed, source,
            "launcher fixture transform matched nothing"
        );
        fs::write(&self.launcher, transformed).expect("rewritten launcher fixture");
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(&self.node)
            .arg(&self.launcher)
            .args(args)
            .output()
            .expect("run launcher fixture")
    }
}

impl Drop for LauncherFixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).expect("remove launcher fixture");
    }
}

fn node_executable() -> PathBuf {
    let output = Command::new("node")
        .args(["-p", "process.execPath"])
        .output()
        .expect("Node is required for npm launcher verification");
    assert!(output.status.success(), "{}", stderr(&output));
    PathBuf::from(
        String::from_utf8(output.stdout)
            .expect("Node path UTF-8")
            .trim(),
    )
}

#[cfg(unix)]
fn set_executable(path: &Path, executable: bool) {
    use std::os::unix::fs::PermissionsExt as _;

    let mode = if executable { 0o755 } else { 0o644 };
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).expect("fixture permissions");
}

#[cfg(not(unix))]
fn set_executable(_path: &Path, _executable: bool) {}
