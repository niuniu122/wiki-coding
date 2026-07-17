use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;

use minimax_retrieval::{
    CapabilityDocument, CapabilityIndex, IndexSnapshot, ProjectDocument, ProjectIndex,
    SnapshotError, WikiDocument, WikiIndex, load_snapshot, publish_snapshot,
};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

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
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Fixture {
    schema_version: u16,
    corpus_id: String,
    source: FixtureSource,
    corpus_fingerprint: String,
    thresholds: Thresholds,
    descriptors: Vec<FixtureDescriptor>,
    case_groups: Vec<CaseGroup>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FixtureSource {
    path: String,
    sha256: String,
    retained_until: String,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Thresholds {
    minimum_cases: usize,
    recall_at5: f64,
    top1: f64,
    mrr: f64,
    no_match_precision: f64,
    id_validity: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FixtureDescriptor {
    schema_version: u16,
    id: String,
    name: String,
    description: String,
    aliases: Vec<String>,
    commands: Vec<String>,
    safety_class: String,
    idempotent: bool,
    execution: Execution,
    facets: Facets,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Facets {
    domain: Vec<String>,
    action: Vec<String>,
    object: Vec<String>,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Execution {
    WorkspaceRead { operation: String },
    NpmScript { script: String, argv: Vec<String> },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CaseGroup {
    id: String,
    expected_ids: Vec<String>,
    no_match: bool,
    query_ids: Vec<String>,
    queries: Vec<String>,
}

fn sha256(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(&mut output, "{byte:02x}").expect("writing to a String cannot fail");
    }
    output
}

fn strict_fixture(raw: &str) -> Result<Fixture, String> {
    let value: Value = serde_json::from_str(raw).map_err(|error| error.to_string())?;
    let fixture: Fixture =
        serde_json::from_value(value.clone()).map_err(|error| error.to_string())?;
    if fixture.schema_version != 1
        || fixture.corpus_id != "capability-retrieval-expanded-v1"
        || fixture.source.path.is_empty()
        || fixture.source.sha256.len() != 64
        || !fixture
            .source
            .sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || fixture.source.retained_until != "14-01"
    {
        return Err("invalid immutable corpus metadata".into());
    }
    if fixture.thresholds.minimum_cases != 150
        || fixture.thresholds.recall_at5 != 0.95
        || fixture.thresholds.top1 != 0.85
        || fixture.thresholds.mrr != 0.9
        || fixture.thresholds.no_match_precision != 0.95
        || fixture.thresholds.id_validity != 1.0
    {
        return Err("retrieval thresholds drifted".into());
    }
    let fingerprint_input = json!({
        "caseGroups": value["caseGroups"],
        "descriptors": value["descriptors"],
        "thresholds": value["thresholds"]
    });
    let fingerprint = sha256(
        &serde_json::to_vec(&fingerprint_input).map_err(|error| error.to_string())?,
    );
    if fixture.corpus_fingerprint != fingerprint {
        return Err("corpus fingerprint mismatch".into());
    }

    let mut descriptor_ids = BTreeSet::new();
    for descriptor in &fixture.descriptors {
        if descriptor.schema_version != 1
            || descriptor.safety_class.is_empty()
            || descriptor.id.is_empty()
            || !descriptor_ids.insert(descriptor.id.as_str())
        {
            return Err("invalid or duplicate descriptor".into());
        }
        match &descriptor.execution {
            Execution::WorkspaceRead { operation } if operation.is_empty() => {
                return Err("empty workspace operation".into());
            }
            Execution::NpmScript { script, argv } if script.is_empty() || !argv.is_empty() => {
                return Err("invalid npm execution declaration".into());
            }
            _ => {}
        }
        if !descriptor.idempotent && !matches!(&descriptor.execution, Execution::NpmScript { .. }) {
            return Err("non-idempotent capability must be the declared diagnostic".into());
        }
    }

    let mut group_ids = BTreeSet::new();
    let mut query_ids = BTreeSet::new();
    let mut case_count = 0usize;
    for group in &fixture.case_groups {
        if group.id.is_empty()
            || !group_ids.insert(group.id.as_str())
            || group.queries.len() != group.query_ids.len()
            || group.queries.iter().any(|query| query.trim().is_empty())
            || group.no_match != group.expected_ids.is_empty()
            || group
                .expected_ids
                .iter()
                .any(|id| !descriptor_ids.contains(id.as_str()))
        {
            return Err("invalid case group".into());
        }
        for query_id in &group.query_ids {
            case_count += 1;
            if query_id != &format!("capability-case-{case_count:03}")
                || !query_ids.insert(query_id.as_str())
            {
                return Err("invalid or duplicate stable query ID".into());
            }
        }
    }
    if case_count != 175 || case_count < fixture.thresholds.minimum_cases {
        return Err("retrieval case count drifted".into());
    }
    Ok(fixture)
}

#[test]
fn existing_typescript_175_case_fixture_meets_capability_gates() {
    let fixture = strict_fixture(include_str!(
        "../../../test/fixtures/capabilities/retrieval-cases-expanded.json"
    ))
    .expect("strict immutable fixture");
    let thresholds = fixture.thresholds.clone();
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
        .collect::<Vec<_>>();
    let valid_ids = documents
        .iter()
        .map(|document| document.id.clone())
        .collect::<BTreeSet<_>>();
    let index = CapabilityIndex::new(documents);
    let mut cases = 0usize;
    let mut positives = 0usize;
    let mut negatives = 0usize;
    let mut recalled = 0usize;
    let mut top1 = 0usize;
    let mut reciprocal_rank = 0.0;
    let mut no_match_correct = 0usize;
    let mut returned = 0usize;
    let mut valid = 0usize;
    let mut exact = 0usize;
    let mut bm25 = 0usize;
    for group in fixture.case_groups {
        for query in group.queries {
            cases += 1;
            let result = index.search(&query, 5);
            returned += result.len();
            valid += result
                .iter()
                .filter(|hit| valid_ids.contains(&hit.document.id))
                .count();
            match result.first().map(|hit| hit.mode) {
                Some(minimax_protocol::RetrievalMode::Exact) => exact += 1,
                Some(minimax_protocol::RetrievalMode::Bm25) => bm25 += 1,
                _ => {}
            }
            if group.no_match {
                negatives += 1;
                assert!(
                    result.is_empty(),
                    "unexpected match for {query:?}: {:?}",
                    result
                        .iter()
                        .map(|hit| hit.document.id.as_str())
                        .collect::<Vec<_>>()
                );
                no_match_correct += usize::from(result.is_empty());
            } else {
                positives += 1;
                let rank = result
                    .iter()
                    .position(|hit| group.expected_ids.contains(&hit.document.id));
                if let Some(rank) = rank {
                    recalled += 1;
                    reciprocal_rank += 1.0 / (rank + 1) as f64;
                    top1 += usize::from(rank == 0);
                }
            }
        }
    }
    assert_eq!(cases, 175);
    assert!(exact > 0, "corpus must exercise exact retrieval");
    assert!(bm25 > 0, "corpus must exercise BM25 retrieval");
    assert!((recalled as f64 / positives as f64) >= thresholds.recall_at5);
    assert!((top1 as f64 / positives as f64) >= thresholds.top1);
    assert!((reciprocal_rank / positives as f64) >= thresholds.mrr);
    assert!((no_match_correct as f64 / negatives as f64) >= thresholds.no_match_precision);
    assert!((valid as f64 / returned as f64) >= thresholds.id_validity);
}

#[test]
fn immutable_corpus_rejects_schema_identity_and_case_drift() {
    let raw = include_str!("../../../test/fixtures/capabilities/retrieval-cases-expanded.json");
    let value: Value = serde_json::from_str(raw).expect("fixture JSON");

    let mut unknown = value.clone();
    unknown["surprise"] = Value::Bool(true);
    assert!(strict_fixture(&unknown.to_string()).is_err());

    let mut duplicate_id = value.clone();
    duplicate_id["caseGroups"][0]["queryIds"][1] =
        duplicate_id["caseGroups"][0]["queryIds"][0].clone();
    assert!(strict_fixture(&duplicate_id.to_string()).is_err());

    let mut invalid_expected = value.clone();
    invalid_expected["caseGroups"][0]["expectedIds"][0] = Value::String("outsider".into());
    assert!(strict_fixture(&invalid_expected.to_string()).is_err());

    let mut missing_case = value;
    missing_case["caseGroups"][0]["queries"]
        .as_array_mut()
        .expect("queries")
        .pop();
    missing_case["caseGroups"][0]["queryIds"]
        .as_array_mut()
        .expect("query IDs")
        .pop();
    assert!(strict_fixture(&missing_case.to_string()).is_err());
}

#[test]
fn bm25_ties_are_ordered_by_stable_document_id() {
    let index = CapabilityIndex::new(vec![
        capability("z-last", "Same", &[], &[], "identical stable tie phrase"),
        capability("a-first", "Same", &[], &[], "identical stable tie phrase"),
    ]);
    let ids = index
        .search("stable tie phrase", 5)
        .into_iter()
        .map(|hit| hit.document.id)
        .collect::<Vec<_>>();
    assert_eq!(ids, ["a-first", "z-last"]);
}
