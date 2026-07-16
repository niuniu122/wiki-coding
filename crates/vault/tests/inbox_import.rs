use minimax_protocol::{InboxImportStatus, ProjectId, TransactionId};
use minimax_vault::{
    PreparedWikiTransaction, ProjectVault, WikiChange, complete_inbox_import, hash_vault_bytes,
    import_inbox_file,
};

fn vault() -> (tempfile::TempDir, tempfile::TempDir, ProjectVault) {
    let project = tempfile::tempdir().expect("project");
    let root = tempfile::tempdir().expect("vault");
    let vault = ProjectVault::bootstrap(
        project.path(),
        root.path(),
        ProjectId::new("project").expect("project ID"),
        1,
    )
    .expect("bootstrap");
    (project, root, vault)
}

fn committed_transaction(vault: &ProjectVault, value: &str) -> TransactionId {
    let path = vault.root().join("wiki/index.md");
    let before = std::fs::read(&path).expect("index");
    let transaction_id = TransactionId::new(value).expect("transaction ID");
    PreparedWikiTransaction::prepare(
        vault,
        transaction_id.clone(),
        vec![WikiChange {
            relative_path: "wiki/index.md".to_owned(),
            expected_old_hash: Some(hash_vault_bytes(&before)),
            bytes: format!("# Project Wiki\n\n{value}\n").into_bytes(),
        }],
        2,
    )
    .expect("prepare")
    .roll_forward()
    .expect("commit");
    transaction_id
}

#[test]
fn unicode_text_import_is_content_addressed_repeatable_and_removed_after_commit() {
    let (_project, _root, vault) = vault();
    let origin = vault.root().join("inbox/江南.md");
    std::fs::write(&origin, "路明非决定保留证据。\n").expect("inbox");
    let first = import_inbox_file(&vault, "inbox/江南.md", 10).expect("import");
    let second = import_inbox_file(&vault, "inbox/江南.md", 10).expect("repeat");
    assert_eq!(first, second);
    assert_eq!(first.status, InboxImportStatus::ImportedSourceRetained);
    assert!(origin.is_file());
    assert_eq!(
        std::fs::read(vault.root().join(&first.imported_relative_path)).expect("raw"),
        "路明非决定保留证据。\n".as_bytes()
    );
    let transaction_id = committed_transaction(&vault, "tx-import");
    let completed =
        complete_inbox_import(&vault, &first.content_hash, &transaction_id).expect("complete");
    assert_eq!(completed.status, InboxImportStatus::CompiledSourceRemoved);
    assert!(!origin.exists());
    assert_eq!(
        complete_inbox_import(&vault, &first.content_hash, &transaction_id).expect("idempotent"),
        completed
    );
}

#[test]
fn empty_and_binary_imports_have_stable_truthful_identity() {
    let (_project, _root, vault) = vault();
    std::fs::write(vault.root().join("inbox/empty.txt"), b"").expect("empty");
    let empty = import_inbox_file(&vault, "inbox/empty.txt", 10).expect("empty import");
    assert_eq!(empty.bytes, 0);
    std::fs::write(vault.root().join("inbox/image.bin"), [0, 159, 146, 150]).expect("binary");
    let binary = import_inbox_file(&vault, "inbox/image.bin", 11).expect("binary import");
    assert_eq!(binary.status, InboxImportStatus::EvidenceOnly);
    assert!(binary.imported_relative_path.starts_with("raw/assets/"));
    let transaction_id = committed_transaction(&vault, "tx-binary");
    assert_eq!(
        complete_inbox_import(&vault, &binary.content_hash, &transaction_id)
            .expect("binary remains"),
        binary
    );
    assert!(vault.root().join("inbox/image.bin").exists());
}

#[test]
fn changed_human_original_is_preserved_after_knowledge_commit() {
    let (_project, _root, vault) = vault();
    let origin = vault.root().join("inbox/note.md");
    std::fs::write(&origin, "first\n").expect("first");
    let imported = import_inbox_file(&vault, "inbox/note.md", 10).expect("import");
    std::fs::write(&origin, "human changed this\n").expect("change");
    let transaction_id = committed_transaction(&vault, "tx-changed");
    let completed =
        complete_inbox_import(&vault, &imported.content_hash, &transaction_id).expect("retained");
    assert_eq!(completed.status, InboxImportStatus::ImportedSourceRetained);
    assert_eq!(
        std::fs::read_to_string(origin).expect("origin"),
        "human changed this\n"
    );
}
