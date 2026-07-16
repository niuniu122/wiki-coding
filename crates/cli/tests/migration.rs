use std::path::{Path, PathBuf};

use minimax_cli::{
    MigrationError, MigrationPlan, MigrationReceipt, apply_migration, build_migration_plan,
    inventory_migration, rollback_migration, verify_migration,
};
use minimax_core::SessionMachine;
use minimax_protocol::SessionRecordV1;
use sha2::{Digest as _, Sha256};

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("repository root")
        .join("fixtures/compat/migration/typescript-v1")
}

fn copy_fixture(destination: &Path) {
    copy_directory(&fixture_root(), destination);
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

fn write_plan(root: &Path, plan: &MigrationPlan) -> PathBuf {
    std::fs::create_dir_all(root).expect("plan directory");
    let path = root.join("plan.json");
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(plan).expect("serialize plan"),
    )
    .expect("write plan");
    path
}

fn tree_hash(root: &Path) -> String {
    let mut files = Vec::new();
    collect_files(root, root, &mut files);
    files.sort();
    let mut digest = Sha256::new();
    for path in files {
        let relative = path.strip_prefix(root).expect("relative path");
        digest.update(relative.to_string_lossy().replace('\\', "/").as_bytes());
        digest.update([0]);
        digest.update(std::fs::read(path).expect("file bytes"));
        digest.update(*b"\n");
    }
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn collect_files(root: &Path, directory: &Path, files: &mut Vec<PathBuf>) {
    assert!(directory.starts_with(root));
    for entry in std::fs::read_dir(directory).expect("read directory") {
        let entry = entry.expect("directory entry");
        if entry.file_type().expect("file type").is_dir() {
            collect_files(root, &entry.path(), files);
        } else {
            files.push(entry.path());
        }
    }
}

#[test]
fn inventory_and_dry_run_are_deterministic_and_write_nothing() {
    let project = tempfile::tempdir().expect("project");
    let source = project.path().join(".mini-codex");
    copy_fixture(&source);
    let before = tree_hash(project.path());

    let first = build_migration_plan(&source, project.path()).expect("first plan");
    let inventory = inventory_migration(&source, project.path()).expect("inventory");
    let second = build_migration_plan(&source, project.path()).expect("second plan");

    assert_eq!(first, second);
    assert_eq!(before, tree_hash(project.path()));
    assert!(inventory.collisions.is_empty());
    assert_eq!(inventory.targets.len(), 3);
    assert!(
        inventory
            .excluded
            .iter()
            .any(|item| item.reason == "secret_path")
    );
    assert!(
        inventory
            .excluded
            .iter()
            .any(|item| item.reason == "private_trace")
    );
    assert!(
        inventory
            .excluded
            .iter()
            .any(|item| item.reason == "derived_data")
    );
    assert!(
        inventory
            .excluded
            .iter()
            .any(|item| item.relative_path == "record:turn-secret:user_message")
    );
}

#[test]
fn transaction_imports_safe_data_replays_and_is_idempotent() {
    let project = tempfile::tempdir().expect("project");
    let source = project.path().join(".mini-codex");
    copy_fixture(&source);
    let source_before = tree_hash(&source);
    let plan = build_migration_plan(&source, project.path()).expect("plan");
    let plan_path = write_plan(project.path(), &plan);

    let receipt = apply_migration(&plan_path, &plan.confirmation()).expect("apply");
    assert_eq!(source_before, tree_hash(&source));
    assert_eq!(receipt.created.len(), 3);
    assert!(receipt.reused.is_empty());
    let repeated = apply_migration(&plan_path, &plan.confirmation()).expect("idempotent apply");
    assert_eq!(repeated, receipt);

    let receipt_path = project.path().join(format!(
        ".minimax/migrations/v1/{}/receipt.json",
        plan.migration_id
    ));
    let verified = verify_migration(&receipt_path).expect("verify");
    assert!(verified.source_unchanged);
    assert_eq!(verified.targets_verified, 3);

    let config =
        std::fs::read_to_string(project.path().join(".minimax/config.json")).expect("Rust config");
    assert!(config.contains("MINIMAX_API_KEY"));
    assert!(!config.contains("sk-fixture"));
    assert!(!config.contains("Authorization"));
    let journal =
        std::fs::read_to_string(project.path().join(".minimax/runtime/v1/sessions.jsonl"))
            .expect("Rust journal");
    assert!(journal.contains("List the project files safely"));
    assert!(journal.contains("list_files"));
    assert!(!journal.contains("turn-secret"));
    assert!(!journal.contains("PRIVATE_REASONING"));
    let records = journal
        .lines()
        .map(|line| serde_json::from_str::<SessionRecordV1>(line).expect("session record"))
        .collect::<Vec<_>>();
    SessionMachine::replay(records).expect("replay imported journal");
}

#[test]
fn rollback_removes_only_created_unchanged_targets_and_keeps_receipt() {
    let project = tempfile::tempdir().expect("project");
    let source = project.path().join(".mini-codex");
    copy_fixture(&source);
    let source_before = tree_hash(&source);
    let plan = build_migration_plan(&source, project.path()).expect("plan");
    let plan_path = write_plan(project.path(), &plan);
    let receipt = apply_migration(&plan_path, &plan.confirmation()).expect("apply");
    let receipt_path = project.path().join(format!(
        ".minimax/migrations/v1/{}/receipt.json",
        plan.migration_id
    ));

    let report = rollback_migration(&receipt_path, &receipt.confirmation()).expect("rollback");
    assert!(report.rolled_back);
    assert_eq!(source_before, tree_hash(&source));
    assert!(receipt_path.is_file());
    assert!(!project.path().join(".minimax/config.json").exists());
    assert!(
        !project
            .path()
            .join(".minimax/runtime/v1/sessions.jsonl")
            .exists()
    );
    assert!(
        project
            .path()
            .join(format!(
                ".minimax/migrations/v1/{}/rollback.json",
                plan.migration_id
            ))
            .is_file()
    );
    assert_eq!(
        rollback_migration(&receipt_path, &receipt.confirmation()).expect("repeat rollback"),
        report
    );
}

#[test]
fn collision_drift_forgery_and_changed_target_fail_closed() {
    let project = tempfile::tempdir().expect("project");
    let source = project.path().join(".mini-codex");
    copy_fixture(&source);
    let plan = build_migration_plan(&source, project.path()).expect("plan");
    let plan_path = write_plan(project.path(), &plan);
    assert_eq!(
        apply_migration(&plan_path, "MIGRATE:wrong"),
        Err(MigrationError::Confirmation)
    );

    let mut forged = plan.clone();
    forged.target_schema = "forged".to_owned();
    let forged_path = write_plan(&project.path().join("forged"), &forged);
    assert_eq!(
        apply_migration(&forged_path, &plan.confirmation()),
        Err(MigrationError::Plan)
    );

    std::fs::write(source.join("new-unsupported.txt"), "drift").expect("source drift");
    assert_eq!(
        apply_migration(&plan_path, &plan.confirmation()),
        Err(MigrationError::Drift)
    );

    let collision_project = tempfile::tempdir().expect("collision project");
    let collision_source = collision_project.path().join(".mini-codex");
    copy_fixture(&collision_source);
    std::fs::create_dir_all(collision_project.path().join(".minimax")).expect("target directory");
    std::fs::write(
        collision_project.path().join(".minimax/config.json"),
        "different",
    )
    .expect("collision");
    let collision =
        build_migration_plan(&collision_source, collision_project.path()).expect("collision plan");
    assert_eq!(collision.collisions.len(), 1);
    let collision_path = write_plan(collision_project.path(), &collision);
    assert_eq!(
        apply_migration(&collision_path, &collision.confirmation()),
        Err(MigrationError::Collision)
    );

    let changed_project = tempfile::tempdir().expect("changed project");
    let changed_source = changed_project.path().join(".mini-codex");
    copy_fixture(&changed_source);
    let changed_plan = build_migration_plan(&changed_source, changed_project.path()).expect("plan");
    let changed_plan_path = write_plan(changed_project.path(), &changed_plan);
    let receipt = apply_migration(&changed_plan_path, &changed_plan.confirmation()).expect("apply");
    let receipt_path = changed_project.path().join(format!(
        ".minimax/migrations/v1/{}/receipt.json",
        changed_plan.migration_id
    ));
    std::fs::write(
        changed_project.path().join(".minimax/config.json"),
        "user change",
    )
    .expect("change target");
    assert_eq!(
        rollback_migration(&receipt_path, &receipt.confirmation()),
        Err(MigrationError::TargetChanged)
    );
    assert!(
        changed_project
            .path()
            .join(".minimax/config.json")
            .is_file()
    );
}

#[test]
fn malformed_known_input_is_rejected_instead_of_silently_skipped() {
    let project = tempfile::tempdir().expect("project");
    let source = project.path().join(".mini-codex");
    copy_fixture(&source);
    std::fs::write(source.join("indexes/threads.json"), "{bad json").expect("malformed index");
    assert_eq!(
        build_migration_plan(&source, project.path()),
        Err(MigrationError::Malformed)
    );
}

#[test]
fn receipt_hash_is_required_and_forgery_is_detected() {
    let project = tempfile::tempdir().expect("project");
    let source = project.path().join(".mini-codex");
    copy_fixture(&source);
    let plan = build_migration_plan(&source, project.path()).expect("plan");
    let plan_path = write_plan(project.path(), &plan);
    let receipt = apply_migration(&plan_path, &plan.confirmation()).expect("apply");
    let receipt_path = project.path().join(format!(
        ".minimax/migrations/v1/{}/receipt.json",
        plan.migration_id
    ));
    assert_eq!(
        rollback_migration(&receipt_path, "ROLLBACK:wrong"),
        Err(MigrationError::Confirmation)
    );
    let mut forged: MigrationReceipt =
        serde_json::from_slice(&std::fs::read(&receipt_path).expect("receipt bytes"))
            .expect("receipt");
    forged.source_fingerprint = "forged".to_owned();
    let forged_path = project.path().join("forged-receipt.json");
    std::fs::write(
        &forged_path,
        serde_json::to_vec_pretty(&forged).expect("forged receipt"),
    )
    .expect("write forged receipt");
    assert_eq!(verify_migration(&forged_path), Err(MigrationError::Receipt));
    assert!(receipt.receipt_hash.len() == 64);
}

#[test]
fn interrupted_operation_manifest_recovers_only_exact_claimed_targets() {
    let project = tempfile::tempdir().expect("project");
    let source = project.path().join(".mini-codex");
    copy_fixture(&source);
    let plan = build_migration_plan(&source, project.path()).expect("plan");
    let plan_path = write_plan(project.path(), &plan);
    let first = apply_migration(&plan_path, &plan.confirmation()).expect("first apply");
    let migration_root = project
        .path()
        .join(format!(".minimax/migrations/v1/{}", plan.migration_id));
    std::fs::remove_file(migration_root.join("receipt.json")).expect("simulate lost receipt");
    let operation = migration_root.join("operation");
    std::fs::create_dir_all(&operation).expect("operation directory");
    std::fs::write(
        operation.join("operation.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "schemaVersion": 1,
            "planHash": plan.plan_hash,
            "candidates": first.created
        }))
        .expect("operation manifest"),
    )
    .expect("write operation manifest");

    let recovered = apply_migration(&plan_path, &plan.confirmation()).expect("recovered apply");
    assert_eq!(recovered.created.len(), 3);
    assert!(migration_root.join("receipt.json").is_file());
    assert!(!operation.exists());
}
