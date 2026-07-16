use minimax_protocol::{
    ContentHash, EvidenceId, KnowledgeJobId, KnowledgeOperation, KnowledgePage,
    KnowledgePageStatus, KnowledgePatch, KnowledgeValidationError, PageId, SchemaVersion,
    SourceCitation, TopicId,
};

fn hash(byte: char) -> ContentHash {
    ContentHash::new(byte.to_string().repeat(64)).expect("hash")
}

fn page(id: &str, topic: &str, status: KnowledgePageStatus) -> KnowledgePage {
    KnowledgePage {
        schema_version: SchemaVersion,
        page_id: PageId::new(id).expect("page ID"),
        topic_id: TopicId::new(topic).expect("topic ID"),
        relative_path: format!("wiki/concepts/{id}.md"),
        title: format!("Title {id}"),
        status,
        superseded_by: None,
        sources: vec![SourceCitation {
            source_id: EvidenceId::new("source-1").expect("source ID"),
            source_hash: hash('a'),
        }],
        body: "A source-grounded fact.".to_owned(),
    }
}

#[test]
fn patch_round_trips_with_strict_schema_one_records() {
    let patch = KnowledgePatch {
        schema_version: SchemaVersion,
        job_id: KnowledgeJobId::new("job-1").expect("job ID"),
        operations: vec![KnowledgeOperation::Create {
            page: page("page-1", "topic-1", KnowledgePageStatus::Current),
        }],
    }
    .validate()
    .expect("valid patch");
    let encoded = serde_json::to_string(&patch).expect("JSON");
    assert!(encoded.contains("\"schemaVersion\":1"));
    assert_eq!(
        serde_json::from_str::<KnowledgePatch>(&encoded)
            .expect("decode")
            .validate()
            .expect("validate"),
        patch
    );
}

#[test]
fn invalid_paths_duplicates_and_supersession_fail_closed() {
    let mut invalid = page("page-1", "topic-1", KnowledgePageStatus::Current);
    invalid.relative_path = "../outside.md".to_owned();
    assert_eq!(
        invalid.validate(),
        Err(KnowledgeValidationError::InvalidPath)
    );

    let mut invalid = page("page-2", "topic-1", KnowledgePageStatus::Superseded);
    assert_eq!(
        invalid.clone().validate(),
        Err(KnowledgeValidationError::InvalidSupersession)
    );
    invalid.superseded_by = Some(PageId::new("page-3").expect("page ID"));
    invalid.clone().validate().expect("valid tombstone");

    let patch = KnowledgePatch {
        schema_version: SchemaVersion,
        job_id: KnowledgeJobId::new("job-2").expect("job ID"),
        operations: vec![
            KnowledgeOperation::Create {
                page: page("same", "one", KnowledgePageStatus::Current),
            },
            KnowledgeOperation::Replace {
                page: page("same", "two", KnowledgePageStatus::Current),
                expected_hash: hash('b'),
            },
        ],
    };
    assert_eq!(
        patch.validate(),
        Err(KnowledgeValidationError::DuplicateOperation)
    );
}

#[test]
fn unknown_fields_and_future_schema_fail() {
    let raw = r#"{"schema_version":1,"job_id":"job-1","operations":[],"extra":true}"#;
    assert!(serde_json::from_str::<KnowledgePatch>(raw).is_err());
    let raw = r#"{"schema_version":2,"job_id":"job-1","operations":[]}"#;
    assert!(serde_json::from_str::<KnowledgePatch>(raw).is_err());
}
