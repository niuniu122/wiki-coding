use minimax_protocol::ProjectId;
use minimax_vault::{ProjectVault, VaultError, VaultWarning, classify_vault_path};

fn project_id(value: &str) -> ProjectId {
    ProjectId::new(value).expect("project ID")
}

#[test]
fn bootstrap_is_fixed_idempotent_and_project_bound() {
    let project = tempfile::tempdir().expect("project");
    let vault_parent = tempfile::tempdir().expect("vault parent");
    let root = vault_parent.path().join("project.vault");
    let manifest_before = {
        let vault = ProjectVault::bootstrap(project.path(), &root, project_id("one"), 42)
            .expect("bootstrap");
        assert!(
            vault
                .warnings()
                .contains(&VaultWarning::PlaintextLocalFiles)
        );
        for path in [
            "AGENTS.md",
            "inbox",
            "raw/sessions",
            "raw/imports",
            "raw/assets",
            "wiki/index.md",
            "wiki/decisions",
            "log.md",
            ".minimax/manifest.json",
            ".minimax/transactions",
            ".minimax/trash",
        ] {
            assert!(root.join(path).exists(), "missing {path}");
        }
        std::fs::read(root.join(".minimax/manifest.json")).expect("manifest")
    };

    let reopened = ProjectVault::bootstrap(project.path(), &root, project_id("one"), 999)
        .expect("idempotent reopen");
    assert_eq!(reopened.manifest().created_at_unix_ms, 42);
    assert_eq!(
        std::fs::read(root.join(".minimax/manifest.json")).expect("manifest"),
        manifest_before
    );
    drop(reopened);
    assert!(matches!(
        ProjectVault::bootstrap(project.path(), &root, project_id("two"), 42),
        Err(VaultError::ProjectMismatch)
    ));
}

#[test]
fn second_writer_is_busy_and_human_guidance_is_not_schema() {
    let project = tempfile::tempdir().expect("project");
    let vault_parent = tempfile::tempdir().expect("vault parent");
    let root = vault_parent.path().join("vault");
    let first =
        ProjectVault::bootstrap(project.path(), &root, project_id("one"), 1).expect("first");
    std::fs::write(root.join("AGENTS.md"), "human edit\n").expect("human edit");
    assert!(matches!(
        ProjectVault::bootstrap(project.path(), &root, project_id("one"), 1),
        Err(VaultError::Busy)
    ));
    drop(first);
    ProjectVault::bootstrap(project.path(), &root, project_id("one"), 1).expect("reopen");
    assert_eq!(
        std::fs::read_to_string(root.join("AGENTS.md")).expect("guidance"),
        "human edit\n"
    );
}

#[test]
fn in_git_path_warns_without_editing_gitignore_and_ownership_is_explicit() {
    let project = tempfile::tempdir().expect("project");
    std::fs::create_dir(project.path().join(".git")).expect("git marker");
    std::fs::write(project.path().join(".gitignore"), "target/\n").expect("gitignore");
    let before = std::fs::read(project.path().join(".gitignore")).expect("before");
    let vault = ProjectVault::bootstrap(
        project.path(),
        project.path().join("notes"),
        project_id("inside"),
        1,
    )
    .expect("bootstrap");
    assert!(vault.warnings().contains(&VaultWarning::VaultInsideProject));
    assert!(
        vault
            .warnings()
            .contains(&VaultWarning::VaultInsideGitWorkTree)
    );
    assert_eq!(
        std::fs::read(project.path().join(".gitignore")).expect("after"),
        before
    );
    assert_eq!(
        classify_vault_path("inbox/note.md"),
        Some(minimax_protocol::VaultOwnership::Human)
    );
    assert_eq!(
        classify_vault_path("wiki/index.md"),
        Some(minimax_protocol::VaultOwnership::Agent)
    );
    assert_eq!(
        classify_vault_path(".minimax/manifest.json"),
        Some(minimax_protocol::VaultOwnership::Internal)
    );
    assert_eq!(classify_vault_path("elsewhere"), None);
}
