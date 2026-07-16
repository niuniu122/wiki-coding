use std::fs;
use std::io::Write as _;
use std::marker::PhantomData;
use std::path::Path;

use minimax_protocol::{IndexDomain, SchemaVersion};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::domain::DomainMarker;
use crate::{QUERY_TOKENIZER_VERSION, SearchDocument};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IndexSnapshot<D: SearchDocument> {
    pub schema_version: SchemaVersion,
    pub domain: IndexDomain,
    pub tokenizer_version: String,
    pub documents_hash: String,
    pub documents: Vec<D>,
    #[serde(skip)]
    marker: PhantomData<D::Marker>,
}

#[derive(Debug)]
pub enum SnapshotError {
    Io(std::io::Error),
    Json(serde_json::Error),
    DomainMismatch,
    TokenizerMismatch,
    DocumentsHashMismatch,
    ExpectedHashMismatch,
}

impl std::fmt::Display for SnapshotError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Io(_) => "snapshot I/O failed",
            Self::Json(_) => "snapshot JSON is invalid",
            Self::DomainMismatch => "snapshot domain does not match the typed index",
            Self::TokenizerMismatch => "snapshot tokenizer version is incompatible",
            Self::DocumentsHashMismatch => "snapshot document hash is invalid",
            Self::ExpectedHashMismatch => "snapshot expected hash does not match",
        })
    }
}

impl std::error::Error for SnapshotError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            _ => None,
        }
    }
}

impl From<std::io::Error> for SnapshotError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for SnapshotError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

impl<D> IndexSnapshot<D>
where
    D: SearchDocument + Serialize,
{
    pub fn new(documents: Vec<D>) -> Result<Self, SnapshotError> {
        let documents_hash = hash_bytes(&serde_json::to_vec(&documents)?);
        Ok(Self {
            schema_version: SchemaVersion,
            domain: <D::Marker as DomainMarker>::DOMAIN,
            tokenizer_version: QUERY_TOKENIZER_VERSION.to_owned(),
            documents_hash,
            documents,
            marker: PhantomData,
        })
    }
}

impl<D> IndexSnapshot<D>
where
    D: SearchDocument + DeserializeOwned + Serialize,
{
    pub fn from_slice(bytes: &[u8]) -> Result<Self, SnapshotError> {
        let value: serde_json::Value = serde_json::from_slice(bytes)?;
        let serialized_domain = value
            .get("domain")
            .cloned()
            .ok_or_else(|| SnapshotError::Json(missing_field_error("domain")))?;
        let domain: IndexDomain = serde_json::from_value(serialized_domain)?;
        if domain != <D::Marker as DomainMarker>::DOMAIN {
            return Err(SnapshotError::DomainMismatch);
        }
        let snapshot: Self = serde_json::from_value(value)?;
        if snapshot.tokenizer_version != QUERY_TOKENIZER_VERSION {
            return Err(SnapshotError::TokenizerMismatch);
        }
        let actual = hash_bytes(&serde_json::to_vec(&snapshot.documents)?);
        if snapshot.documents_hash != actual {
            return Err(SnapshotError::DocumentsHashMismatch);
        }
        Ok(snapshot)
    }
}

pub fn load_snapshot<D>(path: &Path) -> Result<IndexSnapshot<D>, SnapshotError>
where
    D: SearchDocument + DeserializeOwned + Serialize,
{
    IndexSnapshot::from_slice(&fs::read(path)?)
}

pub fn publish_snapshot<D>(
    path: &Path,
    expected_previous_hash: Option<&str>,
    snapshot: &IndexSnapshot<D>,
) -> Result<String, SnapshotError>
where
    D: SearchDocument + Serialize,
{
    let current_hash = if path.exists() {
        Some(snapshot_file_hash(path)?)
    } else {
        None
    };
    if current_hash.as_deref() != expected_previous_hash {
        return Err(SnapshotError::ExpectedHashMismatch);
    }
    let parent = path.parent().ok_or_else(|| {
        SnapshotError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "snapshot path has no parent",
        ))
    })?;
    fs::create_dir_all(parent)?;
    let bytes = serde_json::to_vec_pretty(snapshot)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(&bytes)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(path)
        .map_err(|error| SnapshotError::Io(error.error))?;
    Ok(hash_bytes(&bytes))
}

pub fn snapshot_file_hash(path: &Path) -> Result<String, SnapshotError> {
    Ok(hash_bytes(&fs::read(path)?))
}

fn hash_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn missing_field_error(field: &'static str) -> serde_json::Error {
    <serde_json::Error as serde::de::Error>::missing_field(field)
}
