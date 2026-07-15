use std::collections::BTreeSet;
use std::fs;

use minimax_compat_harness::{
    ArchitectureError, ArchitectureGraph, ArchitecturePackage, ManifestError, ParityStatus,
    build_report, load_cargo_architecture, load_compat_manifests, report_json, repository_root,
    validate_architecture, validate_report,
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
    assert_eq!(first_json, expected);
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
}

#[test]
fn architecture_rejects_core_to_vault() {
    let graph = synthetic_graph(&[("minimax-core", &["minimax-vault"])]);
    assert_eq!(
        validate_architecture(&graph),
        Err(ArchitectureError::Violation(
            "forbidden local dependency: minimax-core -> minimax-vault".to_owned()
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
    let mut graph = synthetic_graph(&[("minimax-protocol", &[])]);
    graph.packages.push(ArchitecturePackage {
        name: "rusqlite".to_owned(),
        local: false,
        dependencies: Vec::new(),
    });
    assert_eq!(
        validate_architecture(&graph),
        Err(ArchitectureError::Violation(
            "database dependency denied: rusqlite".to_owned()
        ))
    );
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
