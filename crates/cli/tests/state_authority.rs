use std::collections::BTreeMap;
use std::future::Future;
use std::path::{Component, Path, PathBuf};
use std::pin::Pin;

use minimax_cli::{
    DriverIds, ProviderPort, RuntimeDriver, apply_migration, build_migration_plan,
    capability_search, capability_status, inspect, inventory_migration, project_search,
    project_status, resolve_project_vault, rollback_migration,
};
use minimax_protocol::{
    ModelBinding, ModelId, ProviderId, ProviderProtocolKind, RuntimeErrorCode, RuntimeFailure,
    StreamEvent,
};
use minimax_provider::CredentialError;
use minimax_vault::RuntimeStore;
use sha2::{Digest as _, Sha256};
use tokio_util::sync::CancellationToken;

type TreeSnapshot = BTreeMap<String, String>;

struct NeverProvider;

impl ProviderPort for NeverProvider {
    fn rebind(&mut self, _binding: &ModelBinding) {}

    fn stream<'a>(
        &'a mut self,
        _request: &'a minimax_protocol::TurnRequest,
        _cancellation: &'a CancellationToken,
        _emit: &'a mut (dyn FnMut(StreamEvent) + Send),
    ) -> Pin<Box<dyn Future<Output = Result<(), RuntimeFailure>> + Send + 'a>> {
        Box::pin(async {
            panic!("state-only session operations must not invoke a provider");
        })
    }
}

#[tokio::test]
async fn supported_state_paths_write_only_normalized_minimax_descendants() {
    let project = tempfile::tempdir().expect("project");
    let vault = tempfile::tempdir().expect("Vault");
    let empty = tree_snapshot(project.path());

    let _ = capability_status();
    let _ = capability_search("doctor", 3);
    let _ = project_status(None, None).expect("project index status");
    let _ = project_search(None, None, "terminal coding", 3)
        .await
        .expect("project index search");
    assert_eq!(
        tree_snapshot(project.path()),
        empty,
        "index reads wrote state"
    );

    let report = inspect(
        project.path(),
        Err(RuntimeErrorCode::Configuration),
        Err(CredentialError::Missing),
        false,
    );
    assert!(
        report
            .checks
            .iter()
            .any(|check| check.name == "runtime_index")
    );

    {
        let mut driver = RuntimeDriver::open(
            project.path(),
            binding(),
            NeverProvider,
            DriverIds::new("state-authority", 1_000),
        )
        .expect("session runtime");
        assert_eq!(driver.list_sessions().expect("list sessions").len(), 1);
        let second = driver.create_session(binding()).expect("create session");
        driver.resume(second).expect("resume session");
        assert_eq!(driver.list_sessions().expect("list two sessions").len(), 2);
    }

    let resolved = resolve_project_vault(
        project.path(),
        Some(vault.path()),
        Some("state-authority-project"),
        2_000,
    )
    .expect("Vault binding");
    assert_eq!(
        resolved.binding.vault_root,
        vault.path().canonicalize().expect("Vault root")
    );
    assert!(
        project
            .path()
            .join(".minimax/vault-binding.v1.json")
            .is_file()
    );
    assert!(!project.path().join(".mini-codex").exists());

    let after = tree_snapshot(project.path());
    assert_authority_delta(project.path(), &empty, &after);
}

#[test]
fn runtime_store_normalizes_an_aliased_project_root_before_writing() {
    let project = tempfile::tempdir().expect("project");
    let nested = project.path().join("nested");
    std::fs::create_dir_all(&nested).expect("nested directory");
    let aliased_project = nested.join("..");

    let store = RuntimeStore::open(&aliased_project).expect("runtime store");
    let journal = store.journal_path();
    assert!(journal.is_absolute());
    assert!(
        journal.starts_with(
            project
                .path()
                .canonicalize()
                .expect("project root")
                .join(".minimax")
        )
    );
    assert!(
        journal
            .components()
            .all(|component| !matches!(component, Component::CurDir | Component::ParentDir))
    );
}

#[test]
fn legacy_migration_is_read_only_at_source_and_receipt_scoped_at_target() {
    let project = tempfile::tempdir().expect("project");
    let source = project.path().join(".mini-codex");
    copy_directory(&fixture_root(), &source);
    let plan_directory = tempfile::tempdir().expect("plan directory");
    let before = tree_snapshot(project.path());
    let source_before = tree_snapshot(&source);

    let inventory = inventory_migration(&source, project.path()).expect("inventory");
    let first = build_migration_plan(&source, project.path()).expect("first dry run");
    let second = build_migration_plan(&source, project.path()).expect("second dry run");
    assert!(!inventory.targets.is_empty());
    assert_eq!(first, second, "dry-run plan is not deterministic");
    assert_eq!(
        tree_snapshot(project.path()),
        before,
        "read-only migration route wrote state"
    );
    assert_eq!(
        tree_snapshot(&source),
        source_before,
        "legacy source changed during read"
    );
    assert!(
        first
            .targets
            .iter()
            .all(|target| is_minimax_relative(Path::new(&target.relative_path)))
    );

    let migration_source = include_str!("../src/migration.rs");
    assert!(!migration_source.contains("std::process::Command"));
    assert!(!migration_source.contains("Command::new"));

    let plan_path = plan_directory.path().join("plan.json");
    std::fs::write(
        &plan_path,
        serde_json::to_vec_pretty(&first).expect("serialize plan"),
    )
    .expect("write plan");
    let receipt = apply_migration(&plan_path, &first.confirmation()).expect("apply");
    let applied = tree_snapshot(project.path());
    assert_authority_delta(project.path(), &before, &applied);
    assert_eq!(
        tree_snapshot(&source),
        source_before,
        "apply changed legacy source"
    );

    let sentinel = project.path().join(".minimax/authority-sentinel.txt");
    std::fs::write(&sentinel, b"not receipt owned\n").expect("sentinel");
    let receipt_path = project.path().join(format!(
        ".minimax/migrations/v1/{}/receipt.json",
        first.migration_id
    ));
    let rollback = rollback_migration(&receipt_path, &receipt.confirmation()).expect("rollback");
    assert!(rollback.rolled_back);
    assert_eq!(
        std::fs::read(&sentinel).expect("unowned sentinel survives"),
        b"not receipt owned\n"
    );
    for target in &receipt.created {
        assert!(
            !project.path().join(&target.relative_path).exists(),
            "receipt-owned target survived rollback: {}",
            target.relative_path
        );
    }
    assert_eq!(
        tree_snapshot(&source),
        source_before,
        "rollback changed legacy source"
    );
    assert_authority_delta(project.path(), &before, &tree_snapshot(project.path()));
}

fn binding() -> ModelBinding {
    ModelBinding {
        provider_id: ProviderId::new("state-authority").expect("provider ID"),
        model_id: ModelId::new("state-authority-model").expect("model ID"),
        protocol: ProviderProtocolKind::Responses,
    }
}

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("repository root")
        .join("fixtures/compat/migration/typescript-v1")
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

fn tree_snapshot(root: &Path) -> TreeSnapshot {
    let mut snapshot = TreeSnapshot::new();
    collect_entries(root, root, &mut snapshot);
    snapshot
}

fn collect_entries(root: &Path, directory: &Path, snapshot: &mut TreeSnapshot) {
    let mut entries = std::fs::read_dir(directory)
        .expect("read snapshot directory")
        .collect::<Result<Vec<_>, _>>()
        .expect("snapshot entries");
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .expect("snapshot relative path")
            .to_string_lossy()
            .replace('\\', "/");
        let file_type = entry.file_type().expect("snapshot file type");
        assert!(
            !file_type.is_symlink(),
            "snapshot contains a symlink: {relative}"
        );
        if file_type.is_dir() {
            snapshot.insert(relative, "directory".to_owned());
            collect_entries(root, &path, snapshot);
        } else {
            let digest = Sha256::digest(std::fs::read(path).expect("snapshot file"));
            let digest = digest
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            snapshot.insert(relative, format!("file:{digest}"));
        }
    }
}

fn assert_authority_delta(root: &Path, before: &TreeSnapshot, after: &TreeSnapshot) {
    let canonical_root = root.canonicalize().expect("canonical project root");
    let authority_root = canonical_root.join(".minimax");
    let changed = before
        .keys()
        .chain(after.keys())
        .filter(|path| before.get(*path) != after.get(*path))
        .collect::<std::collections::BTreeSet<_>>();
    assert!(
        !changed.is_empty(),
        "authority exercise produced no state changes"
    );
    for relative in changed {
        let path = Path::new(relative);
        assert!(
            is_minimax_relative(path),
            "state escaped .minimax authority: {relative}"
        );
        assert!(canonical_root.join(path).starts_with(&authority_root));
    }
}

fn is_minimax_relative(path: &Path) -> bool {
    !path.is_absolute()
        && path
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
        && matches!(
            path.components().next(),
            Some(Component::Normal(value)) if value == ".minimax"
        )
}
