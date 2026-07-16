use std::collections::BTreeSet;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use minimax_core::SessionMachine;
use minimax_protocol::{
    ContentHash, EvidenceId, ModelBinding, SchemaVersion, SessionId, SessionRecordV1,
    parse_session_record_v1,
};
use serde::{Deserialize, Serialize};

use crate::bootstrap::{ProjectVault, VaultError};
use crate::path::{atomic_create_or_same, content_hash, sha256_hex};
use crate::runtime::RUNTIME_DIRECTORY;
use crate::runtime::lease::WorkspaceLease;
use crate::runtime::record_session_id;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FinalizedSessionEvidence {
    pub schema_version: SchemaVersion,
    pub evidence_id: EvidenceId,
    pub session_id: SessionId,
    pub binding: ModelBinding,
    pub created_at_unix_ms: u64,
    pub updated_at_unix_ms: u64,
    pub finalized_at_unix_ms: u64,
    pub turn_count: u32,
    pub event_count: u32,
    pub events_hash: ContentHash,
}

pub fn read_finalized_session(
    vault: &ProjectVault,
    evidence: &FinalizedSessionEvidence,
) -> Result<minimax_protocol::SessionRecord, VaultError> {
    let path = vault
        .root()
        .join("raw/sessions")
        .join(safe_session_directory(&evidence.session_id))
        .join("events.jsonl");
    let bytes = std::fs::read(path).map_err(|_| VaultError::Io)?;
    if content_hash(&bytes) != evidence.events_hash {
        return Err(VaultError::Conflict);
    }
    let records = parse_records(&bytes)?;
    if records
        .iter()
        .any(|record| record_session_id(record) != Some(&evidence.session_id))
    {
        return Err(VaultError::RecoveryRequired);
    }
    let machine = SessionMachine::replay(records).map_err(|_| VaultError::RecoveryRequired)?;
    let session = machine
        .sessions()
        .get(&evidence.session_id)
        .cloned()
        .ok_or(VaultError::SessionNotFound)?;
    if session.binding != evidence.binding
        || session.created_at_unix_ms != evidence.created_at_unix_ms
        || session.updated_at_unix_ms != evidence.updated_at_unix_ms
        || session.turns.len() != usize::try_from(evidence.turn_count).unwrap_or(usize::MAX)
    {
        return Err(VaultError::Conflict);
    }
    Ok(session)
}

pub fn finalize_runtime_session(
    project_root: impl AsRef<Path>,
    vault: &ProjectVault,
    session_id: &SessionId,
    finalized_at_unix_ms: u64,
) -> Result<FinalizedSessionEvidence, VaultError> {
    let runtime_dir = project_root.as_ref().join(RUNTIME_DIRECTORY);
    let _runtime_lease = WorkspaceLease::acquire(&runtime_dir)?;
    let journal_path = runtime_dir.join("sessions.jsonl");
    let mut bytes = std::fs::read(&journal_path).map_err(|_| VaultError::Io)?;
    repair_final_fragment(vault.root(), &journal_path, &mut bytes)?;
    finalize_runtime_bytes(
        &runtime_dir,
        vault,
        session_id,
        finalized_at_unix_ms,
        &bytes,
    )
}

pub(crate) fn finalize_runtime_session_from_open_store(
    runtime_dir: &Path,
    journal_path: &Path,
    vault: &ProjectVault,
    session_id: &SessionId,
    finalized_at_unix_ms: u64,
) -> Result<FinalizedSessionEvidence, VaultError> {
    let bytes = std::fs::read(journal_path).map_err(|_| VaultError::Io)?;
    if !bytes.is_empty() && !bytes.ends_with(b"\n") {
        return Err(VaultError::RecoveryRequired);
    }
    finalize_runtime_bytes(runtime_dir, vault, session_id, finalized_at_unix_ms, &bytes)
}

fn finalize_runtime_bytes(
    runtime_dir: &Path,
    vault: &ProjectVault,
    session_id: &SessionId,
    finalized_at_unix_ms: u64,
    bytes: &[u8],
) -> Result<FinalizedSessionEvidence, VaultError> {
    let all_records = parse_records(bytes)?;
    let machine =
        SessionMachine::replay(all_records.clone()).map_err(|_| VaultError::RecoveryRequired)?;
    let session = machine
        .sessions()
        .get(session_id)
        .ok_or(VaultError::SessionNotFound)?;
    if session.turns.is_empty()
        || session.turns.iter().any(|turn| !turn.status.is_terminal())
        || session
            .turns
            .iter()
            .flat_map(|turn| &turn.tool_invocations)
            .any(|invocation| invocation.terminal_result.is_none())
    {
        return Err(VaultError::SessionNotTerminal);
    }

    let selected = all_records
        .into_iter()
        .filter(|record| record_session_id(record) == Some(session_id))
        .collect::<Vec<_>>();
    let mut event_bytes = Vec::new();
    for record in &selected {
        serde_json::to_writer(&mut event_bytes, record)
            .map_err(|_| VaultError::RecoveryRequired)?;
        event_bytes.push(b'\n');
    }
    reject_sensitive(&event_bytes)?;
    let events_hash = content_hash(&event_bytes);
    let evidence_id = EvidenceId::new(format!(
        "session:{}:{}",
        session_id.as_str(),
        &events_hash.as_str()[..16]
    ))
    .map_err(|_| VaultError::RecoveryRequired)?;
    let evidence = FinalizedSessionEvidence {
        schema_version: SchemaVersion,
        evidence_id,
        session_id: session_id.clone(),
        binding: session.binding.clone(),
        created_at_unix_ms: session.created_at_unix_ms,
        updated_at_unix_ms: session.updated_at_unix_ms,
        finalized_at_unix_ms,
        turn_count: u32::try_from(session.turns.len()).map_err(|_| VaultError::RecoveryRequired)?,
        event_count: u32::try_from(selected.len()).map_err(|_| VaultError::RecoveryRequired)?,
        events_hash,
    };
    let mut metadata_bytes = serde_json::to_vec_pretty(&evidence).map_err(|_| VaultError::Io)?;
    metadata_bytes.push(b'\n');
    let raw_dir = vault
        .root()
        .join("raw/sessions")
        .join(safe_session_directory(session_id));
    std::fs::create_dir_all(&raw_dir).map_err(|_| VaultError::Io)?;
    atomic_create_or_same(&raw_dir.join("events.jsonl"), &event_bytes).map_err(
        |error| match error {
            VaultError::Conflict => VaultError::Finalized,
            other => other,
        },
    )?;
    atomic_create_or_same(&raw_dir.join("session.json"), &metadata_bytes).map_err(|error| {
        match error {
            VaultError::Conflict => VaultError::Finalized,
            other => other,
        }
    })?;

    let marker_path = finalization_marker_path(runtime_dir, session_id);
    if let Some(parent) = marker_path.parent() {
        std::fs::create_dir_all(parent).map_err(|_| VaultError::Io)?;
    }
    atomic_create_or_same(&marker_path, &metadata_bytes).map_err(|error| match error {
        VaultError::Conflict => VaultError::Finalized,
        other => other,
    })?;
    Ok(evidence)
}

pub(crate) fn finalization_marker_path(runtime_dir: &Path, session_id: &SessionId) -> PathBuf {
    runtime_dir.join("finalized").join(format!(
        "{}.json",
        sha256_hex(session_id.as_str().as_bytes())
    ))
}

fn parse_records(bytes: &[u8]) -> Result<Vec<SessionRecordV1>, VaultError> {
    if bytes.is_empty() {
        return Err(VaultError::SessionNotFound);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| VaultError::RecoveryRequired)?;
    let mut seen = BTreeSet::new();
    let mut records = Vec::new();
    for line in text.lines() {
        if line.is_empty() || line.len() > 1024 * 1024 {
            return Err(VaultError::RecoveryRequired);
        }
        let record = parse_session_record_v1(line).map_err(|_| VaultError::RecoveryRequired)?;
        if !seen.insert(record.record_id.clone()) {
            return Err(VaultError::RecoveryRequired);
        }
        records.push(record);
    }
    Ok(records)
}

fn repair_final_fragment(
    vault_root: &Path,
    journal_path: &Path,
    bytes: &mut Vec<u8>,
) -> Result<(), VaultError> {
    if bytes.is_empty() || bytes.ends_with(b"\n") {
        return Ok(());
    }
    let complete_len = bytes
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |position| position + 1);
    let fragment = &bytes[complete_len..];
    let repair_name = format!("final-fragment-{}.partial", content_hash(fragment).as_str());
    let repair_path = vault_root.join(".minimax/recovery").join(repair_name);
    atomic_create_or_same(&repair_path, fragment)?;
    let mut journal = OpenOptions::new()
        .write(true)
        .open(journal_path)
        .map_err(|_| VaultError::Io)?;
    journal
        .set_len(u64::try_from(complete_len).map_err(|_| VaultError::RecoveryRequired)?)
        .and_then(|()| journal.flush())
        .and_then(|()| journal.sync_all())
        .map_err(|_| VaultError::Io)?;
    bytes.truncate(complete_len);
    Ok(())
}

fn safe_session_directory(session_id: &SessionId) -> String {
    sha256_hex(session_id.as_str().as_bytes())
}

fn reject_sensitive(bytes: &[u8]) -> Result<(), VaultError> {
    let lower = std::str::from_utf8(bytes)
        .map_err(|_| VaultError::RecoveryRequired)?
        .to_ascii_lowercase();
    let fixed_markers = [
        "-----begin private key-----",
        "-----begin rsa private key-----",
        "-----begin openssh private key-----",
        "github_pat_",
        "api_key=",
        "api-key=",
        "client_secret=",
        "access_token=",
        "password=",
        "authorization: bearer ",
        "\"private_reasoning\"",
        "\"chain_of_thought\"",
        "<thinking>",
    ];
    if fixed_markers.iter().any(|marker| lower.contains(marker))
        || lower.lines().any(|line| {
            let trimmed = line.trim();
            [
                "api_key=",
                "api-key=",
                "client_secret=",
                "access_token=",
                "password=",
                "authorization: bearer ",
            ]
            .iter()
            .any(|prefix| {
                trimmed.strip_prefix(prefix).is_some_and(|value| {
                    let value = value.trim_matches(['\"', '\'', ' ']);
                    value.len() >= 12
                        && !matches!(
                            value,
                            "placeholder" | "example-value" | "your-api-key" | "<redacted>"
                        )
                })
            })
        })
    {
        return Err(VaultError::SensitiveContent);
    }
    Ok(())
}
