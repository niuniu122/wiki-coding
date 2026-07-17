use minimax_cli::{augment_agent_prompt, is_capability_discovery_intent, project_search};
use minimax_protocol::{IndexDomain, RetrievalDegradedReason, RetrievalMode};

#[tokio::test]
async fn bundled_catalog_makes_bm25_first_discovery_available_without_expert_paths() {
    let response = project_search(None, None, "我需要一个快速查找文件的开源命令行工具", 5)
        .await
        .expect("bundled discovery");
    assert_eq!(response.domain, IndexDomain::Project);
    assert_eq!(response.mode, RetrievalMode::Bm25);
    assert_eq!(
        response.degraded_reason,
        Some(RetrievalDegradedReason::EmbeddingMissing)
    );
    assert!(!response.keywords.is_empty());
    assert_eq!(response.results[0].id, "sharkdp/fd");
    assert!(response.results.iter().all(|hit| {
        hit.source_url.is_some() && hit.repository_url.is_some() && hit.explanation.lexical_rank > 0
    }));
}

#[tokio::test]
async fn ordinary_agent_need_receives_read_only_discovery_context_only_when_requested() {
    let prompt = "帮我找一个开源 CLI、Skill 或 MCP 工具来搜索源代码".to_owned();
    assert!(is_capability_discovery_intent(&prompt));
    let augmented = augment_agent_prompt(None, None, None, prompt.clone())
        .await
        .expect("augmented prompt");
    assert!(augmented.starts_with(&prompt));
    assert!(augmented.contains("[local_capability_discovery schema=1 read_only=true]"));
    assert!(augmented.contains("project:burntsushi/ripgrep"));
    assert!(augmented.contains("skill:openai/openai-docs"));
    assert!(augmented.contains("\"mode\":\"bm25\""));
    assert!(augmented.contains(
        "Do not download, install, authorize, or run a project, Skill, or MCP server automatically."
    ));

    let ordinary = "Explain this local function".to_owned();
    assert!(!is_capability_discovery_intent(&ordinary));
    assert_eq!(
        augment_agent_prompt(None, None, None, ordinary.clone())
            .await
            .expect("ordinary prompt"),
        ordinary
    );
}
