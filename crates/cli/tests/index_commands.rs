use std::path::{Path, PathBuf};

use clap::Parser as _;
use minimax_cli::{
    CapabilityIndexAction, Cli, CliCommand, IndexAction, JsonlWriter, ProjectIndexAction,
    WikiIndexAction, capability_search, capability_status, project_search, project_status,
    wiki_search, wiki_status,
};
use minimax_protocol::{
    ContentHash, EvidenceId, KnowledgePage, KnowledgePageStatus, PageId, ProjectId,
    RetrievalDegradedReason, RetrievalMode, RetrievalResponse, SchemaVersion, SourceCitation,
    TopicId,
};
use minimax_tui::EventRenderer;
use minimax_vault::{ProjectVault, render_wiki_page};

fn fixture_catalog() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/compat/retrieval/projects.v1.json")
}

#[test]
fn clap_exposes_explicit_read_only_status_and_search_routes() {
    for arguments in [
        vec!["minimax-codex-rust", "index", "capabilities", "status"],
        vec![
            "minimax-codex-rust",
            "index",
            "capabilities",
            "search",
            "files",
        ],
        vec![
            "minimax-codex-rust",
            "index",
            "projects",
            "search",
            "fast file search",
            "--catalog",
            "projects.json",
        ],
        vec![
            "minimax-codex-rust",
            "index",
            "wiki",
            "search",
            "architecture",
            "--vault",
            "project.vault",
            "--project-id",
            "project-1",
        ],
    ] {
        assert!(Cli::try_parse_from(arguments).is_ok());
    }
    assert!(
        Cli::try_parse_from(["minimax-codex-rust", "index", "projects", "search", "query"])
            .is_err()
    );
    assert!(
        Cli::try_parse_from([
            "minimax-codex-rust",
            "index",
            "projects",
            "search",
            "query",
            "--catalog",
            "projects.json",
            "--execute"
        ])
        .is_err()
    );

    let parsed = Cli::try_parse_from([
        "minimax-codex-rust",
        "index",
        "projects",
        "search",
        "query",
        "--catalog",
        "projects.json",
    ])
    .expect("project search");
    assert!(matches!(
        parsed.command,
        CliCommand::Index(args)
            if matches!(args.action, IndexAction::Projects {
                action: ProjectIndexAction::Search { .. }
            })
    ));
    assert!(matches!(
        Cli::try_parse_from(["minimax-codex-rust", "index", "capabilities", "status"])
            .expect("capability status")
            .command,
        CliCommand::Index(args)
            if matches!(args.action, IndexAction::Capabilities {
                action: CapabilityIndexAction::Status
            })
    ));
    assert!(matches!(
        Cli::try_parse_from([
            "minimax-codex-rust", "index", "wiki", "status", "--vault", "vault",
            "--project-id", "p"
        ])
        .expect("wiki status")
        .command,
        CliCommand::Index(args)
            if matches!(args.action, IndexAction::Wiki {
                action: WikiIndexAction::Status { .. }
            })
    ));
}

#[tokio::test]
async fn capability_and_project_text_jsonl_contain_the_same_truthful_facts() {
    let capability = capability_search("search available commands", 5);
    assert!(!capability.results.is_empty());
    assert!(!capability.keywords.is_empty());
    let capability_text = EventRenderer::retrieval(&capability);
    assert!(capability_text.contains("domain=capability"));
    assert_eq!(capability_status().documents, 6);

    let project = project_search(&fixture_catalog(), None, "fast command line file search", 5)
        .await
        .expect("project search");
    assert_eq!(project.mode, RetrievalMode::Bm25);
    assert_eq!(
        project.degraded_reason,
        Some(RetrievalDegradedReason::EmbeddingMissing)
    );
    assert!(!project.results.is_empty());
    assert!(project.results.iter().all(|hit| hit.source_url.is_some()));
    assert!(project.results.iter().all(|hit| hit.license.is_none()));
    let text = EventRenderer::retrieval(&project);
    for fact in [
        "mode=bm25",
        "degraded=embedding_missing",
        "license=unknown",
        "maintenance=unknown",
        "repository=https://",
    ] {
        assert!(text.contains(fact), "missing {fact:?} in {text:?}");
    }

    let mut writer = JsonlWriter::new(Vec::new());
    writer.write_json(&project).expect("JSONL");
    let encoded = String::from_utf8(writer.into_inner()).expect("UTF-8");
    let decoded: RetrievalResponse = serde_json::from_str(encoded.trim()).expect("strict response");
    assert_eq!(decoded, project);

    let status = project_status(&fixture_catalog(), None).expect("status");
    assert_eq!(status.documents, 6);
    assert_eq!(
        status.degraded_reason,
        Some(RetrievalDegradedReason::EmbeddingMissing)
    );
}

#[test]
fn wiki_search_reads_current_pages_through_the_vault_boundary_only() {
    let project = tempfile::tempdir().expect("project");
    let root = tempfile::tempdir().expect("vault");
    let project_id = ProjectId::new("project-1").expect("project ID");
    let vault = ProjectVault::bootstrap(project.path(), root.path(), project_id.clone(), 1)
        .expect("bootstrap");
    let current_id = PageId::new("current-page").expect("page ID");
    let current = page(
        current_id.clone(),
        KnowledgePageStatus::Current,
        None,
        "Current architecture",
        "Rust Vault retrieval keeps current knowledge searchable.",
        "wiki/concepts/current-architecture.md",
    );
    let superseded = page(
        PageId::new("old-page").expect("page ID"),
        KnowledgePageStatus::Superseded,
        Some(current_id),
        "Old secret architecture",
        "legacy superseded secret phrase",
        "wiki/concepts/old-secret-architecture.md",
    );
    for page in [current, superseded] {
        std::fs::write(
            vault.root().join(&page.relative_path),
            render_wiki_page(&page).expect("page"),
        )
        .expect("write page");
    }
    drop(vault);

    let status = wiki_status(project.path(), root.path(), project_id.clone()).expect("status");
    assert_eq!(status.documents, 1);
    let result = wiki_search(
        project.path(),
        root.path(),
        project_id,
        "Rust Vault retrieval",
        5,
    )
    .expect("search");
    assert_eq!(result.results.len(), 1);
    assert_eq!(result.results[0].id, "current-page");
    let old = wiki_search(
        project.path(),
        root.path(),
        ProjectId::new("project-1").expect("project ID"),
        "legacy superseded secret phrase",
        5,
    )
    .expect("old search");
    assert!(old.results.is_empty());
}

fn page(
    page_id: PageId,
    status: KnowledgePageStatus,
    superseded_by: Option<PageId>,
    title: &str,
    body: &str,
    relative_path: &str,
) -> KnowledgePage {
    KnowledgePage {
        schema_version: SchemaVersion,
        page_id,
        topic_id: TopicId::new("topic-architecture").expect("topic"),
        relative_path: relative_path.into(),
        title: title.into(),
        status,
        superseded_by,
        sources: vec![SourceCitation {
            source_id: EvidenceId::new("source-1").expect("source"),
            source_hash: ContentHash::new("a".repeat(64)).expect("hash"),
        }],
        body: body.into(),
    }
}
