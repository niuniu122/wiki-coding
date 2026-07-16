use std::collections::BTreeSet;
use std::future::Future;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::Stdio;
use std::time::Duration;

use minimax_protocol::{RetrievalDegradedReason, SchemaVersion};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::process::Command;

use crate::catalog::encode_digest;

pub const GRANITE_EMBEDDING_MODEL_ID: &str = "ibm-granite/granite-embedding-97m-multilingual-r2";
pub const GRANITE_RESOURCE_PACKAGE_ID: &str = "@minimax-codex/embedding-granite-97m-r2-avx2";
pub const EMBEDDING_HELPER_ABI: &str = "minimax-embedding-helper-v1";
const MAX_HELPER_REQUEST_BYTES: usize = 512 * 1024;
const MAX_HELPER_OUTPUT_BYTES: usize = 4 * 1024 * 1024;
const MAX_CANDIDATES: usize = 20;
const MAX_VECTOR_DIMENSIONS: usize = 4_096;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResourceFile {
    pub path: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmbeddingResourceManifest {
    pub schema_version: SchemaVersion,
    pub package_id: String,
    pub model_id: String,
    pub model_revision: String,
    pub runtime_abi: String,
    pub architecture: String,
    pub quantization: String,
    pub license: String,
    pub tokenizer_version: String,
    pub dimensions: usize,
    pub catalog_fingerprint: String,
    pub vector_fingerprint: String,
    pub helper_relative_path: String,
    pub platform_health: String,
    pub files: Vec<ResourceFile>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmbeddingHost {
    pub architecture: String,
    pub avx2: bool,
    pub runtime_abi: String,
}

impl EmbeddingHost {
    #[must_use]
    pub fn detect() -> Self {
        #[cfg(target_arch = "x86_64")]
        let avx2 = std::arch::is_x86_feature_detected!("avx2");
        #[cfg(not(target_arch = "x86_64"))]
        let avx2 = false;
        Self {
            architecture: std::env::consts::ARCH.to_owned(),
            avx2,
            runtime_abi: EMBEDDING_HELPER_ABI.to_owned(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct VerifiedEmbeddingResource {
    pub directory: PathBuf,
    pub helper_path: PathBuf,
    pub manifest: EmbeddingResourceManifest,
}

pub fn validate_embedding_resource(
    directory: &Path,
    host: &EmbeddingHost,
    catalog_fingerprint: &str,
) -> Result<VerifiedEmbeddingResource, RetrievalDegradedReason> {
    if !directory.is_dir() {
        return Err(RetrievalDegradedReason::EmbeddingMissing);
    }
    let root = directory
        .canonicalize()
        .map_err(|_| RetrievalDegradedReason::InvalidManifest)?;
    let manifest_bytes = std::fs::read(root.join("manifest.json"))
        .map_err(|_| RetrievalDegradedReason::EmbeddingMissing)?;
    let manifest: EmbeddingResourceManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|_| RetrievalDegradedReason::InvalidManifest)?;
    if manifest.package_id != GRANITE_RESOURCE_PACKAGE_ID
        || manifest.model_id != GRANITE_EMBEDDING_MODEL_ID
        || manifest.model_revision.trim().is_empty()
        || manifest.runtime_abi != EMBEDDING_HELPER_ABI
        || manifest.architecture != "x64-avx2"
        || manifest.quantization != "qint8"
        || manifest.license.trim().is_empty()
        || manifest.tokenizer_version.trim().is_empty()
        || manifest.platform_health != "verified"
        || manifest.dimensions == 0
        || manifest.dimensions > MAX_VECTOR_DIMENSIONS
        || !valid_fingerprint(&manifest.catalog_fingerprint)
        || !valid_fingerprint(&manifest.vector_fingerprint)
        || !valid_relative_resource_path(&manifest.helper_relative_path)
        || manifest.files.is_empty()
    {
        return Err(RetrievalDegradedReason::InvalidManifest);
    }
    if host.architecture != "x86_64" || !host.avx2 {
        return Err(RetrievalDegradedReason::IncompatibleCpu);
    }
    if host.runtime_abi != manifest.runtime_abi {
        return Err(RetrievalDegradedReason::RuntimeAbiMismatch);
    }
    if manifest.catalog_fingerprint != catalog_fingerprint {
        return Err(RetrievalDegradedReason::FingerprintMismatch);
    }

    let mut paths = BTreeSet::new();
    for file in &manifest.files {
        if !valid_relative_resource_path(&file.path)
            || !valid_sha256(&file.sha256)
            || !paths.insert(file.path.as_str())
        {
            return Err(RetrievalDegradedReason::InvalidManifest);
        }
        let path = root.join(&file.path);
        let canonical = path
            .canonicalize()
            .map_err(|_| RetrievalDegradedReason::HashMismatch)?;
        if !canonical.starts_with(&root) || !canonical.is_file() {
            return Err(RetrievalDegradedReason::HashMismatch);
        }
        let actual = sha256_file(&canonical).map_err(|_| RetrievalDegradedReason::HashMismatch)?;
        if actual != file.sha256 {
            return Err(RetrievalDegradedReason::HashMismatch);
        }
    }
    if !paths.contains(manifest.helper_relative_path.as_str()) {
        return Err(RetrievalDegradedReason::InvalidManifest);
    }
    let helper_path = root.join(&manifest.helper_relative_path);
    Ok(VerifiedEmbeddingResource {
        directory: root,
        helper_path,
        manifest,
    })
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmbeddingCandidate {
    pub id: String,
    pub text: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmbeddingRequest {
    pub schema_version: SchemaVersion,
    pub query: String,
    pub catalog_fingerprint: String,
    pub vector_fingerprint: String,
    pub candidates: Vec<EmbeddingCandidate>,
}

impl EmbeddingRequest {
    fn valid(&self) -> bool {
        !self.query.trim().is_empty()
            && self.query.len() <= 4 * 1024
            && !self.candidates.is_empty()
            && self.candidates.len() <= MAX_CANDIDATES
            && self
                .candidates
                .iter()
                .all(|candidate| !candidate.id.is_empty() && candidate.text.len() <= 16 * 1024)
            && self
                .candidates
                .iter()
                .map(|candidate| candidate.id.as_str())
                .collect::<BTreeSet<_>>()
                .len()
                == self.candidates.len()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CandidateVector {
    pub id: String,
    pub vector: Vec<f32>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmbeddingOutput {
    pub schema_version: SchemaVersion,
    pub model_id: String,
    pub runtime_abi: String,
    pub catalog_fingerprint: String,
    pub vector_fingerprint: String,
    pub dimensions: usize,
    pub query_vector: Vec<f32>,
    pub candidates: Vec<CandidateVector>,
}

pub trait EmbeddingRunner: Send + Sync {
    fn embed<'a>(
        &'a self,
        resource: &'a VerifiedEmbeddingResource,
        request: &'a EmbeddingRequest,
    ) -> Pin<Box<dyn Future<Output = Result<EmbeddingOutput, RetrievalDegradedReason>> + Send + 'a>>;
}

#[derive(Clone, Debug)]
pub struct ProcessEmbeddingRunner {
    deadline: Duration,
}

impl ProcessEmbeddingRunner {
    #[must_use]
    pub fn new(deadline: Duration) -> Self {
        Self { deadline }
    }
}

impl Default for ProcessEmbeddingRunner {
    fn default() -> Self {
        Self::new(Duration::from_millis(150))
    }
}

impl EmbeddingRunner for ProcessEmbeddingRunner {
    fn embed<'a>(
        &'a self,
        resource: &'a VerifiedEmbeddingResource,
        request: &'a EmbeddingRequest,
    ) -> Pin<Box<dyn Future<Output = Result<EmbeddingOutput, RetrievalDegradedReason>> + Send + 'a>>
    {
        Box::pin(async move {
            if !request.valid() {
                return Err(RetrievalDegradedReason::MalformedVector);
            }
            let bytes = serde_json::to_vec(request)
                .map_err(|_| RetrievalDegradedReason::MalformedVector)?;
            if bytes.len() > MAX_HELPER_REQUEST_BYTES {
                return Err(RetrievalDegradedReason::MalformedVector);
            }
            let mut child = Command::new(&resource.helper_path)
                .args(["--protocol", EMBEDDING_HELPER_ABI])
                .current_dir(&resource.directory)
                .env_clear()
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .kill_on_drop(true)
                .spawn()
                .map_err(|_| RetrievalDegradedReason::HelperUnavailable)?;
            let mut stdin = child
                .stdin
                .take()
                .ok_or(RetrievalDegradedReason::HelperUnavailable)?;
            stdin
                .write_all(&bytes)
                .await
                .map_err(|_| RetrievalDegradedReason::HelperCrashed)?;
            stdin
                .shutdown()
                .await
                .map_err(|_| RetrievalDegradedReason::HelperCrashed)?;
            drop(stdin);
            let stdout = child
                .stdout
                .take()
                .ok_or(RetrievalDegradedReason::HelperUnavailable)?;
            let mut output = Vec::new();
            let execution = async {
                let mut bounded = stdout.take((MAX_HELPER_OUTPUT_BYTES + 1) as u64);
                let (_, status) = tokio::try_join!(bounded.read_to_end(&mut output), child.wait())
                    .map_err(|_| RetrievalDegradedReason::HelperCrashed)?;
                if !status.success() {
                    return Err(RetrievalDegradedReason::HelperCrashed);
                }
                Ok(())
            };
            match tokio::time::timeout(self.deadline, execution).await {
                Ok(result) => result?,
                Err(_) => {
                    let _ = child.kill().await;
                    let _ = child.wait().await;
                    return Err(RetrievalDegradedReason::HelperTimeout);
                }
            }
            if output.len() > MAX_HELPER_OUTPUT_BYTES {
                return Err(RetrievalDegradedReason::MalformedVector);
            }
            serde_json::from_slice(&output).map_err(|_| RetrievalDegradedReason::MalformedVector)
        })
    }
}

fn sha256_file(path: &Path) -> Result<String, std::io::Error> {
    let mut file = std::fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let bytes = file.read(&mut buffer)?;
        if bytes == 0 {
            break;
        }
        digest.update(&buffer[..bytes]);
    }
    Ok(encode_digest(digest.finalize()))
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_fingerprint(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(valid_sha256)
}

fn valid_relative_resource_path(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 512
        && !value.starts_with('/')
        && !value.starts_with('\\')
        && !value.contains('\\')
        && !value
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        && !value.chars().any(char::is_control)
}
