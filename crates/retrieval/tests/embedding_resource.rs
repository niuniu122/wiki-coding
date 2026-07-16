use std::fs;

use minimax_protocol::{RetrievalDegradedReason, SchemaVersion};
use minimax_retrieval::{
    EMBEDDING_HELPER_ABI, EmbeddingHost, EmbeddingResourceManifest, GRANITE_EMBEDDING_MODEL_ID,
    GRANITE_RESOURCE_PACKAGE_ID, ResourceFile, validate_embedding_resource,
};
use sha2::{Digest as _, Sha256};

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    Sha256::digest(bytes)
        .iter()
        .flat_map(|byte| {
            [
                char::from(DIGITS[usize::from(*byte >> 4)]),
                char::from(DIGITS[usize::from(*byte & 0x0f)]),
            ]
        })
        .collect()
}

fn host(avx2: bool) -> EmbeddingHost {
    EmbeddingHost {
        architecture: "x86_64".into(),
        avx2,
        runtime_abi: EMBEDDING_HELPER_ABI.into(),
    }
}

fn install_resource(directory: &std::path::Path, catalog_fingerprint: &str) {
    let helper = b"tiny deterministic helper fixture";
    let vectors = b"tiny deterministic vectors";
    fs::write(directory.join("helper.bin"), helper).expect("helper");
    fs::write(directory.join("vectors.bin"), vectors).expect("vectors");
    let manifest = EmbeddingResourceManifest {
        schema_version: SchemaVersion,
        package_id: GRANITE_RESOURCE_PACKAGE_ID.into(),
        model_id: GRANITE_EMBEDDING_MODEL_ID.into(),
        model_revision: "835ad14087e140460703cf0fae09f97d469d65c2".into(),
        runtime_abi: EMBEDDING_HELPER_ABI.into(),
        architecture: "x64-avx2".into(),
        quantization: "qint8".into(),
        license: "Apache-2.0".into(),
        tokenizer_version: "granite-test-v1".into(),
        dimensions: 3,
        catalog_fingerprint: catalog_fingerprint.into(),
        vector_fingerprint: format!("sha256:{}", hex(vectors)),
        helper_relative_path: "helper.bin".into(),
        platform_health: "verified".into(),
        files: vec![
            ResourceFile {
                path: "helper.bin".into(),
                sha256: hex(helper),
            },
            ResourceFile {
                path: "vectors.bin".into(),
                sha256: hex(vectors),
            },
        ],
    };
    fs::write(
        directory.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).expect("manifest"),
    )
    .expect("write manifest");
}

#[test]
fn resource_requires_every_identity_hash_abi_cpu_and_fingerprint_proof() {
    let directory = tempfile::tempdir().expect("tempdir");
    let fingerprint = format!("sha256:{}", "1".repeat(64));
    install_resource(directory.path(), &fingerprint);
    let verified =
        validate_embedding_resource(directory.path(), &host(true), &fingerprint).expect("verified");
    assert_eq!(verified.manifest.dimensions, 3);
    assert_eq!(
        verified.helper_path,
        directory
            .path()
            .canonicalize()
            .expect("root")
            .join("helper.bin")
    );

    assert!(matches!(
        validate_embedding_resource(directory.path(), &host(false), &fingerprint),
        Err(RetrievalDegradedReason::IncompatibleCpu)
    ));
    assert!(matches!(
        validate_embedding_resource(
            directory.path(),
            &host(true),
            &format!("sha256:{}", "2".repeat(64))
        ),
        Err(RetrievalDegradedReason::FingerprintMismatch)
    ));

    fs::write(directory.path().join("vectors.bin"), b"tampered").expect("tamper");
    assert!(matches!(
        validate_embedding_resource(directory.path(), &host(true), &fingerprint),
        Err(RetrievalDegradedReason::HashMismatch)
    ));
}

#[test]
fn missing_and_unknown_manifest_data_fail_closed_without_creating_resources() {
    let parent = tempfile::tempdir().expect("tempdir");
    let missing = parent.path().join("missing");
    let fingerprint = format!("sha256:{}", "1".repeat(64));
    assert!(matches!(
        validate_embedding_resource(&missing, &host(true), &fingerprint),
        Err(RetrievalDegradedReason::EmbeddingMissing)
    ));
    assert!(!missing.exists());

    let resource = tempfile::tempdir().expect("resource");
    install_resource(resource.path(), &fingerprint);
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(resource.path().join("manifest.json")).expect("read"))
            .expect("json");
    manifest["networkUrl"] = serde_json::Value::String("https://forbidden.test/model".into());
    fs::write(
        resource.path().join("manifest.json"),
        serde_json::to_vec(&manifest).expect("json"),
    )
    .expect("write");
    assert!(matches!(
        validate_embedding_resource(resource.path(), &host(true), &fingerprint),
        Err(RetrievalDegradedReason::InvalidManifest)
    ));
}
