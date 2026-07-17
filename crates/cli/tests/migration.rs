use std::fs::OpenOptions;
use std::path::{Component, Path, PathBuf};

use minimax_cli::migration::MigrationReceiptTarget;
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

fn assert_temp_contained(temp_root: &Path, candidate: &Path) {
    let canonical_root = temp_root.canonicalize().expect("canonical temp root");
    let relative = candidate
        .strip_prefix(temp_root)
        .expect("test path must be lexically below its temp root");
    let mut resolved = canonical_root.clone();
    for component in relative.components() {
        let Component::Normal(segment) = component else {
            panic!("test path must use normal components: {candidate:?}");
        };
        let next = resolved.join(segment);
        match std::fs::symlink_metadata(&next) {
            Ok(_) => {
                resolved = next
                    .canonicalize()
                    .expect("resolve existing test component");
                assert!(
                    resolved.starts_with(&canonical_root),
                    "resolved test path escaped its temp root: {candidate:?}"
                );
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => resolved = next,
            Err(error) => panic!("inspect test path {candidate:?}: {error}"),
        }
    }
    assert!(
        resolved.starts_with(&canonical_root),
        "future test path escaped its temp root: {candidate:?}"
    );
}

fn create_dir_all_checked(temp_root: &Path, path: &Path) {
    assert_temp_contained(temp_root, path);
    std::fs::create_dir_all(path).expect("create contained test directory");
    assert_temp_contained(temp_root, path);
}

fn write_checked(temp_root: &Path, path: &Path, bytes: impl AsRef<[u8]>) {
    assert_temp_contained(temp_root, path);
    if let Some(parent) = path.parent() {
        create_dir_all_checked(temp_root, parent);
    }
    std::fs::write(path, bytes).expect("write contained test file");
    assert_temp_contained(temp_root, path);
}

fn remove_file_checked(temp_root: &Path, path: &Path) {
    assert_temp_contained(temp_root, path);
    std::fs::remove_file(path).expect("remove contained test file");
}

fn copy_fixture_checked(temp_root: &Path, destination: &Path) {
    assert_temp_contained(temp_root, destination);
    copy_fixture(destination);
    assert_temp_contained(temp_root, destination);
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

fn write_plan_checked(temp_root: &Path, root: &Path, plan: &MigrationPlan) -> PathBuf {
    create_dir_all_checked(temp_root, root);
    let path = root.join("plan.json");
    write_checked(
        temp_root,
        &path,
        serde_json::to_vec_pretty(plan).expect("serialize checked plan"),
    );
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

fn assert_build_failure_preserves_everything(
    project: &Path,
    source: &Path,
    expected: MigrationError,
) {
    let source_before = tree_hash(source);
    let project_before = tree_hash(project);
    assert_eq!(build_migration_plan(source, project), Err(expected));
    assert_eq!(source_before, tree_hash(source));
    assert_eq!(project_before, tree_hash(project));
    assert!(!project.join(".minimax").exists());
}

#[cfg(unix)]
fn symlink_file(original: &Path, link: &Path) {
    std::os::unix::fs::symlink(original, link).expect("create fixture symlink");
}

#[cfg(windows)]
fn symlink_file(original: &Path, link: &Path) {
    std::os::windows::fs::symlink_file(original, link).expect("create fixture symlink");
}

#[cfg(unix)]
fn symlink_directory(original: &Path, link: &Path) {
    std::os::unix::fs::symlink(original, link).expect("create target directory symlink");
}

#[cfg(windows)]
fn symlink_directory(original: &Path, link: &Path) {
    std::os::windows::fs::symlink_dir(original, link)
        .expect("create target directory symlink; RED must not depend on setup failure");
}

fn collect_tree_snapshot(root: &Path, directory: &Path, entries: &mut Vec<(String, String)>) {
    assert_temp_contained(root, directory);
    let mut children = std::fs::read_dir(directory)
        .expect("snapshot directory")
        .collect::<Result<Vec<_>, _>>()
        .expect("snapshot entries");
    children.sort_by_key(std::fs::DirEntry::file_name);
    for child in children {
        let path = child.path();
        assert_temp_contained(root, &path);
        let relative = path
            .strip_prefix(root)
            .expect("snapshot relative path")
            .to_string_lossy()
            .replace('\\', "/");
        let metadata = std::fs::symlink_metadata(&path).expect("snapshot metadata");
        if metadata.file_type().is_symlink() {
            entries.push((
                relative,
                format!(
                    "link:{}",
                    std::fs::read_link(&path)
                        .expect("snapshot link target")
                        .to_string_lossy()
                ),
            ));
        } else if metadata.is_dir() {
            entries.push((relative, "directory".to_owned()));
            collect_tree_snapshot(root, &path, entries);
        } else {
            entries.push((relative, format!("file:{}", tree_file_hash(&path))));
        }
    }
}

fn tree_snapshot(root: &Path) -> Vec<(String, String)> {
    assert_temp_contained(root, root);
    let mut entries = Vec::new();
    collect_tree_snapshot(root, root, &mut entries);
    entries
}

fn tree_file_hash(path: &Path) -> String {
    let bytes = std::fs::read(path).expect("snapshot file bytes");
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn serializable_sha256(value: &impl serde::Serialize) -> String {
    Sha256::digest(serde_json::to_vec(value).expect("serialize integrity body"))
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ForgedOperationBody {
    schema_version: u16,
    migration_id: String,
    plan_hash: String,
    created: Vec<MigrationReceiptTarget>,
    reused: Vec<MigrationReceiptTarget>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ForgedOperation {
    schema_version: u16,
    migration_id: String,
    plan_hash: String,
    created: Vec<MigrationReceiptTarget>,
    reused: Vec<MigrationReceiptTarget>,
    operation_hash: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ForgedReceiptBody {
    schema_version: u16,
    migration_id: String,
    plan_hash: String,
    source_root: String,
    target_root: String,
    source_fingerprint: String,
    created: Vec<MigrationReceiptTarget>,
    reused: Vec<MigrationReceiptTarget>,
}

fn replace_first_jsonl_record(path: &Path, mutate: impl FnOnce(&mut serde_json::Value)) {
    let raw = std::fs::read_to_string(path).expect("JSONL fixture");
    let mut lines = raw.lines().map(str::to_owned).collect::<Vec<_>>();
    let mut first: serde_json::Value = serde_json::from_str(&lines[0]).expect("first JSONL row");
    mutate(&mut first);
    lines[0] = serde_json::to_string(&first).expect("serialize first JSONL row");
    std::fs::write(path, format!("{}\n", lines.join("\n"))).expect("rewrite JSONL fixture");
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
    assert_eq!(inventory.schema_version, 1);
    assert_eq!(inventory.target_schema, "minimax-rust-v1");
    assert_eq!(first.confirmation(), format!("MIGRATE:{}", first.plan_hash));
    assert_eq!(first.source_fingerprint.len(), 64);
    assert_eq!(first.plan_hash.len(), 64);
    assert!(first.included.iter().all(|item| item.sha256.len() == 64));
    assert!(first.targets.iter().all(|target| target.sha256.len() == 64));
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
fn file_aggregate_and_count_bounds_fail_before_any_target_mutation() {
    let oversized_project = tempfile::tempdir().expect("oversized project");
    let oversized_source = oversized_project.path().join(".mini-codex");
    copy_fixture(&oversized_source);
    OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(oversized_source.join("oversized.bin"))
        .expect("oversized fixture")
        .set_len(16 * 1024 * 1024 + 1)
        .expect("oversized length");
    assert_build_failure_preserves_everything(
        oversized_project.path(),
        &oversized_source,
        MigrationError::Bounds,
    );

    let aggregate_project = tempfile::tempdir().expect("aggregate project");
    let aggregate_source = aggregate_project.path().join(".mini-codex");
    copy_fixture(&aggregate_source);
    let aggregate = aggregate_source.join("aggregate");
    std::fs::create_dir_all(&aggregate).expect("aggregate directory");
    for index in 0..9 {
        OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(aggregate.join(format!("{index}.bin")))
            .expect("aggregate fixture")
            .set_len(16 * 1024 * 1024)
            .expect("aggregate length");
    }
    assert_build_failure_preserves_everything(
        aggregate_project.path(),
        &aggregate_source,
        MigrationError::Bounds,
    );

    let count_project = tempfile::tempdir().expect("count project");
    let count_source = count_project.path().join(".mini-codex");
    copy_fixture(&count_source);
    let many = count_source.join("many");
    std::fs::create_dir_all(&many).expect("many directory");
    for index in 0..9_991 {
        std::fs::write(many.join(format!("{index:05}.txt")), []).expect("count fixture");
    }
    assert_build_failure_preserves_everything(
        count_project.path(),
        &count_source,
        MigrationError::Bounds,
    );
}

#[test]
fn symlink_private_reasoning_and_newer_schema_fail_before_target_mutation() {
    let symlink_project = tempfile::tempdir().expect("symlink project");
    let symlink_source = symlink_project.path().join(".mini-codex");
    copy_fixture(&symlink_source);
    let outside = symlink_project.path().join("outside-config.json");
    std::fs::copy(symlink_source.join("config.json"), &outside).expect("outside config");
    std::fs::remove_file(symlink_source.join("config.json")).expect("remove source config");
    symlink_file(&outside, &symlink_source.join("config.json"));
    assert_build_failure_preserves_everything(
        symlink_project.path(),
        &symlink_source,
        MigrationError::Symlink,
    );

    let reasoning_project = tempfile::tempdir().expect("reasoning project");
    let reasoning_source = reasoning_project.path().join(".mini-codex");
    copy_fixture(&reasoning_source);
    replace_first_jsonl_record(
        &reasoning_source.join("sessions/2026/07/15/thread-safe.jsonl"),
        |record| {
            record["payload"]["privateReasoning"] =
                serde_json::json!("PRIVATE_REASONING must never migrate");
        },
    );
    assert_build_failure_preserves_everything(
        reasoning_project.path(),
        &reasoning_source,
        MigrationError::Secret,
    );

    let schema_project = tempfile::tempdir().expect("schema project");
    let schema_source = schema_project.path().join(".mini-codex");
    copy_fixture(&schema_source);
    let config_path = schema_source.join("config.json");
    let mut config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&config_path).expect("config bytes"))
            .expect("config JSON");
    config["schemaVersion"] = serde_json::json!(2);
    std::fs::write(
        &config_path,
        serde_json::to_vec_pretty(&config).expect("config bytes"),
    )
    .expect("write config");
    assert_build_failure_preserves_everything(
        schema_project.path(),
        &schema_source,
        MigrationError::Malformed,
    );
}

#[test]
fn malformed_known_jsonl_and_secret_bearing_required_fields_fail_closed() {
    for relative in [
        "sessions/2026/07/15/thread-safe.jsonl",
        "turns/thread-safe.turns.jsonl",
    ] {
        let project = tempfile::tempdir().expect("malformed project");
        let source = project.path().join(".mini-codex");
        copy_fixture(&source);
        std::fs::write(source.join(relative), "{bad json\n").expect("malformed JSONL");
        assert_build_failure_preserves_everything(
            project.path(),
            &source,
            MigrationError::Malformed,
        );
    }

    let secret_project = tempfile::tempdir().expect("secret project");
    let secret_source = secret_project.path().join(".mini-codex");
    copy_fixture(&secret_source);
    let config_path = secret_source.join("config.json");
    let mut config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&config_path).expect("config bytes"))
            .expect("config JSON");
    config["model"] = serde_json::json!("sk-secret-bearing-model-value");
    std::fs::write(
        &config_path,
        serde_json::to_vec_pretty(&config).expect("config bytes"),
    )
    .expect("write config");
    assert_build_failure_preserves_everything(
        secret_project.path(),
        &secret_source,
        MigrationError::Secret,
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
fn verify_reports_source_drift_and_rejects_target_drift_without_writing() {
    let project = tempfile::tempdir().expect("project");
    let source = project.path().join(".mini-codex");
    copy_fixture(&source);
    let plan = build_migration_plan(&source, project.path()).expect("plan");
    let plan_path = write_plan(project.path(), &plan);
    apply_migration(&plan_path, &plan.confirmation()).expect("apply");
    let receipt_path = project.path().join(format!(
        ".minimax/migrations/v1/{}/receipt.json",
        plan.migration_id
    ));

    let drift = source.join("new-unsupported.txt");
    std::fs::write(&drift, "source drift").expect("source drift");
    let before_verify = tree_hash(project.path());
    let report = verify_migration(&receipt_path).expect("source drift report");
    assert!(!report.source_unchanged);
    assert_eq!(report.targets_verified, 3);
    assert_eq!(before_verify, tree_hash(project.path()));

    std::fs::remove_file(drift).expect("restore source");
    let target = project.path().join(".minimax/config.json");
    std::fs::write(&target, "target drift").expect("target drift");
    let before_failed_verify = tree_hash(project.path());
    assert_eq!(
        verify_migration(&receipt_path),
        Err(MigrationError::TargetChanged)
    );
    assert_eq!(before_failed_verify, tree_hash(project.path()));
}

#[test]
fn recomputed_forged_receipt_cannot_claim_an_unowned_project_file() {
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct ReceiptBody {
        schema_version: u16,
        migration_id: String,
        plan_hash: String,
        source_root: String,
        target_root: String,
        source_fingerprint: String,
        created: Vec<MigrationReceiptTarget>,
        reused: Vec<MigrationReceiptTarget>,
    }

    let project = tempfile::tempdir().expect("project");
    let source = project.path().join(".mini-codex");
    copy_fixture(&source);
    let source_before = tree_hash(&source);
    let plan = build_migration_plan(&source, project.path()).expect("plan");
    let plan_path = write_plan(project.path(), &plan);
    let mut receipt = apply_migration(&plan_path, &plan.confirmation()).expect("apply");
    let receipt_path = project.path().join(format!(
        ".minimax/migrations/v1/{}/receipt.json",
        plan.migration_id
    ));

    let unowned_path = project.path().join("unowned-project-file.txt");
    let unowned_bytes = b"receipt must not claim this project file";
    std::fs::write(&unowned_path, unowned_bytes).expect("unowned file");
    let mut digest = Sha256::new();
    digest.update(unowned_bytes);
    let unowned_sha256 = digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    receipt.created = vec![MigrationReceiptTarget {
        relative_path: "unowned-project-file.txt".to_owned(),
        sha256: unowned_sha256,
        bytes: unowned_bytes.len() as u64,
    }];
    receipt.reused.clear();
    let body = ReceiptBody {
        schema_version: receipt.schema_version,
        migration_id: receipt.migration_id.clone(),
        plan_hash: receipt.plan_hash.clone(),
        source_root: receipt.source_root.clone(),
        target_root: receipt.target_root.clone(),
        source_fingerprint: receipt.source_fingerprint.clone(),
        created: receipt.created.clone(),
        reused: receipt.reused.clone(),
    };
    receipt.receipt_hash = {
        let mut digest = Sha256::new();
        digest.update(serde_json::to_vec(&body).expect("receipt body"));
        digest
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    };
    std::fs::write(
        &receipt_path,
        serde_json::to_vec_pretty(&receipt).expect("forged receipt"),
    )
    .expect("write forged receipt");

    assert_eq!(
        rollback_migration(&receipt_path, &receipt.confirmation()),
        Err(MigrationError::Receipt)
    );
    assert_eq!(
        std::fs::read(&unowned_path).expect("unowned file survives"),
        unowned_bytes
    );
    assert_eq!(source_before, tree_hash(&source));
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

#[test]
fn forged_operation_manifest_cannot_delete_unowned_project_files() {
    let project = tempfile::tempdir().expect("project");
    let source = project.path().join(".mini-codex");
    copy_fixture(&source);
    let source_before = tree_hash(&source);
    let plan = build_migration_plan(&source, project.path()).expect("plan");
    let plan_path = write_plan(project.path(), &plan);
    apply_migration(&plan_path, &plan.confirmation()).expect("first apply");
    let migration_root = project
        .path()
        .join(format!(".minimax/migrations/v1/{}", plan.migration_id));
    std::fs::remove_file(migration_root.join("receipt.json")).expect("simulate lost receipt");

    let unowned_path = project.path().join("unowned-project-file.txt");
    let unowned_bytes = b"must survive forged recovery";
    std::fs::write(&unowned_path, unowned_bytes).expect("unowned project file");
    let mut unowned_digest = Sha256::new();
    unowned_digest.update(unowned_bytes);
    let unowned_sha256 = unowned_digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let operation = migration_root.join("operation");
    std::fs::create_dir_all(&operation).expect("operation directory");
    std::fs::write(
        operation.join("operation.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "schemaVersion": 1,
            "planHash": "0".repeat(64),
            "candidates": [{
                "relativePath": "unowned-project-file.txt",
                "sha256": unowned_sha256,
                "bytes": unowned_bytes.len()
            }]
        }))
        .expect("forged operation manifest"),
    )
    .expect("write forged operation manifest");

    assert_eq!(
        apply_migration(&plan_path, &plan.confirmation()),
        Err(MigrationError::Recovery)
    );
    assert_eq!(
        std::fs::read(&unowned_path).expect("unowned file survives"),
        unowned_bytes
    );
    assert_eq!(source_before, tree_hash(&source));
}

#[test]
fn rollback_preserves_reused_targets_receipts_and_user_changes() {
    let seed_project = tempfile::tempdir().expect("seed project");
    let seed_source = seed_project.path().join(".mini-codex");
    copy_fixture(&seed_source);
    let seed_plan = build_migration_plan(&seed_source, seed_project.path()).expect("seed plan");
    let seed_plan_path = write_plan(seed_project.path(), &seed_plan);
    apply_migration(&seed_plan_path, &seed_plan.confirmation()).expect("seed apply");
    let reusable_config =
        std::fs::read(seed_project.path().join(".minimax/config.json")).expect("seed config");

    let project = tempfile::tempdir().expect("project");
    let source = project.path().join(".mini-codex");
    copy_fixture(&source);
    let source_before = tree_hash(&source);
    std::fs::create_dir_all(project.path().join(".minimax")).expect("target root");
    std::fs::write(
        project.path().join(".minimax/config.json"),
        &reusable_config,
    )
    .expect("reused config");
    let plan = build_migration_plan(&source, project.path()).expect("plan");
    assert!(plan.collisions.is_empty());
    let plan_path = write_plan(project.path(), &plan);
    let receipt = apply_migration(&plan_path, &plan.confirmation()).expect("apply");
    assert_eq!(receipt.reused.len(), 1);
    assert_eq!(receipt.created.len(), 2);
    let receipt_path = project.path().join(format!(
        ".minimax/migrations/v1/{}/receipt.json",
        plan.migration_id
    ));

    let report = rollback_migration(&receipt_path, &receipt.confirmation()).expect("rollback");
    assert!(report.rolled_back);
    assert_eq!(report.targets_verified, 1);
    assert_eq!(
        std::fs::read(project.path().join(".minimax/config.json")).expect("reused target retained"),
        reusable_config
    );
    assert!(receipt_path.is_file());
    assert_eq!(source_before, tree_hash(&source));
    assert!(
        !project
            .path()
            .join(".minimax/runtime/v1/sessions.jsonl")
            .exists()
    );
}

#[test]
fn gap_closure_forged_created_claims_cannot_delete_preexisting_allowlisted_targets() {
    let outer = tempfile::tempdir().expect("outer safety root");
    let outer_root = outer.path();
    let seed_project = outer_root.join("seed-project");
    create_dir_all_checked(outer_root, &seed_project);
    let seed_source = seed_project.join(".mini-codex");
    copy_fixture_checked(outer_root, &seed_source);
    let seed_plan = build_migration_plan(&seed_source, &seed_project).expect("seed plan");
    let seed_plan_path = write_plan_checked(outer_root, &outer_root.join("seed-plan"), &seed_plan);
    apply_migration(&seed_plan_path, &seed_plan.confirmation()).expect("seed apply");

    for (index, seed_target) in seed_plan.targets.iter().enumerate() {
        let case_root = outer_root.join(format!("forged-case-{index}"));
        let project = case_root.join("project");
        create_dir_all_checked(outer_root, &project);
        let source = project.join(".mini-codex");
        copy_fixture_checked(outer_root, &source);
        let source_before = tree_hash(&source);

        let seed_path = seed_project.join(&seed_target.relative_path);
        assert_temp_contained(outer_root, &seed_path);
        let expected_bytes = std::fs::read(&seed_path).expect("canonical seed artifact");
        let target_path = project.join(&seed_target.relative_path);
        write_checked(outer_root, &target_path, &expected_bytes);

        let plan = build_migration_plan(&source, &project).expect("reused-target plan");
        assert!(plan.collisions.is_empty());
        let plan_path = write_plan_checked(outer_root, &case_root.join("input"), &plan);
        let legitimate =
            apply_migration(&plan_path, &plan.confirmation()).expect("legitimate apply");
        let reused_target = legitimate
            .reused
            .iter()
            .find(|target| target.relative_path == seed_target.relative_path)
            .cloned()
            .expect("pre-existing target must be classified as reused");

        let migration_root = project.join(format!(".minimax/migrations/v1/{}", plan.migration_id));
        let durable_plan_path = migration_root.join("plan.json");
        if !durable_plan_path.exists() {
            write_checked(
                outer_root,
                &durable_plan_path,
                serde_json::to_vec_pretty(&plan).expect("durable plan snapshot"),
            );
        }

        let mut forged_created = legitimate.created.clone();
        forged_created.push(reused_target.clone());
        forged_created.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
        let mut forged_reused = legitimate
            .reused
            .iter()
            .filter(|target| target.relative_path != reused_target.relative_path)
            .cloned()
            .collect::<Vec<_>>();
        forged_reused.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));

        let operation_body = ForgedOperationBody {
            schema_version: 1,
            migration_id: plan.migration_id.clone(),
            plan_hash: plan.plan_hash.clone(),
            created: forged_created.clone(),
            reused: forged_reused.clone(),
        };
        let operation = ForgedOperation {
            schema_version: operation_body.schema_version,
            migration_id: operation_body.migration_id.clone(),
            plan_hash: operation_body.plan_hash.clone(),
            created: operation_body.created.clone(),
            reused: operation_body.reused.clone(),
            operation_hash: serializable_sha256(&operation_body),
        };
        let operation_path = migration_root.join("operation.json");
        if operation_path.exists() {
            remove_file_checked(outer_root, &operation_path);
        }
        write_checked(
            outer_root,
            &operation_path,
            serde_json::to_vec_pretty(&operation).expect("forged operation"),
        );

        let receipt_body = ForgedReceiptBody {
            schema_version: legitimate.schema_version,
            migration_id: legitimate.migration_id.clone(),
            plan_hash: legitimate.plan_hash.clone(),
            source_root: legitimate.source_root.clone(),
            target_root: legitimate.target_root.clone(),
            source_fingerprint: legitimate.source_fingerprint.clone(),
            created: forged_created,
            reused: forged_reused,
        };
        let forged_receipt = MigrationReceipt {
            schema_version: receipt_body.schema_version,
            migration_id: receipt_body.migration_id.clone(),
            plan_hash: receipt_body.plan_hash.clone(),
            source_root: receipt_body.source_root.clone(),
            target_root: receipt_body.target_root.clone(),
            source_fingerprint: receipt_body.source_fingerprint.clone(),
            created: receipt_body.created.clone(),
            reused: receipt_body.reused.clone(),
            receipt_hash: serializable_sha256(&receipt_body),
        };
        let receipt_path = migration_root.join("receipt.json");
        remove_file_checked(outer_root, &receipt_path);
        write_checked(
            outer_root,
            &receipt_path,
            serde_json::to_vec_pretty(&forged_receipt).expect("forged receipt"),
        );

        assert_temp_contained(outer_root, &receipt_path);
        assert_temp_contained(outer_root, &operation_path);
        assert_temp_contained(outer_root, &target_path);
        let verify_result = verify_migration(&receipt_path);
        let rollback_result = rollback_migration(&receipt_path, &forged_receipt.confirmation());

        assert_eq!(
            std::fs::read(&target_path).ok().as_deref(),
            Some(expected_bytes.as_slice()),
            "forged ownership deleted pre-existing target {}",
            seed_target.relative_path
        );
        assert!(
            matches!(
                verify_result,
                Err(MigrationError::Receipt | MigrationError::Recovery)
            ),
            "verify accepted forged ownership for {}: {verify_result:?}",
            seed_target.relative_path
        );
        assert!(
            matches!(
                rollback_result,
                Err(MigrationError::Receipt | MigrationError::Recovery)
            ),
            "rollback accepted forged ownership for {}: {rollback_result:?}",
            seed_target.relative_path
        );
        let plan_json = serde_json::to_value(&plan).expect("plan JSON");
        let planned_target = plan_json["targets"]
            .as_array()
            .expect("plan targets")
            .iter()
            .find(|target| target["relativePath"] == seed_target.relative_path)
            .expect("planned reused target");
        assert_eq!(
            planned_target["preWriteDisposition"], "byte-identical-existing",
            "dry-run must durably classify the pre-existing target"
        );
        assert_eq!(source_before, tree_hash(&source));
    }
}

#[test]
fn gap_closure_target_ancestor_symlinks_fail_before_any_external_write() {
    for (index, ancestor) in [".minimax", ".minimax/runtime"].iter().enumerate() {
        let outer = tempfile::tempdir().expect("outer safety root");
        let outer_root = outer.path();
        let project = outer_root.join(format!("project-{index}"));
        let external = outer_root.join(format!("external-{index}"));
        create_dir_all_checked(outer_root, &project);
        create_dir_all_checked(outer_root, &external);
        let source = project.join(".mini-codex");
        copy_fixture_checked(outer_root, &source);
        let source_before = tree_hash(&source);

        let link = project.join(ancestor);
        create_dir_all_checked(outer_root, link.parent().expect("target ancestor parent"));
        assert_temp_contained(outer_root, &external);
        assert_temp_contained(outer_root, &link);
        symlink_directory(&external, &link);
        assert_temp_contained(outer_root, &link);
        let external_before = tree_snapshot(&external);

        let outcome = match build_migration_plan(&source, &project) {
            Ok(plan) => {
                let plan_path = write_plan_checked(
                    outer_root,
                    &outer_root.join(format!("plan-{index}")),
                    &plan,
                );
                apply_migration(&plan_path, &plan.confirmation()).map(|_| ())
            }
            Err(error) => Err(error),
        };

        assert_eq!(source_before, tree_hash(&source));
        assert_eq!(
            external_before,
            tree_snapshot(&external),
            "target ancestor {ancestor} received lock, staging, record, or artifact bytes"
        );
        assert!(
            matches!(
                outcome,
                Err(MigrationError::Symlink | MigrationError::Target)
            ),
            "target ancestor {ancestor} did not fail closed: {outcome:?}"
        );
        if *ancestor != ".minimax" {
            assert!(
                !project.join(".minimax/migrations").exists(),
                "migration records were created before nested symlink rejection"
            );
        }
    }
}

#[test]
fn migration_implementation_has_no_network_provider_or_credential_read_path() {
    let source = include_str!("../src/migration.rs");
    for forbidden in [
        "std::net",
        "TcpStream",
        "UdpSocket",
        "reqwest",
        "minimax_provider",
        "keyring::",
        "std::env::var",
        "Command::new",
    ] {
        assert!(
            !source.contains(forbidden),
            "migration must not contain runtime authority: {forbidden}"
        );
    }
}
