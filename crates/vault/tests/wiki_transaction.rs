use minimax_protocol::{
    ContentHash, EvidenceId, KnowledgePage, KnowledgePageStatus, PageId, ProjectId, SchemaVersion,
    SourceCitation, TopicId, TransactionId,
};
use minimax_vault::{
    PreparedWikiTransaction, ProjectVault, TransactionFaultPoint, VaultError, WikiChange,
    hash_vault_bytes, parse_wiki_page, recover_wiki_transaction, render_wiki_page,
};

fn hash(byte: char) -> ContentHash {
    ContentHash::new(byte.to_string().repeat(64)).expect("hash")
}

fn page(path: &str) -> KnowledgePage {
    KnowledgePage {
        schema_version: SchemaVersion,
        page_id: PageId::new("page-architecture").expect("page ID"),
        topic_id: TopicId::new("topic-architecture").expect("topic ID"),
        relative_path: path.to_owned(),
        title: "架构 Decision".to_owned(),
        status: KnowledgePageStatus::Current,
        superseded_by: None,
        sources: vec![SourceCitation {
            source_id: EvidenceId::new("source-1").expect("source"),
            source_hash: hash('a'),
        }],
        body: "保留原始证据，再编译当前结论。".to_owned(),
    }
}

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

#[test]
fn page_frontmatter_is_strict_obisidian_compatible_and_deterministic() {
    let page = page("wiki/decisions/架构-decision.md");
    let rendered = render_wiki_page(&page).expect("render");
    assert_eq!(render_wiki_page(&page).expect("repeat"), rendered);
    assert!(
        std::str::from_utf8(&rendered)
            .expect("UTF-8")
            .starts_with("---\n")
    );
    assert_eq!(
        parse_wiki_page(&page.relative_path, &rendered).expect("parse"),
        page
    );
    let mut unknown = rendered.clone();
    let position = unknown
        .windows(4)
        .position(|window| window == b"---\n")
        .expect("frontmatter")
        + 4;
    unknown.splice(position..position, b"extra: true\n".iter().copied());
    assert!(matches!(
        parse_wiki_page(&page.relative_path, &unknown),
        Err(VaultError::InvalidPage)
    ));
    let mut invalid_slug = page.clone();
    invalid_slug.relative_path = "wiki/decisions/Not Normal.md".to_owned();
    assert!(render_wiki_page(&invalid_slug).is_err());
}

#[test]
fn crash_rolls_forward_in_page_index_log_order_and_is_idempotent() {
    let (_project, _root, vault) = vault();
    let page_path = "wiki/decisions/architecture.md";
    let page_bytes = render_wiki_page(&page(page_path)).expect("page");
    let index_before = std::fs::read(vault.root().join("wiki/index.md")).expect("index");
    let log_before = std::fs::read(vault.root().join("log.md")).expect("log");
    let transaction_id = TransactionId::new("tx-crash").expect("transaction ID");
    let mut transaction = PreparedWikiTransaction::prepare(
        &vault,
        transaction_id.clone(),
        vec![
            WikiChange {
                relative_path: "log.md".to_owned(),
                expected_old_hash: Some(hash_vault_bytes(&log_before)),
                bytes: b"# MiniMax Knowledge Log\n\n- committed\n".to_vec(),
            },
            WikiChange {
                relative_path: "wiki/index.md".to_owned(),
                expected_old_hash: Some(hash_vault_bytes(&index_before)),
                bytes: b"# Project Wiki\n\n- [[decisions/architecture]]\n".to_vec(),
            },
            WikiChange {
                relative_path: page_path.to_owned(),
                expected_old_hash: None,
                bytes: page_bytes.clone(),
            },
        ],
        10,
    )
    .expect("prepare");
    assert!(matches!(
        transaction.roll_forward_with_fault(Some(TransactionFaultPoint::AfterTarget(0))),
        Err(VaultError::FaultInjected)
    ));
    assert_eq!(
        std::fs::read(vault.root().join(page_path)).expect("page"),
        page_bytes
    );
    assert_eq!(
        std::fs::read(vault.root().join("wiki/index.md")).expect("index"),
        index_before
    );

    let first = recover_wiki_transaction(&vault, &transaction_id).expect("recover");
    let second = recover_wiki_transaction(&vault, &transaction_id).expect("repeat recover");
    assert_eq!(first, second);
    assert!(
        std::fs::read_to_string(vault.root().join("wiki/index.md"))
            .expect("index")
            .contains("architecture")
    );
    assert!(
        std::fs::read_to_string(vault.root().join("log.md"))
            .expect("log")
            .contains("committed")
    );
}

#[test]
fn stale_external_edits_and_tampered_staging_replace_nothing() {
    let (_project, _root, vault) = vault();
    let index_path = vault.root().join("wiki/index.md");
    let log_path = vault.root().join("log.md");
    let index_before = std::fs::read(&index_path).expect("index");
    let log_before = std::fs::read(&log_path).expect("log");
    let transaction_id = TransactionId::new("tx-conflict").expect("transaction ID");
    let transaction = PreparedWikiTransaction::prepare(
        &vault,
        transaction_id,
        vec![
            WikiChange {
                relative_path: "wiki/index.md".to_owned(),
                expected_old_hash: Some(hash_vault_bytes(&index_before)),
                bytes: b"new index\n".to_vec(),
            },
            WikiChange {
                relative_path: "log.md".to_owned(),
                expected_old_hash: Some(hash_vault_bytes(&log_before)),
                bytes: b"new log\n".to_vec(),
            },
        ],
        10,
    )
    .expect("prepare");
    std::fs::write(&log_path, b"human external edit\n").expect("external edit");
    assert!(matches!(
        transaction.roll_forward(),
        Err(VaultError::Conflict)
    ));
    assert_eq!(std::fs::read(&index_path).expect("index"), index_before);
    assert_eq!(
        std::fs::read(&log_path).expect("log"),
        b"human external edit\n"
    );

    let transaction_id = TransactionId::new("tx-tamper").expect("transaction ID");
    let transaction = PreparedWikiTransaction::prepare(
        &vault,
        transaction_id,
        vec![WikiChange {
            relative_path: "wiki/index.md".to_owned(),
            expected_old_hash: Some(hash_vault_bytes(&index_before)),
            bytes: b"tamper target\n".to_vec(),
        }],
        11,
    )
    .expect("prepare");
    let staged = vault
        .root()
        .join(&transaction.manifest().targets[0].staged_relative_path);
    std::fs::write(staged, b"tampered").expect("tamper staging");
    assert!(matches!(
        transaction.roll_forward(),
        Err(VaultError::RecoveryRequired)
    ));
    assert_eq!(std::fs::read(index_path).expect("index"), index_before);
}
