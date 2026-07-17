use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{Read as _, Write as _};
use std::path::{Component, Path, PathBuf};

use minimax_core::SessionMachine;
use minimax_protocol::SessionRecordV1;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest as _, Sha256};

const SCHEMA_VERSION: u16 = 1;
const TARGET_SCHEMA: &str = "minimax-rust-v1";
const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 128 * 1024 * 1024;
const MAX_FILES: usize = 10_000;
const MAX_PLAN_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MigrationError {
    Source,
    Target,
    Read,
    Write,
    Bounds,
    Symlink,
    Malformed,
    Secret,
    Collision,
    Drift,
    Confirmation,
    Plan,
    Receipt,
    TargetChanged,
    Busy,
    Serialization,
    Recovery,
}

impl MigrationError {
    #[must_use]
    pub const fn is_usage(&self) -> bool {
        matches!(
            self,
            Self::Source
                | Self::Target
                | Self::Bounds
                | Self::Symlink
                | Self::Malformed
                | Self::Secret
                | Self::Collision
                | Self::Drift
                | Self::Confirmation
                | Self::Plan
                | Self::Receipt
                | Self::TargetChanged
        )
    }
}

impl fmt::Display for MigrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Source => "the TypeScript source root is missing or invalid",
            Self::Target => "the Rust target root is missing, invalid, or unsafe",
            Self::Read => "migration input could not be read",
            Self::Write => "migration output could not be written durably",
            Self::Bounds => "migration input exceeds the file, count, or byte limit",
            Self::Symlink => "migration rejected a symlink in an allowlisted path",
            Self::Malformed => "migration input does not match the supported TypeScript schema",
            Self::Secret => "migration input contains secret-looking or private content",
            Self::Collision => "a target exists with different bytes",
            Self::Drift => "the source changed after the dry-run plan was created",
            Self::Confirmation => "the exact plan or receipt confirmation is required",
            Self::Plan => "the migration plan is invalid or has been changed",
            Self::Receipt => "the migration receipt is invalid or has been changed",
            Self::TargetChanged => "a receipt-owned target is missing or has changed",
            Self::Busy => "another migration operation is active for this target",
            Self::Serialization => "migration evidence could not be serialized",
            Self::Recovery => "an interrupted migration could not be recovered safely",
        })
    }
}

impl std::error::Error for MigrationError {}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MigrationSourceItem {
    pub relative_path: String,
    pub kind: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MigrationExclusion {
    pub relative_path: String,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MigrationTarget {
    pub relative_path: String,
    pub kind: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MigrationCollision {
    pub relative_path: String,
    pub expected_sha256: String,
    pub actual_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MigrationInventory {
    pub schema_version: u16,
    pub source_root: String,
    pub target_root: String,
    pub source_fingerprint: String,
    pub target_schema: String,
    pub included: Vec<MigrationSourceItem>,
    pub excluded: Vec<MigrationExclusion>,
    pub targets: Vec<MigrationTarget>,
    pub collisions: Vec<MigrationCollision>,
}

impl MigrationInventory {
    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "migration inventory | included={} | excluded={} | targets={} | collisions={} | source={}",
            self.included.len(),
            self.excluded.len(),
            self.targets.len(),
            self.collisions.len(),
            self.source_fingerprint
        )
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MigrationPlanBody {
    schema_version: u16,
    migration_id: String,
    source_root: String,
    target_root: String,
    source_fingerprint: String,
    target_schema: String,
    included: Vec<MigrationSourceItem>,
    excluded: Vec<MigrationExclusion>,
    targets: Vec<MigrationTarget>,
    collisions: Vec<MigrationCollision>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MigrationPlan {
    pub schema_version: u16,
    pub migration_id: String,
    pub source_root: String,
    pub target_root: String,
    pub source_fingerprint: String,
    pub target_schema: String,
    pub included: Vec<MigrationSourceItem>,
    pub excluded: Vec<MigrationExclusion>,
    pub targets: Vec<MigrationTarget>,
    pub collisions: Vec<MigrationCollision>,
    pub plan_hash: String,
}

impl MigrationPlan {
    #[must_use]
    pub fn confirmation(&self) -> String {
        format!("MIGRATE:{}", self.plan_hash)
    }

    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "migration dry-run | id={} | plan={} | targets={} | excluded={} | collisions={} | confirmation={}",
            self.migration_id,
            self.plan_hash,
            self.targets.len(),
            self.excluded.len(),
            self.collisions.len(),
            self.confirmation()
        )
    }

    fn body(&self) -> MigrationPlanBody {
        MigrationPlanBody {
            schema_version: self.schema_version,
            migration_id: self.migration_id.clone(),
            source_root: self.source_root.clone(),
            target_root: self.target_root.clone(),
            source_fingerprint: self.source_fingerprint.clone(),
            target_schema: self.target_schema.clone(),
            included: self.included.clone(),
            excluded: self.excluded.clone(),
            targets: self.targets.clone(),
            collisions: self.collisions.clone(),
        }
    }

    fn validate(&self) -> Result<(), MigrationError> {
        if self.schema_version != SCHEMA_VERSION
            || self.target_schema != TARGET_SCHEMA
            || self.migration_id.is_empty()
            || hash_serializable(&self.body())? != self.plan_hash
        {
            return Err(MigrationError::Plan);
        }
        validate_unique_targets(&self.targets)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MigrationReceiptTarget {
    pub relative_path: String,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MigrationReceiptBody {
    schema_version: u16,
    migration_id: String,
    plan_hash: String,
    source_root: String,
    target_root: String,
    source_fingerprint: String,
    created: Vec<MigrationReceiptTarget>,
    reused: Vec<MigrationReceiptTarget>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MigrationReceipt {
    pub schema_version: u16,
    pub migration_id: String,
    pub plan_hash: String,
    pub source_root: String,
    pub target_root: String,
    pub source_fingerprint: String,
    pub created: Vec<MigrationReceiptTarget>,
    pub reused: Vec<MigrationReceiptTarget>,
    pub receipt_hash: String,
}

impl MigrationReceipt {
    #[must_use]
    pub fn confirmation(&self) -> String {
        format!("ROLLBACK:{}", self.receipt_hash)
    }

    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "migration applied | id={} | created={} | reused={} | receipt={} | rollback={}",
            self.migration_id,
            self.created.len(),
            self.reused.len(),
            self.receipt_hash,
            self.confirmation()
        )
    }

    fn body(&self) -> MigrationReceiptBody {
        MigrationReceiptBody {
            schema_version: self.schema_version,
            migration_id: self.migration_id.clone(),
            plan_hash: self.plan_hash.clone(),
            source_root: self.source_root.clone(),
            target_root: self.target_root.clone(),
            source_fingerprint: self.source_fingerprint.clone(),
            created: self.created.clone(),
            reused: self.reused.clone(),
        }
    }

    fn validate(&self) -> Result<(), MigrationError> {
        if self.schema_version != SCHEMA_VERSION
            || self.migration_id.is_empty()
            || hash_serializable(&self.body())? != self.receipt_hash
        {
            return Err(MigrationError::Receipt);
        }
        validate_receipt_ownership(self)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MigrationVerifyReport {
    pub schema_version: u16,
    pub migration_id: String,
    pub receipt_hash: String,
    pub source_unchanged: bool,
    pub targets_verified: usize,
    pub rolled_back: bool,
}

impl MigrationVerifyReport {
    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "migration verification | id={} | targets={} | source_unchanged={} | rolled_back={}",
            self.migration_id, self.targets_verified, self.source_unchanged, self.rolled_back
        )
    }
}

#[derive(Clone)]
struct SourceFile {
    relative_path: String,
    bytes: Vec<u8>,
    sha256: String,
    kind: SourceKind,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum SourceKind {
    Config,
    Threads,
    Session,
    Turn,
    Capability,
    Excluded,
}

impl SourceKind {
    const fn label(self) -> &'static str {
        match self {
            Self::Config => "safe_config",
            Self::Threads => "thread_index",
            Self::Session => "session_messages_and_tools",
            Self::Turn => "turns",
            Self::Capability => "capability_metadata",
            Self::Excluded => "excluded",
        }
    }
}

struct BuiltMigration {
    plan: MigrationPlan,
    artifacts: BTreeMap<String, Vec<u8>>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OperationManifest {
    schema_version: u16,
    plan_hash: String,
    candidates: Vec<MigrationReceiptTarget>,
}

pub fn inventory_migration(
    source_root: &Path,
    target_root: &Path,
) -> Result<MigrationInventory, MigrationError> {
    let built = build_migration(source_root, target_root)?;
    Ok(MigrationInventory {
        schema_version: built.plan.schema_version,
        source_root: built.plan.source_root,
        target_root: built.plan.target_root,
        source_fingerprint: built.plan.source_fingerprint,
        target_schema: built.plan.target_schema,
        included: built.plan.included,
        excluded: built.plan.excluded,
        targets: built.plan.targets,
        collisions: built.plan.collisions,
    })
}

pub fn build_migration_plan(
    source_root: &Path,
    target_root: &Path,
) -> Result<MigrationPlan, MigrationError> {
    build_migration(source_root, target_root).map(|built| built.plan)
}

pub fn apply_migration(
    plan_path: &Path,
    confirmation: &str,
) -> Result<MigrationReceipt, MigrationError> {
    let plan: MigrationPlan = read_json_bounded(plan_path, MAX_PLAN_BYTES, MigrationError::Plan)?;
    plan.validate()?;
    if confirmation != plan.confirmation() {
        return Err(MigrationError::Confirmation);
    }
    if !plan.collisions.is_empty() {
        return Err(MigrationError::Collision);
    }
    let target_root = canonical_directory(Path::new(&plan.target_root), MigrationError::Target)?;
    let _lock = MigrationLock::acquire(&target_root)?;
    let receipt_path = receipt_path(&target_root, &plan.migration_id)?;
    if receipt_path.is_file() {
        let receipt = read_receipt(&receipt_path)?;
        if receipt.plan_hash != plan.plan_hash {
            return Err(MigrationError::Receipt);
        }
        verify_receipt_targets(&receipt)?;
        return Ok(receipt);
    }
    recover_interrupted(
        &target_root,
        &plan.migration_id,
        &plan.plan_hash,
        &plan.targets,
    )?;
    let rebuilt = build_migration(Path::new(&plan.source_root), &target_root)?;
    if rebuilt.plan.body() != plan.body() || rebuilt.plan.plan_hash != plan.plan_hash {
        return Err(MigrationError::Drift);
    }

    let operation_dir = operation_directory(&target_root, &plan.migration_id)?;
    let staging = operation_dir.join("staging");
    std::fs::create_dir_all(&staging).map_err(|_| MigrationError::Write)?;
    for (relative, bytes) in &rebuilt.artifacts {
        let staged = safe_join(&staging, relative)?;
        write_new_file(&staged, bytes)?;
    }

    let mut candidates = Vec::new();
    let mut reused = Vec::new();
    for target in &plan.targets {
        let destination = safe_join(&target_root, &target.relative_path)?;
        if destination.is_file() {
            if hash_file(&destination)? != target.sha256 {
                return Err(MigrationError::Collision);
            }
            reused.push(receipt_target(target));
        } else {
            candidates.push(receipt_target(target));
        }
    }
    let manifest = OperationManifest {
        schema_version: SCHEMA_VERSION,
        plan_hash: plan.plan_hash.clone(),
        candidates: candidates.clone(),
    };
    write_json_new(&operation_dir.join("operation.json"), &manifest)?;

    let mut created = Vec::new();
    let publish = (|| {
        for candidate in &candidates {
            let staged = safe_join(&staging, &candidate.relative_path)?;
            let bytes = std::fs::read(&staged).map_err(|_| MigrationError::Read)?;
            let destination = safe_join(&target_root, &candidate.relative_path)?;
            write_new_file(&destination, &bytes)?;
            created.push(candidate.clone());
        }
        verify_targets(&target_root, created.iter().chain(&reused))?;
        let body = MigrationReceiptBody {
            schema_version: SCHEMA_VERSION,
            migration_id: plan.migration_id.clone(),
            plan_hash: plan.plan_hash.clone(),
            source_root: plan.source_root.clone(),
            target_root: plan.target_root.clone(),
            source_fingerprint: plan.source_fingerprint.clone(),
            created: created.clone(),
            reused: reused.clone(),
        };
        let receipt = MigrationReceipt {
            schema_version: body.schema_version,
            migration_id: body.migration_id.clone(),
            plan_hash: body.plan_hash.clone(),
            source_root: body.source_root.clone(),
            target_root: body.target_root.clone(),
            source_fingerprint: body.source_fingerprint.clone(),
            created: body.created.clone(),
            reused: body.reused.clone(),
            receipt_hash: hash_serializable(&body)?,
        };
        write_json_new(&receipt_path, &receipt)?;
        Ok(receipt)
    })();
    match publish {
        Ok(receipt) => {
            remove_operation_files(&operation_dir)?;
            Ok(receipt)
        }
        Err(error) => {
            rollback_created(&target_root, &created)?;
            let _ = remove_operation_files(&operation_dir);
            Err(error)
        }
    }
}

pub fn verify_migration(receipt_path: &Path) -> Result<MigrationVerifyReport, MigrationError> {
    let receipt = read_receipt(receipt_path)?;
    let target_root = canonical_directory(Path::new(&receipt.target_root), MigrationError::Target)?;
    let rollback_path = rollback_receipt_path(&target_root, &receipt.migration_id)?;
    if rollback_path.is_file() {
        return Ok(MigrationVerifyReport {
            schema_version: SCHEMA_VERSION,
            migration_id: receipt.migration_id,
            receipt_hash: receipt.receipt_hash,
            source_unchanged: source_fingerprint(Path::new(&receipt.source_root))?
                == receipt.source_fingerprint,
            targets_verified: 0,
            rolled_back: true,
        });
    }
    verify_receipt_targets(&receipt)?;
    Ok(MigrationVerifyReport {
        schema_version: SCHEMA_VERSION,
        migration_id: receipt.migration_id,
        receipt_hash: receipt.receipt_hash,
        source_unchanged: source_fingerprint(Path::new(&receipt.source_root))?
            == receipt.source_fingerprint,
        targets_verified: receipt.created.len() + receipt.reused.len(),
        rolled_back: false,
    })
}

pub fn rollback_migration(
    receipt_path: &Path,
    confirmation: &str,
) -> Result<MigrationVerifyReport, MigrationError> {
    let receipt = read_receipt(receipt_path)?;
    if confirmation != receipt.confirmation() {
        return Err(MigrationError::Confirmation);
    }
    let target_root = canonical_directory(Path::new(&receipt.target_root), MigrationError::Target)?;
    let _lock = MigrationLock::acquire(&target_root)?;
    let rollback_path = rollback_receipt_path(&target_root, &receipt.migration_id)?;
    if rollback_path.is_file() {
        return read_json_bounded(&rollback_path, MAX_PLAN_BYTES, MigrationError::Receipt);
    }
    verify_receipt_targets(&receipt)?;
    rollback_created(&target_root, &receipt.created)?;
    let report = MigrationVerifyReport {
        schema_version: SCHEMA_VERSION,
        migration_id: receipt.migration_id,
        receipt_hash: receipt.receipt_hash,
        source_unchanged: source_fingerprint(Path::new(&receipt.source_root))?
            == receipt.source_fingerprint,
        targets_verified: receipt.reused.len(),
        rolled_back: true,
    };
    write_json_new(&rollback_path, &report)?;
    Ok(report)
}

fn build_migration(
    source_root: &Path,
    target_root: &Path,
) -> Result<BuiltMigration, MigrationError> {
    let source_root = canonical_directory(source_root, MigrationError::Source)?;
    let target_root = canonical_directory(target_root, MigrationError::Target)?;
    if source_root == target_root || target_root.starts_with(&source_root) {
        return Err(MigrationError::Target);
    }
    let (files, mut excluded, source_fingerprint) = scan_source(&source_root)?;
    let included = files
        .iter()
        .filter(|file| file.kind != SourceKind::Excluded)
        .map(|file| MigrationSourceItem {
            relative_path: file.relative_path.clone(),
            kind: file.kind.label().to_owned(),
            bytes: file.bytes.len() as u64,
            sha256: file.sha256.clone(),
        })
        .collect::<Vec<_>>();
    let migration_id = format!("ts-{}", &source_fingerprint[..16]);
    let normalization = normalize_source(&files, &migration_id)?;
    excluded.extend(normalization.excluded);
    excluded.sort_by(|left, right| {
        left.relative_path
            .cmp(&right.relative_path)
            .then_with(|| left.reason.cmp(&right.reason))
    });

    let mut artifacts = BTreeMap::new();
    if let Some(config) = normalization.config {
        artifacts.insert(".minimax/config.json".to_owned(), config);
    }
    if !normalization.sessions.is_empty() {
        artifacts.insert(
            ".minimax/runtime/v1/sessions.jsonl".to_owned(),
            normalization.sessions,
        );
    }
    if let Some(capabilities) = normalization.capabilities {
        artifacts.insert(
            format!(".minimax/migrations/v1/{migration_id}/capability-metadata.json"),
            capabilities,
        );
    }
    let targets = artifacts
        .iter()
        .map(|(relative_path, bytes)| MigrationTarget {
            relative_path: relative_path.clone(),
            kind: target_kind(relative_path).to_owned(),
            bytes: bytes.len() as u64,
            sha256: sha256(bytes),
        })
        .collect::<Vec<_>>();
    let collisions = find_collisions(&target_root, &targets)?;
    let body = MigrationPlanBody {
        schema_version: SCHEMA_VERSION,
        migration_id,
        source_root: path_string(&source_root),
        target_root: path_string(&target_root),
        source_fingerprint,
        target_schema: TARGET_SCHEMA.to_owned(),
        included,
        excluded,
        targets,
        collisions,
    };
    let plan = MigrationPlan {
        schema_version: body.schema_version,
        migration_id: body.migration_id.clone(),
        source_root: body.source_root.clone(),
        target_root: body.target_root.clone(),
        source_fingerprint: body.source_fingerprint.clone(),
        target_schema: body.target_schema.clone(),
        included: body.included.clone(),
        excluded: body.excluded.clone(),
        targets: body.targets.clone(),
        collisions: body.collisions.clone(),
        plan_hash: hash_serializable(&body)?,
    };
    Ok(BuiltMigration { plan, artifacts })
}

struct NormalizedSource {
    config: Option<Vec<u8>>,
    sessions: Vec<u8>,
    capabilities: Option<Vec<u8>>,
    excluded: Vec<MigrationExclusion>,
}

fn normalize_source(
    files: &[SourceFile],
    migration_id: &str,
) -> Result<NormalizedSource, MigrationError> {
    let mut excluded = Vec::new();
    let config = files
        .iter()
        .find(|file| file.kind == SourceKind::Config)
        .map(|file| normalize_config(&file.bytes))
        .transpose()?;
    let sessions = normalize_sessions(files, migration_id, &mut excluded)?;
    let capabilities = normalize_capabilities(files, &mut excluded)?;
    Ok(NormalizedSource {
        config,
        sessions,
        capabilities,
        excluded,
    })
}

fn normalize_config(bytes: &[u8]) -> Result<Vec<u8>, MigrationError> {
    let value: Value = serde_json::from_slice(bytes).map_err(|_| MigrationError::Malformed)?;
    if contains_private_reasoning(&value) {
        return Err(MigrationError::Secret);
    }
    let object = value.as_object().ok_or(MigrationError::Malformed)?;
    if object.get("schemaVersion").and_then(Value::as_u64) != Some(1) {
        return Err(MigrationError::Malformed);
    }
    let selected = object
        .get("modelProvider")
        .and_then(Value::as_str)
        .ok_or(MigrationError::Malformed)?;
    let provider = object
        .get("modelProviders")
        .and_then(Value::as_object)
        .and_then(|providers| providers.get(selected))
        .and_then(Value::as_object)
        .ok_or(MigrationError::Malformed)?;
    let endpoint = provider
        .get("baseUrl")
        .and_then(Value::as_str)
        .ok_or(MigrationError::Malformed)?;
    let protocol = provider
        .get("protocol")
        .and_then(Value::as_str)
        .ok_or(MigrationError::Malformed)?;
    if !matches!(protocol, "responses" | "chat_completions") {
        return Err(MigrationError::Malformed);
    }
    let model = object
        .get("model")
        .and_then(Value::as_str)
        .or_else(|| provider.get("defaultModel").and_then(Value::as_str))
        .ok_or(MigrationError::Malformed)?;
    if [selected, endpoint, protocol, model]
        .iter()
        .any(|value| secret_string(value))
    {
        return Err(MigrationError::Secret);
    }
    let mut output = Map::new();
    output.insert("schemaVersion".to_owned(), json!(1));
    output.insert("providerId".to_owned(), json!(selected));
    output.insert("endpoint".to_owned(), json!(endpoint));
    output.insert("protocol".to_owned(), json!(protocol));
    output.insert("model".to_owned(), json!(model));
    if let Some(key) = provider.get("envKey").and_then(Value::as_str) {
        if key.is_empty()
            || key.len() > 64
            || !key.bytes().enumerate().all(|(index, byte)| {
                (index == 0 && byte.is_ascii_uppercase())
                    || (index > 0
                        && (byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_'))
            })
        {
            return Err(MigrationError::Malformed);
        }
        output.insert("environmentKey".to_owned(), json!(key));
    }
    if let Some(limit) = object
        .get("context")
        .and_then(Value::as_object)
        .and_then(|context| context.get("maxCompletionTokens"))
        .and_then(Value::as_u64)
        && let Ok(limit) = u32::try_from(limit)
    {
        output.insert("maxOutputTokens".to_owned(), json!(limit));
    }
    if let Some(loopback) = provider
        .get("allowInsecureLoopback")
        .and_then(Value::as_bool)
    {
        output.insert("allowInsecureLoopback".to_owned(), json!(loopback));
    }
    pretty_json_bytes(&Value::Object(output))
}

fn normalize_sessions(
    files: &[SourceFile],
    migration_id: &str,
    excluded: &mut Vec<MigrationExclusion>,
) -> Result<Vec<u8>, MigrationError> {
    let Some(thread_file) = files.iter().find(|file| file.kind == SourceKind::Threads) else {
        if files
            .iter()
            .any(|file| matches!(file.kind, SourceKind::Session | SourceKind::Turn))
        {
            return Err(MigrationError::Malformed);
        }
        return Ok(Vec::new());
    };
    let index: Value =
        serde_json::from_slice(&thread_file.bytes).map_err(|_| MigrationError::Malformed)?;
    if contains_private_reasoning(&index) {
        return Err(MigrationError::Secret);
    }
    let threads = index
        .get("threads")
        .and_then(Value::as_array)
        .ok_or(MigrationError::Malformed)?;
    let mut items_by_thread: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let mut turns_by_thread: BTreeMap<String, BTreeMap<String, Value>> = BTreeMap::new();
    let mut deltas_by_turn: BTreeMap<String, String> = BTreeMap::new();
    for file in files.iter().filter(|file| file.kind == SourceKind::Session) {
        for value in parse_jsonl(&file.bytes)? {
            let payload = envelope_payload(value, "thread.item")?;
            if contains_private_reasoning(&payload) {
                return Err(MigrationError::Secret);
            }
            let thread_id = payload
                .get("threadId")
                .and_then(Value::as_str)
                .ok_or(MigrationError::Malformed)?;
            items_by_thread
                .entry(thread_id.to_owned())
                .or_default()
                .push(payload);
        }
    }
    for file in files.iter().filter(|file| file.kind == SourceKind::Turn) {
        for value in parse_jsonl(&file.bytes)? {
            let (kind, payload) = envelope_or_legacy_turn(value)?;
            if contains_private_reasoning(&payload) {
                return Err(MigrationError::Secret);
            }
            if kind == "turn.snapshot" {
                let thread_id = payload
                    .get("threadId")
                    .and_then(Value::as_str)
                    .ok_or(MigrationError::Malformed)?;
                let turn_id = payload
                    .get("id")
                    .and_then(Value::as_str)
                    .ok_or(MigrationError::Malformed)?;
                turns_by_thread
                    .entry(thread_id.to_owned())
                    .or_default()
                    .insert(turn_id.to_owned(), payload);
            } else {
                let turn_id = payload
                    .get("turnId")
                    .and_then(Value::as_str)
                    .ok_or(MigrationError::Malformed)?;
                let delta = payload
                    .get("delta")
                    .and_then(Value::as_str)
                    .ok_or(MigrationError::Malformed)?;
                if secret_string(delta) {
                    excluded.push(MigrationExclusion {
                        relative_path: format!("record:{turn_id}:assistant_delta"),
                        reason: "secret_looking_record".to_owned(),
                    });
                } else {
                    deltas_by_turn
                        .entry(turn_id.to_owned())
                        .or_default()
                        .push_str(delta);
                }
            }
        }
    }

    let mut records = Vec::new();
    let mut sorted_threads = threads.iter().collect::<Vec<_>>();
    sorted_threads.sort_by_key(|thread| thread.get("id").and_then(Value::as_str).unwrap_or(""));
    for thread in sorted_threads {
        let thread_id = required_string(thread, "id")?;
        let model = required_string(thread, "model")?;
        let created = timestamp_ms(required_string(thread, "createdAt")?)?;
        let updated = timestamp_ms(required_string(thread, "updatedAt")?)?;
        if secret_string(model) {
            return Err(MigrationError::Secret);
        }
        let status = if thread.get("status").and_then(Value::as_str) == Some("active") {
            "active"
        } else {
            "archived"
        };
        let items = items_by_thread.get(thread_id).cloned().unwrap_or_default();
        let mut turns = Vec::new();
        if let Some(source_turns) = turns_by_thread.get(thread_id) {
            for turn in source_turns.values() {
                if let Some(normalized) = normalize_turn(turn, &items, &deltas_by_turn, excluded)? {
                    turns.push(normalized);
                }
            }
        }
        turns.sort_by_key(|turn| {
            turn.get("startedAtUnixMs")
                .and_then(Value::as_u64)
                .unwrap_or(0)
        });
        let binding = binding_for_turns(&turns, model)?;
        let session = json!({
            "sessionId": imported_id("session", thread_id),
            "createdAtUnixMs": created,
            "updatedAtUnixMs": updated,
            "status": status,
            "binding": binding,
            "turns": turns
        });
        let record = json!({
            "schemaVersion": 1,
            "recordId": format!("migration:{migration_id}:{}", imported_id("record", thread_id)),
            "record": {"type": "session_created", "session": session}
        });
        records.push(record);
    }
    let mut bytes = Vec::new();
    let mut typed = Vec::new();
    for record in records {
        let line = serde_json::to_vec(&record).map_err(|_| MigrationError::Serialization)?;
        let parsed: SessionRecordV1 =
            serde_json::from_slice(&line).map_err(|_| MigrationError::Malformed)?;
        typed.push(parsed);
        bytes.extend_from_slice(&line);
        bytes.push(b'\n');
    }
    SessionMachine::replay(typed).map_err(|_| MigrationError::Malformed)?;
    Ok(bytes)
}

fn normalize_turn(
    turn: &Value,
    items: &[Value],
    deltas: &BTreeMap<String, String>,
    excluded: &mut Vec<MigrationExclusion>,
) -> Result<Option<Value>, MigrationError> {
    let turn_id = required_string(turn, "id")?;
    let user = required_string(turn, "userInput")?;
    if secret_string(user) {
        excluded.push(MigrationExclusion {
            relative_path: format!("record:{turn_id}:user_message"),
            reason: "secret_looking_record".to_owned(),
        });
        return Ok(None);
    }
    let started = timestamp_ms(required_string(turn, "startedAt")?)?;
    let completed = turn
        .get("completedAt")
        .and_then(Value::as_str)
        .map(timestamp_ms)
        .transpose()?;
    let source_status = turn
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("interrupted");
    let status = match source_status {
        "completed" => "completed",
        "failed" => "failed",
        "running" | "interrupted" => "interrupted",
        _ => return Err(MigrationError::Malformed),
    };
    let turn_items = items
        .iter()
        .filter(|item| item.get("turnId").and_then(Value::as_str) == Some(turn_id))
        .collect::<Vec<_>>();
    let mut assistant = turn_items
        .iter()
        .rev()
        .find(|item| {
            item.get("type").and_then(Value::as_str) == Some("assistant_message")
                && item.get("role").and_then(Value::as_str) == Some("assistant")
        })
        .and_then(|item| item.get("content"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| deltas.get(turn_id).cloned())
        .or_else(|| {
            turn.get("assistantDraft")
                .and_then(Value::as_str)
                .map(str::to_owned)
        });
    if assistant.as_deref().is_some_and(secret_string) {
        excluded.push(MigrationExclusion {
            relative_path: format!("record:{turn_id}:assistant_message"),
            reason: "secret_looking_record".to_owned(),
        });
        assistant = None;
    }
    let tools = normalize_tools(turn_id, &turn_items, excluded)?;
    let mut object = Map::new();
    object.insert("turnId".to_owned(), json!(imported_id("turn", turn_id)));
    object.insert(
        "requestId".to_owned(),
        json!(imported_id("request", turn_id)),
    );
    object.insert("startedAtUnixMs".to_owned(), json!(started));
    if let Some(completed) = completed.or((status == "interrupted").then_some(started)) {
        object.insert("completedAtUnixMs".to_owned(), json!(completed));
    }
    object.insert("status".to_owned(), json!(status));
    object.insert(
        "userMessage".to_owned(),
        json!({"role": "user", "content": user, "partial": false}),
    );
    if let Some(assistant) = assistant {
        object.insert(
            "assistantMessage".to_owned(),
            json!({
                "role": "assistant",
                "content": assistant,
                "partial": status != "completed"
            }),
        );
    }
    if !tools.is_empty() {
        object.insert("toolInvocations".to_owned(), Value::Array(tools));
    }
    Ok(Some(Value::Object(object)))
}

fn binding_for_turns(_turns: &[Value], fallback_model: &str) -> Result<Value, MigrationError> {
    if secret_string(fallback_model) {
        return Err(MigrationError::Secret);
    }
    Ok(json!({
        "providerId": "minimax-official",
        "modelId": fallback_model,
        "protocol": "responses"
    }))
}

fn normalize_tools(
    turn_id: &str,
    items: &[&Value],
    excluded: &mut Vec<MigrationExclusion>,
) -> Result<Vec<Value>, MigrationError> {
    let mut requests = BTreeMap::<String, (&str, Value, u64)>::new();
    let mut results = BTreeMap::<String, (&str, &str, u64)>::new();
    for item in items {
        if item.get("type").and_then(Value::as_str) != Some("agent_item") {
            continue;
        }
        let payload = item
            .get("agent")
            .and_then(|agent| agent.get("payload"))
            .and_then(Value::as_object)
            .ok_or(MigrationError::Malformed)?;
        let timestamp = timestamp_ms(required_string(item, "createdAt")?)?;
        match payload.get("kind").and_then(Value::as_str) {
            Some("tool_request") => {
                let invocation = payload
                    .get("invocationId")
                    .and_then(Value::as_str)
                    .ok_or(MigrationError::Malformed)?;
                let capability = payload
                    .get("capabilityId")
                    .and_then(Value::as_str)
                    .ok_or(MigrationError::Malformed)?;
                let arguments = payload
                    .get("arguments")
                    .cloned()
                    .ok_or(MigrationError::Malformed)?;
                if contains_secret(&arguments) {
                    excluded.push(MigrationExclusion {
                        relative_path: format!("record:{turn_id}:tool:{invocation}"),
                        reason: "secret_looking_tool_arguments".to_owned(),
                    });
                } else {
                    requests.insert(invocation.to_owned(), (capability, arguments, timestamp));
                }
            }
            Some("tool_result") => {
                let invocation = payload
                    .get("invocationId")
                    .and_then(Value::as_str)
                    .ok_or(MigrationError::Malformed)?;
                let status = payload
                    .get("status")
                    .and_then(Value::as_str)
                    .ok_or(MigrationError::Malformed)?;
                let output = payload
                    .get("output")
                    .and_then(Value::as_str)
                    .ok_or(MigrationError::Malformed)?;
                if secret_string(output) {
                    excluded.push(MigrationExclusion {
                        relative_path: format!("record:{turn_id}:tool-result:{invocation}"),
                        reason: "secret_looking_tool_result".to_owned(),
                    });
                } else {
                    results.insert(invocation.to_owned(), (status, output, timestamp));
                }
            }
            Some("user" | "assistant" | "final" | "checkpoint" | "error") => {}
            _ => return Err(MigrationError::Malformed),
        }
    }
    let mut normalized = Vec::new();
    for (invocation_id, (capability, arguments, requested_at)) in requests {
        let tool_name = safe_tool_name(capability);
        let terminal = results.get(&invocation_id);
        let terminal_status = terminal.map_or("indeterminate", |(status, _, _)| match *status {
            "completed" => "succeeded",
            "failed" => "failed",
            _ => "indeterminate",
        });
        let terminal_at = terminal.map_or(requested_at, |(_, _, at)| *at);
        let output = terminal.map(|(_, output, _)| *output);
        normalized.push(json!({
            "invocation": {
                "schema_version": 1,
                "call": {
                    "schema_version": 1,
                    "call_id": imported_id("call", &invocation_id),
                    "name": tool_name,
                    "arguments_json": serde_json::to_string(&arguments).map_err(|_| MigrationError::Serialization)?
                },
                "effect": if capability.contains("read") || capability.contains("list") {"read"} else {"process"}
            },
            "requestedAtUnixMs": requested_at,
            "decision": {
                "schema_version": 1,
                "call_id": imported_id("call", &invocation_id),
                "decision": "approved",
                "code": "legacy_import"
            },
            "decisionAtUnixMs": requested_at,
            "startedAtUnixMs": requested_at,
            "terminalResult": {
                "schema_version": 1,
                "call_id": imported_id("call", &invocation_id),
                "tool_name": tool_name,
                "status": terminal_status,
                "code": "legacy_import",
                "output": output
            },
            "terminalAtUnixMs": terminal_at
        }));
    }
    Ok(normalized)
}

fn normalize_capabilities(
    files: &[SourceFile],
    excluded: &mut Vec<MigrationExclusion>,
) -> Result<Option<Vec<u8>>, MigrationError> {
    let mut entries = Vec::new();
    for file in files
        .iter()
        .filter(|file| file.kind == SourceKind::Capability)
    {
        let value: Value =
            serde_json::from_slice(&file.bytes).map_err(|_| MigrationError::Malformed)?;
        if contains_private_reasoning(&value) {
            return Err(MigrationError::Secret);
        }
        if contains_secret(&value) {
            excluded.push(MigrationExclusion {
                relative_path: file.relative_path.clone(),
                reason: "secret_looking_capability_metadata".to_owned(),
            });
            continue;
        }
        entries.push(json!({
            "relativePath": file.relative_path,
            "sha256": file.sha256,
            "document": value
        }));
    }
    if entries.is_empty() {
        Ok(None)
    } else {
        pretty_json_bytes(&json!({
            "schemaVersion": 1,
            "source": "typescript-v1",
            "entries": entries
        }))
        .map(Some)
    }
}

fn scan_source(
    source_root: &Path,
) -> Result<(Vec<SourceFile>, Vec<MigrationExclusion>, String), MigrationError> {
    let mut files = Vec::new();
    let mut exclusions = Vec::new();
    walk_source(source_root, source_root, &mut files, &mut exclusions)?;
    files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    if files.len() > MAX_FILES
        || files
            .iter()
            .map(|file| file.bytes.len() as u64)
            .sum::<u64>()
            > MAX_TOTAL_BYTES
    {
        return Err(MigrationError::Bounds);
    }
    let mut digest = Sha256::new();
    for file in &files {
        digest.update(file.relative_path.as_bytes());
        digest.update([0]);
        digest.update(file.sha256.as_bytes());
        digest.update(*b"\n");
    }
    Ok((files, exclusions, hex_digest(digest.finalize().as_slice())))
}

fn walk_source(
    source_root: &Path,
    directory: &Path,
    files: &mut Vec<SourceFile>,
    exclusions: &mut Vec<MigrationExclusion>,
) -> Result<(), MigrationError> {
    let mut entries = std::fs::read_dir(directory)
        .map_err(|_| MigrationError::Read)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| MigrationError::Read)?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let relative = relative_string(source_root, &path)?;
        let metadata = std::fs::symlink_metadata(&path).map_err(|_| MigrationError::Read)?;
        if metadata.file_type().is_symlink() {
            return Err(MigrationError::Symlink);
        }
        if metadata.is_dir() {
            walk_source(source_root, &path, files, exclusions)?;
            continue;
        }
        if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
            return Err(MigrationError::Bounds);
        }
        let bytes = std::fs::read(&path).map_err(|_| MigrationError::Read)?;
        let kind = classify_source(&relative);
        if kind == SourceKind::Excluded {
            exclusions.push(MigrationExclusion {
                relative_path: relative.clone(),
                reason: exclusion_reason(&relative).to_owned(),
            });
        }
        files.push(SourceFile {
            relative_path: relative,
            sha256: sha256(&bytes),
            bytes,
            kind,
        });
    }
    Ok(())
}

fn classify_source(relative: &str) -> SourceKind {
    let lower = relative.to_ascii_lowercase();
    if lower == "config.json" {
        SourceKind::Config
    } else if lower == "indexes/threads.json" {
        SourceKind::Threads
    } else if lower.starts_with("sessions/") && lower.ends_with(".jsonl") {
        SourceKind::Session
    } else if lower.starts_with("turns/") && lower.ends_with(".jsonl") {
        SourceKind::Turn
    } else if (lower == "capability-snapshot.json" || lower.starts_with("capabilities/"))
        && lower.ends_with(".json")
    {
        SourceKind::Capability
    } else {
        SourceKind::Excluded
    }
}

fn exclusion_reason(relative: &str) -> &'static str {
    let lower = relative.to_ascii_lowercase();
    if lower.contains("secret") || lower.contains("credential") || lower.contains("key") {
        "secret_path"
    } else if lower.starts_with("traces/") {
        "private_trace"
    } else if lower.starts_with("summaries/") {
        "derived_summary"
    } else if lower.starts_with("indexes/") || lower.contains("cache") || lower.starts_with("db/") {
        "derived_data"
    } else if lower.starts_with("locks/") || lower.ends_with(".lock") {
        "lock_state"
    } else {
        "unsupported_path"
    }
}

fn source_fingerprint(source_root: &Path) -> Result<String, MigrationError> {
    let source_root = canonical_directory(source_root, MigrationError::Source)?;
    scan_source(&source_root).map(|(_, _, fingerprint)| fingerprint)
}

fn find_collisions(
    target_root: &Path,
    targets: &[MigrationTarget],
) -> Result<Vec<MigrationCollision>, MigrationError> {
    let mut collisions = Vec::new();
    for target in targets {
        let path = safe_join(target_root, &target.relative_path)?;
        if path.exists() {
            let metadata = std::fs::symlink_metadata(&path).map_err(|_| MigrationError::Read)?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(MigrationError::Collision);
            }
            let actual = hash_file(&path)?;
            if actual != target.sha256 {
                collisions.push(MigrationCollision {
                    relative_path: target.relative_path.clone(),
                    expected_sha256: target.sha256.clone(),
                    actual_sha256: actual,
                });
            }
        }
    }
    Ok(collisions)
}

fn validate_unique_targets(targets: &[MigrationTarget]) -> Result<(), MigrationError> {
    let mut seen = BTreeSet::new();
    for target in targets {
        validate_relative_path(&target.relative_path)?;
        if !seen.insert(target.relative_path.as_str()) {
            return Err(MigrationError::Plan);
        }
    }
    Ok(())
}

fn validate_receipt_ownership(receipt: &MigrationReceipt) -> Result<(), MigrationError> {
    if !valid_migration_id(&receipt.migration_id)
        || !is_sha256(&receipt.plan_hash)
        || !is_sha256(&receipt.source_fingerprint)
        || !is_sha256(&receipt.receipt_hash)
    {
        return Err(MigrationError::Receipt);
    }
    let allowed = BTreeSet::from([
        ".minimax/config.json".to_owned(),
        ".minimax/runtime/v1/sessions.jsonl".to_owned(),
        format!(
            ".minimax/migrations/v1/{}/capability-metadata.json",
            receipt.migration_id
        ),
    ]);
    let mut seen = BTreeSet::new();
    for target in receipt.created.iter().chain(&receipt.reused) {
        if validate_relative_path(&target.relative_path).is_err()
            || !allowed.contains(&target.relative_path)
            || !seen.insert(target.relative_path.as_str())
            || !is_sha256(&target.sha256)
        {
            return Err(MigrationError::Receipt);
        }
    }
    Ok(())
}

fn verify_receipt_targets(receipt: &MigrationReceipt) -> Result<(), MigrationError> {
    receipt.validate()?;
    let root = canonical_directory(Path::new(&receipt.target_root), MigrationError::Target)?;
    verify_targets(&root, receipt.created.iter().chain(&receipt.reused))
}

fn verify_targets<'a>(
    root: &Path,
    targets: impl IntoIterator<Item = &'a MigrationReceiptTarget>,
) -> Result<(), MigrationError> {
    for target in targets {
        let path = safe_join(root, &target.relative_path)?;
        let metadata =
            std::fs::symlink_metadata(&path).map_err(|_| MigrationError::TargetChanged)?;
        if metadata.file_type().is_symlink()
            || !metadata.is_file()
            || metadata.len() != target.bytes
            || hash_file(&path)? != target.sha256
        {
            return Err(MigrationError::TargetChanged);
        }
    }
    Ok(())
}

fn receipt_target(target: &MigrationTarget) -> MigrationReceiptTarget {
    MigrationReceiptTarget {
        relative_path: target.relative_path.clone(),
        sha256: target.sha256.clone(),
        bytes: target.bytes,
    }
}

fn rollback_created(
    target_root: &Path,
    targets: &[MigrationReceiptTarget],
) -> Result<(), MigrationError> {
    for target in targets.iter().rev() {
        let path = safe_join(target_root, &target.relative_path)?;
        if !path.exists() {
            continue;
        }
        let metadata = std::fs::symlink_metadata(&path).map_err(|_| MigrationError::Read)?;
        if metadata.file_type().is_symlink()
            || !metadata.is_file()
            || metadata.len() != target.bytes
            || hash_file(&path)? != target.sha256
        {
            return Err(MigrationError::TargetChanged);
        }
        std::fs::remove_file(path).map_err(|_| MigrationError::Write)?;
    }
    Ok(())
}

fn recover_interrupted(
    target_root: &Path,
    migration_id: &str,
    expected_plan_hash: &str,
    expected_targets: &[MigrationTarget],
) -> Result<(), MigrationError> {
    let directory = operation_directory(target_root, migration_id)?;
    let manifest_path = directory.join("operation.json");
    if !manifest_path.is_file() {
        if directory.exists() {
            remove_operation_files(&directory)?;
        }
        return Ok(());
    }
    let manifest: OperationManifest =
        read_json_bounded(&manifest_path, MAX_PLAN_BYTES, MigrationError::Recovery)?;
    if manifest.schema_version != SCHEMA_VERSION || manifest.plan_hash != expected_plan_hash {
        return Err(MigrationError::Recovery);
    }
    let expected = expected_targets
        .iter()
        .map(|target| (target.relative_path.as_str(), target))
        .collect::<BTreeMap<_, _>>();
    let mut seen = BTreeSet::new();
    for candidate in &manifest.candidates {
        let Some(target) = expected.get(candidate.relative_path.as_str()) else {
            return Err(MigrationError::Recovery);
        };
        if !seen.insert(candidate.relative_path.as_str())
            || candidate.sha256 != target.sha256
            || candidate.bytes != target.bytes
        {
            return Err(MigrationError::Recovery);
        }
    }
    rollback_created(target_root, &manifest.candidates)?;
    remove_operation_files(&directory)
}

fn remove_operation_files(directory: &Path) -> Result<(), MigrationError> {
    if directory.exists() {
        std::fs::remove_dir_all(directory).map_err(|_| MigrationError::Write)?;
    }
    Ok(())
}

fn receipt_path(target_root: &Path, migration_id: &str) -> Result<PathBuf, MigrationError> {
    safe_join(
        target_root,
        &format!(".minimax/migrations/v1/{migration_id}/receipt.json"),
    )
}

fn rollback_receipt_path(
    target_root: &Path,
    migration_id: &str,
) -> Result<PathBuf, MigrationError> {
    safe_join(
        target_root,
        &format!(".minimax/migrations/v1/{migration_id}/rollback.json"),
    )
}

fn operation_directory(target_root: &Path, migration_id: &str) -> Result<PathBuf, MigrationError> {
    safe_join(
        target_root,
        &format!(".minimax/migrations/v1/{migration_id}/operation"),
    )
}

fn read_receipt(path: &Path) -> Result<MigrationReceipt, MigrationError> {
    let receipt: MigrationReceipt =
        read_json_bounded(path, MAX_PLAN_BYTES, MigrationError::Receipt)?;
    receipt.validate()?;
    Ok(receipt)
}

struct MigrationLock {
    file: File,
}

impl MigrationLock {
    fn acquire(target_root: &Path) -> Result<Self, MigrationError> {
        let path = safe_join(target_root, ".minimax/migrations/v1/writer.lock")?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|_| MigrationError::Write)?;
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(|_| MigrationError::Write)?;
        file.try_lock().map_err(|_| MigrationError::Busy)?;
        Ok(Self { file })
    }
}

impl Drop for MigrationLock {
    fn drop(&mut self) {
        let _ = File::unlock(&self.file);
    }
}

fn canonical_directory(path: &Path, error: MigrationError) -> Result<PathBuf, MigrationError> {
    let canonical = path.canonicalize().map_err(|_| error.clone())?;
    if !canonical.is_dir() {
        return Err(error);
    }
    Ok(canonical)
}

fn safe_join(root: &Path, relative: &str) -> Result<PathBuf, MigrationError> {
    validate_relative_path(relative)?;
    let joined = relative
        .split('/')
        .fold(root.to_path_buf(), |path, segment| path.join(segment));
    if !joined.starts_with(root) {
        return Err(MigrationError::Target);
    }
    Ok(joined)
}

fn validate_relative_path(relative: &str) -> Result<(), MigrationError> {
    let path = Path::new(relative);
    if relative.is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(MigrationError::Target);
    }
    Ok(())
}

fn relative_string(root: &Path, path: &Path) -> Result<String, MigrationError> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| MigrationError::Source)?;
    let segments = relative
        .components()
        .map(|component| match component {
            Component::Normal(segment) => segment.to_str().ok_or(MigrationError::Source),
            _ => Err(MigrationError::Source),
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(segments.join("/"))
}

fn path_string(path: &Path) -> String {
    let normalized = path.to_string_lossy().replace('\\', "/");
    normalized
        .strip_prefix("//?/")
        .unwrap_or(&normalized)
        .to_owned()
}

fn write_new_file(path: &Path, bytes: &[u8]) -> Result<(), MigrationError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|_| MigrationError::Write)?;
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                MigrationError::Collision
            } else {
                MigrationError::Write
            }
        })?;
    file.write_all(bytes)
        .and_then(|()| file.flush())
        .and_then(|()| file.sync_all())
        .map_err(|_| MigrationError::Write)
}

fn write_json_new<T: Serialize>(path: &Path, value: &T) -> Result<(), MigrationError> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|_| MigrationError::Serialization)?;
    bytes.push(b'\n');
    write_new_file(path, &bytes)
}

fn read_json_bounded<T: for<'de> Deserialize<'de>>(
    path: &Path,
    max_bytes: u64,
    error: MigrationError,
) -> Result<T, MigrationError> {
    let metadata = std::fs::symlink_metadata(path).map_err(|_| error.clone())?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > max_bytes {
        return Err(error);
    }
    let bytes = std::fs::read(path).map_err(|_| MigrationError::Read)?;
    serde_json::from_slice(&bytes).map_err(|_| error)
}

fn hash_file(path: &Path) -> Result<String, MigrationError> {
    let mut file = File::open(path).map_err(|_| MigrationError::Read)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|_| MigrationError::Read)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(hex_digest(digest.finalize().as_slice()))
}

fn hash_serializable<T: Serialize>(value: &T) -> Result<String, MigrationError> {
    serde_json::to_vec(value)
        .map(|bytes| sha256(&bytes))
        .map_err(|_| MigrationError::Serialization)
}

fn sha256(bytes: &[u8]) -> String {
    hex_digest(Sha256::digest(bytes).as_slice())
}

fn hex_digest(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn pretty_json_bytes(value: &Value) -> Result<Vec<u8>, MigrationError> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|_| MigrationError::Serialization)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn parse_jsonl(bytes: &[u8]) -> Result<Vec<Value>, MigrationError> {
    let raw = std::str::from_utf8(bytes).map_err(|_| MigrationError::Malformed)?;
    raw.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).map_err(|_| MigrationError::Malformed))
        .collect()
}

fn envelope_payload(value: Value, expected_kind: &str) -> Result<Value, MigrationError> {
    if value.get("schemaVersion").is_some() {
        if value.get("schemaVersion").and_then(Value::as_u64) != Some(1)
            || value.get("kind").and_then(Value::as_str) != Some(expected_kind)
        {
            return Err(MigrationError::Malformed);
        }
        value
            .get("payload")
            .cloned()
            .ok_or(MigrationError::Malformed)
    } else {
        Ok(value)
    }
}

fn envelope_or_legacy_turn(value: Value) -> Result<(&'static str, Value), MigrationError> {
    if value.get("schemaVersion").is_some() {
        if value.get("schemaVersion").and_then(Value::as_u64) != Some(1) {
            return Err(MigrationError::Malformed);
        }
        let kind = value
            .get("kind")
            .and_then(Value::as_str)
            .ok_or(MigrationError::Malformed)?;
        let payload = value
            .get("payload")
            .cloned()
            .ok_or(MigrationError::Malformed)?;
        match kind {
            "turn.snapshot" => Ok(("turn.snapshot", payload)),
            "assistant.delta" => Ok(("assistant.delta", payload)),
            _ => Err(MigrationError::Malformed),
        }
    } else if value.get("kind").and_then(Value::as_str) == Some("turn.snapshot") {
        Ok((
            "turn.snapshot",
            value
                .get("turn")
                .cloned()
                .ok_or(MigrationError::Malformed)?,
        ))
    } else if value.get("kind").and_then(Value::as_str) == Some("assistant.delta") {
        Ok(("assistant.delta", value))
    } else {
        Err(MigrationError::Malformed)
    }
}

fn required_string<'a>(value: &'a Value, key: &str) -> Result<&'a str, MigrationError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or(MigrationError::Malformed)
}

fn contains_secret(value: &Value) -> bool {
    match value {
        Value::Object(object) => object
            .iter()
            .any(|(key, value)| secret_key(key) || contains_secret(value)),
        Value::Array(values) => values.iter().any(contains_secret),
        Value::String(value) => secret_string(value),
        _ => false,
    }
}

fn contains_private_reasoning(value: &Value) -> bool {
    match value {
        Value::Object(object) => object.iter().any(|(key, value)| {
            let normalized = key
                .bytes()
                .filter(u8::is_ascii_alphanumeric)
                .map(|byte| byte.to_ascii_lowercase())
                .collect::<Vec<_>>();
            let normalized = String::from_utf8_lossy(&normalized);
            matches!(normalized.as_ref(), "privatereasoning" | "reasoningcontent")
                || contains_private_reasoning(value)
        }),
        Value::Array(values) => values.iter().any(contains_private_reasoning),
        _ => false,
    }
}

fn secret_key(key: &str) -> bool {
    let normalized = key
        .bytes()
        .filter(u8::is_ascii_alphanumeric)
        .map(|byte| byte.to_ascii_lowercase())
        .collect::<Vec<_>>();
    let normalized = String::from_utf8_lossy(&normalized);
    [
        "apikey",
        "secret",
        "password",
        "authorization",
        "cookie",
        "credential",
        "privatereasoning",
        "reasoningcontent",
    ]
    .iter()
    .any(|denied| normalized.contains(denied))
}

fn secret_string(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("private_reasoning")
        || lower.contains("authorization: bearer")
        || lower.contains("bearer ey")
        || lower.contains("api_key=")
        || lower.contains("apikey=")
        || lower.contains("minimax_api_key")
        || looks_like_secret_token(value)
}

fn looks_like_secret_token(value: &str) -> bool {
    value
        .split(|character: char| character.is_whitespace() || matches!(character, '"' | '\'' | ','))
        .any(|part| {
            (part.starts_with("sk-") || part.starts_with("sk_"))
                && part.len() >= 16
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        })
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_migration_id(value: &str) -> bool {
    value.len() == 19
        && value.starts_with("ts-")
        && value[3..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn imported_id(kind: &str, original: &str) -> String {
    let mut value = original
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | ':') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    value.truncate(220);
    format!("ts-{kind}:{value}")
}

fn safe_tool_name(capability: &str) -> String {
    let mut value = capability
        .trim_start_matches("capability:")
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    value.truncate(120);
    value
        .trim_matches('_')
        .to_owned()
        .chars()
        .fold(String::new(), |mut output, character| {
            if character != '_' || !output.ends_with('_') {
                output.push(character);
            }
            output
        })
}

fn timestamp_ms(value: &str) -> Result<u64, MigrationError> {
    if value.len() < 20 || !value.ends_with('Z') {
        return Err(MigrationError::Malformed);
    }
    let year = parse_u64(value, 0, 4)? as i64;
    let month = parse_u64(value, 5, 7)? as i64;
    let day = parse_u64(value, 8, 10)? as i64;
    let hour = parse_u64(value, 11, 13)?;
    let minute = parse_u64(value, 14, 16)?;
    let second = parse_u64(value, 17, 19)?;
    if value.as_bytes().get(4) != Some(&b'-')
        || value.as_bytes().get(7) != Some(&b'-')
        || value.as_bytes().get(10) != Some(&b'T')
        || value.as_bytes().get(13) != Some(&b':')
        || value.as_bytes().get(16) != Some(&b':')
        || !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return Err(MigrationError::Malformed);
    }
    let millis = if value.as_bytes().get(19) == Some(&b'.') {
        let fraction = &value[20..value.len() - 1];
        if fraction.is_empty()
            || fraction.len() > 9
            || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(MigrationError::Malformed);
        }
        let first_three = &fraction[..fraction.len().min(3)];
        let parsed = first_three
            .parse::<u64>()
            .map_err(|_| MigrationError::Malformed)?;
        parsed
            * 10_u64
                .pow(u32::try_from(3 - first_three.len()).map_err(|_| MigrationError::Malformed)?)
    } else if value.len() == 20 {
        0
    } else {
        return Err(MigrationError::Malformed);
    };
    let days = days_from_civil(year, month, day).ok_or(MigrationError::Malformed)?;
    let seconds = days
        .checked_mul(86_400)
        .and_then(|value| {
            value.checked_add(i64::try_from(hour * 3600 + minute * 60 + second).ok()?)
        })
        .ok_or(MigrationError::Malformed)?;
    if seconds < 0 {
        return Err(MigrationError::Malformed);
    }
    u64::try_from(seconds)
        .ok()
        .and_then(|seconds| seconds.checked_mul(1000))
        .and_then(|seconds| seconds.checked_add(millis))
        .ok_or(MigrationError::Malformed)
}

fn parse_u64(value: &str, start: usize, end: usize) -> Result<u64, MigrationError> {
    value
        .get(start..end)
        .ok_or(MigrationError::Malformed)?
        .parse()
        .map_err(|_| MigrationError::Malformed)
}

fn days_from_civil(year: i64, month: i64, day: i64) -> Option<i64> {
    let adjusted_year = year - i64::from(month <= 2);
    let era = if adjusted_year >= 0 {
        adjusted_year
    } else {
        adjusted_year - 399
    } / 400;
    let year_of_era = adjusted_year - era * 400;
    let adjusted_month = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * adjusted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era.checked_mul(146_097)?
        .checked_add(day_of_era)?
        .checked_sub(719_468)
}

fn target_kind(relative: &str) -> &'static str {
    if relative.ends_with("config.json") {
        "rust_config"
    } else if relative.ends_with("sessions.jsonl") {
        "rust_session_journal"
    } else {
        "capability_metadata"
    }
}
