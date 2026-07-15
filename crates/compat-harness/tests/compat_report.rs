use std::collections::BTreeSet;
use std::fs;

use minimax_compat_harness::{
    ArchitectureError, ArchitectureGraph, ArchitecturePackage, ManifestError, ParityStatus,
    build_report, load_cargo_architecture, load_compat_manifests, report_json, repository_root,
    validate_architecture, validate_core_source_boundary, validate_core_source_directory,
    validate_core_source_text, validate_report,
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
fn compat_report_contains_every_baseline_item_exactly_once() {
    let root = repository_root();
    let manifests = load_compat_manifests(&root).expect("strict manifests");
    let report = build_report(&manifests);
    let mut expected_ids = BTreeSet::from([
        "rust.permission_modes".to_owned(),
        "rust.product_entry".to_owned(),
    ]);
    for implementation in ["rust", "typescript"] {
        for command in &manifests.commands.commands {
            expected_ids.insert(format!("{implementation}.command.{}", command.name));
            for alias in &command.aliases {
                expected_ids.insert(format!("{implementation}.command.{alias}"));
            }
        }
        for profile in &manifests.providers.profile_classes {
            expected_ids.insert(format!("{implementation}.provider_profile.{}", profile.id));
        }
        for protocol in &manifests.providers.protocols {
            expected_ids.insert(format!("{implementation}.provider_protocol.{protocol}"));
        }
    }
    let report_ids = report
        .entries
        .iter()
        .map(|item| item.id.clone())
        .collect::<BTreeSet<_>>();

    assert_eq!(report.entries.len(), expected_ids.len());
    assert_eq!(report_ids, expected_ids);
    assert_eq!(manifests.commands.commands.len(), 17);
    assert_eq!(manifests.providers.profile_classes.len(), 3);
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
