use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use minimax_compat_harness::{
    ArchitectureError, ArchitectureGraph, ArchitecturePackage, ManifestError, ParityStatus,
    build_report, load_cargo_architecture, load_compat_manifests, report_json, repository_root,
    validate_architecture, validate_cli_tui_markdown_boundary, validate_core_source_boundary,
    validate_core_source_directory, validate_core_source_text, validate_cutover_candidate,
    validate_cutover_evidence, validate_migration_source_boundary, validate_migration_source_text,
    validate_product_entry, validate_report, validate_retrieval_source_boundary,
    validate_retrieval_source_text, validate_rust_command_surface, validate_rust_provider_profiles,
    validate_rust_retrieval_evidence, validate_rust_tool_evidence, validate_rust_vault_evidence,
    validate_ui_source_text, validate_vault_source_boundary, validate_vault_source_text,
};

#[test]
fn compat_report_matches_golden_and_is_byte_identical_on_second_run() {
    let root = repository_root();
    let first_manifests = load_compat_manifests(&root).expect("strict manifests");
    let second_manifests = load_compat_manifests(&root).expect("strict manifests on second load");
    let first = build_report(&first_manifests);
    let second = build_report(&second_manifests);
    validate_report(&first, &root).expect("valid report");
    validate_report(&second, &root).expect("valid report on second run");

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
    let report = build_report(&manifests);
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
fn rust_command_permission_provider_and_product_baselines_are_executable() {
    let root = repository_root();
    let manifests = load_compat_manifests(&root).expect("strict manifests");
    validate_rust_command_surface(&manifests.commands).expect("complete Rust command surface");
    validate_rust_tool_evidence(&root, &manifests.baseline).expect("executable Rust tool evidence");
    validate_rust_vault_evidence(&root).expect("executable Rust Vault evidence");
    validate_rust_retrieval_evidence(&root).expect("executable Rust retrieval evidence");
    validate_rust_provider_profiles(&manifests.providers)
        .expect("executable Rust Provider profile evidence");
    validate_product_entry(&root).expect("Rust npm product entry");
    let package: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.join("package.json")).expect("package manifest"),
    )
    .expect("package JSON");
    assert_eq!(
        package["scripts"]["dev"], "cargo run -p minimax-cli --locked --",
        "development must execute the Rust CLI source"
    );
    assert_eq!(
        package["scripts"]["start"], "node bin/minimax-codex.cjs",
        "start must remain the thin packaged Rust launcher"
    );
    assert_launcher_contract(&root);
    validate_cutover_candidate(&root, &manifests.baseline)
        .expect("hosted cutover candidate prerequisites");
}

#[test]
fn hosted_cutover_evidence_matches_current_product() {
    let root = repository_root();
    let manifests = load_compat_manifests(&root).expect("strict manifests");
    validate_cutover_evidence(&root, &manifests.baseline).expect("hosted cutover evidence");
}

#[test]
fn cutover_rejects_a_pending_mandatory_rust_item() {
    let root = repository_root();
    let manifests = load_compat_manifests(&root).expect("strict manifests");
    let mut baseline = manifests.baseline;
    let release = baseline
        .items
        .iter_mut()
        .find(|item| item.id == "rust.release_gate")
        .expect("release item");
    release.status = ParityStatus::Pending;
    release.evidence.clear();
    assert!(validate_cutover_candidate(&root, &baseline).is_err());
}

#[test]
fn compat_report_rejects_matched_item_without_evidence() {
    let root = repository_root();
    let manifests = load_compat_manifests(&root).expect("strict manifests");
    let mut report = build_report(&manifests);
    let matched = report
        .entries
        .iter_mut()
        .find(|item| item.status == ParityStatus::Matched)
        .expect("matched item");
    let id = matched.id.clone();
    matched.evidence.clear();

    assert_eq!(
        validate_report(&report, &root),
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
    assert_launcher_failure(&missing.run(&["--version"]), "missing");

    let unsafe_entry = LauncherFixture::new(repository_root);
    fs::create_dir_all(unsafe_entry.binary_path()).expect("unsafe binary directory");
    assert_launcher_failure(&unsafe_entry.run(&["--version"]), "safe regular file");

    let non_executable = LauncherFixture::new(repository_root);
    non_executable.write_binary(b"not executable", false);
    non_executable.rewrite_launcher(|source| {
        source.replace("if (process.platform !== \"win32\" &&", "if (true &&")
    });
    assert_launcher_failure(&non_executable.run(&["--version"]), "not executable");

    let unsupported = LauncherFixture::new(repository_root);
    unsupported.rewrite_launcher(|source| {
        source
            .replace("\"win32:x64\"", "\"fixture-win32:x64\"")
            .replace("\"linux:x64\"", "\"fixture-linux:x64\"")
    });
    assert_launcher_failure(&unsupported.run(&["--version"]), "unsupported platform");

    let cannot_start = LauncherFixture::new(repository_root);
    cannot_start.write_binary(b"not an executable image", true);
    assert_launcher_failure(&cannot_start.run(&["--version"]), "could not start");

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
            "ended by signal",
        );
    }
}

fn assert_launcher_failure(output: &Output, expected: &str) {
    assert_eq!(output.status.code(), Some(1), "{}", stderr(output));
    assert!(output.stdout.is_empty());
    let stderr = stderr(output).to_ascii_lowercase();
    assert!(
        stderr.contains(expected),
        "unexpected launcher error: {stderr}"
    );
    for guidance in ["reinstall", "supported", "windows x64", "linux x64"] {
        assert!(stderr.contains(guidance), "missing {guidance}: {stderr}");
    }
    for fallback in ["minimax-codex-legacy", "dist/cli.js", "src/cli.tsx"] {
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
