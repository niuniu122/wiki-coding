use minimax_protocol::{
    ContentHash, ProjectId, SchemaVersion, TransactionId, TransactionManifest, TransactionState,
    TransactionTarget, VaultManifest, VaultValidationError, validate_vault_relative_path,
};

fn hash(byte: char) -> ContentHash {
    ContentHash::new(byte.to_string().repeat(64)).expect("hash")
}

#[test]
fn manifest_and_transaction_round_trip_strictly() {
    let manifest = VaultManifest {
        schema_version: SchemaVersion,
        project_id: ProjectId::new("project-1").expect("project ID"),
        project_fingerprint: hash('a'),
        created_at_unix_ms: 42,
    };
    let encoded = serde_json::to_string(&manifest).expect("JSON");
    assert_eq!(
        serde_json::from_str::<VaultManifest>(&encoded).expect("decode"),
        manifest
    );

    let transaction = TransactionManifest {
        schema_version: SchemaVersion,
        transaction_id: TransactionId::new("tx-1").expect("transaction ID"),
        state: TransactionState::Prepared,
        targets: vec![TransactionTarget {
            relative_path: "wiki/index.md".to_owned(),
            old_hash: None,
            expected_hash: hash('b'),
            staged_relative_path: ".minimax/transactions/tx-1/staged/0".to_owned(),
            order: 0,
        }],
        created_at_unix_ms: 43,
    }
    .validate()
    .expect("valid transaction");
    let encoded = serde_json::to_string(&transaction).expect("JSON");
    assert_eq!(
        serde_json::from_str::<TransactionManifest>(&encoded)
            .expect("decode")
            .validate()
            .expect("validate"),
        transaction
    );
}

#[test]
fn paths_hashes_order_and_unknown_fields_fail_closed() {
    for path in [
        "",
        "/absolute",
        "C:/absolute",
        "wiki/../raw",
        "wiki\\page.md",
    ] {
        assert_eq!(
            validate_vault_relative_path(path),
            Err(VaultValidationError::InvalidPath)
        );
    }
    assert!(ContentHash::new("A".repeat(64)).is_err());
    assert!(ContentHash::new("a".repeat(63)).is_err());

    let transaction = TransactionManifest {
        schema_version: SchemaVersion,
        transaction_id: TransactionId::new("tx-2").expect("transaction ID"),
        state: TransactionState::Prepared,
        targets: vec![TransactionTarget {
            relative_path: "wiki/index.md".to_owned(),
            old_hash: None,
            expected_hash: hash('c'),
            staged_relative_path: ".minimax/transactions/tx-2/staged/0".to_owned(),
            order: 1,
        }],
        created_at_unix_ms: 44,
    };
    assert_eq!(
        transaction.validate(),
        Err(VaultValidationError::InvalidTargetOrder)
    );

    let raw = r#"{"schemaVersion":1,"projectId":"project-1","projectFingerprint":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","createdAtUnixMs":42,"extra":true}"#;
    assert!(serde_json::from_str::<VaultManifest>(raw).is_err());
}
