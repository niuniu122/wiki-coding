use std::fs;

use minimax_retrieval::{
    CapabilityDocument, CapabilityIndex, IndexSnapshot, ProjectDocument, ProjectIndex,
    SnapshotError, WikiDocument, WikiIndex, load_snapshot, publish_snapshot,
};
use serde::Deserialize;

fn capability(
    id: &str,
    name: &str,
    aliases: &[&str],
    commands: &[&str],
    intent: &str,
) -> CapabilityDocument {
    CapabilityDocument {
        id: id.to_owned(),
        name: name.to_owned(),
        description: String::new(),
        aliases: aliases.iter().map(|value| (*value).to_owned()).collect(),
        commands: commands.iter().map(|value| (*value).to_owned()).collect(),
        intent_document: intent.to_owned(),
        available: true,
    }
}

#[test]
fn exact_wins_and_bm25_does_not_manufacture_matches() {
    let index = CapabilityIndex::new(vec![
        capability(
            "read",
            "Read file",
            &["open readme"],
            &["/read"],
            "read inspect local workspace file README config 查看 文件",
        ),
        capability(
            "search",
            "Search code",
            &["find symbol"],
            &["/search"],
            "search find local project source code symbol text 搜索 代码",
        ),
    ]);

    let exact = index.search("/search", 5);
    assert_eq!(exact[0].document.id, "search");
    assert_eq!(exact[0].mode, minimax_protocol::RetrievalMode::Exact);

    let lexical = index.search("please inspect README file", 5);
    assert_eq!(lexical[0].document.id, "read");
    assert!(!lexical[0].contributions.is_empty());
    assert!(index.search("quantum banana", 5).is_empty());
    assert!(index.search("the and please", 5).is_empty());
}

#[test]
fn typed_domains_filter_non_searchable_documents() {
    let projects = ProjectIndex::new(vec![ProjectDocument {
        id: "ripgrep".into(),
        name: "ripgrep".into(),
        aliases: vec!["rg".into()],
        description: "fast recursive text search".into(),
        topics: vec!["search".into(), "cli".into()],
        platforms: vec!["windows".into(), "linux".into()],
    }]);
    assert_eq!(
        projects.search("fast text search", 5)[0].document.id,
        "ripgrep"
    );

    let wiki = WikiIndex::new(vec![
        WikiDocument {
            id: "current".into(),
            title: "Current design".into(),
            body: "vault retrieval".into(),
            aliases: vec![],
            current: true,
        },
        WikiDocument {
            id: "old".into(),
            title: "Old design".into(),
            body: "secret legacy phrase".into(),
            aliases: vec![],
            current: false,
        },
    ]);
    assert_eq!(wiki.len(), 1);
    assert!(wiki.search("secret legacy phrase", 5).is_empty());
}

#[test]
fn snapshots_reject_domain_and_hash_drift_and_keep_last_good_file() {
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join("index.json");
    let snapshot = IndexSnapshot::new(vec![ProjectDocument {
        id: "rg".into(),
        name: "ripgrep".into(),
        aliases: vec![],
        description: "text search".into(),
        topics: vec![],
        platforms: vec![],
    }])
    .expect("snapshot");
    let published = publish_snapshot(&path, None, &snapshot).expect("publish");
    let loaded = load_snapshot::<ProjectDocument>(&path).expect("load");
    assert_eq!(loaded.documents[0].id, "rg");

    assert!(matches!(
        publish_snapshot(&path, Some("wrong"), &snapshot),
        Err(SnapshotError::ExpectedHashMismatch)
    ));
    assert_eq!(
        minimax_retrieval::snapshot_file_hash(&path).expect("hash"),
        published
    );
    assert!(matches!(
        load_snapshot::<CapabilityDocument>(&path),
        Err(SnapshotError::DomainMismatch)
    ));

    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).expect("read")).expect("json");
    value["documents"][0]["description"] = serde_json::Value::String("tampered".into());
    fs::write(&path, serde_json::to_vec(&value).expect("serialize")).expect("write");
    assert!(matches!(
        load_snapshot::<ProjectDocument>(&path),
        Err(SnapshotError::DocumentsHashMismatch)
    ));
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Fixture {
    descriptors: Vec<FixtureDescriptor>,
    case_groups: Vec<CaseGroup>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixtureDescriptor {
    id: String,
    name: String,
    description: String,
    aliases: Vec<String>,
    commands: Vec<String>,
    facets: Facets,
}

#[derive(Deserialize)]
struct Facets {
    domain: Vec<String>,
    action: Vec<String>,
    object: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CaseGroup {
    expected_ids: Vec<String>,
    #[serde(default)]
    no_match: bool,
    queries: Vec<String>,
}

#[test]
fn existing_typescript_175_case_fixture_meets_capability_gates() {
    let fixture: Fixture = serde_json::from_str(include_str!(
        "../../../test/fixtures/capabilities/retrieval-cases-expanded.json"
    ))
    .expect("fixture");
    let documents = fixture
        .descriptors
        .into_iter()
        .map(|descriptor| {
            let intent_document = std::iter::once(descriptor.name.as_str())
                .chain(std::iter::once(descriptor.description.as_str()))
                .chain(descriptor.aliases.iter().map(String::as_str))
                .chain(descriptor.commands.iter().map(String::as_str))
                .chain(descriptor.facets.domain.iter().map(String::as_str))
                .chain(descriptor.facets.action.iter().map(String::as_str))
                .chain(descriptor.facets.object.iter().map(String::as_str))
                .collect::<Vec<_>>()
                .join("\n");
            CapabilityDocument {
                id: descriptor.id,
                name: descriptor.name,
                description: descriptor.description,
                aliases: descriptor.aliases,
                commands: descriptor.commands,
                intent_document,
                available: true,
            }
        })
        .collect();
    let index = CapabilityIndex::new(documents);
    let mut cases = 0usize;
    for group in fixture.case_groups {
        for query in group.queries {
            cases += 1;
            let result = index.search(&query, 3);
            if group.no_match {
                assert!(
                    result.is_empty(),
                    "unexpected match for {query:?}: {:?}",
                    result
                        .iter()
                        .map(|hit| hit.document.id.as_str())
                        .collect::<Vec<_>>()
                );
            } else {
                assert_eq!(
                    result.first().map(|hit| hit.document.id.as_str()),
                    group.expected_ids.first().map(String::as_str),
                    "query {query:?}"
                );
            }
        }
    }
    assert_eq!(cases, 175);
}
