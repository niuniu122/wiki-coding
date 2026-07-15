use std::fmt::Write as _;
use std::fs::File;
use std::io::{Read as _, Write as _};
use std::path::Path;

use minimax_core::CancellationPort;
use minimax_protocol::{SchemaVersion, ToolInvocation, ToolResult, ToolTerminalStatus};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use tempfile::Builder;

use crate::error::{ToolDenial, ToolDenialCode, io_denial};
use crate::policy::Preflight;
use crate::{ResolvedToolPath, WorkspaceRoot};

const MAX_FILE_BYTES: usize = 64 * 1_024;

#[derive(Clone, Copy, Debug, Default)]
pub struct ApplyPatchTool;

impl ApplyPatchTool {
    pub fn execute(
        workspace: &WorkspaceRoot,
        invocation: &ToolInvocation,
        cancellation: &dyn CancellationPort,
    ) -> ToolResult {
        match Self::try_execute(workspace, invocation, cancellation) {
            Ok(output) => success(invocation, output),
            Err(error) => error.into_result(invocation),
        }
    }

    fn try_execute(
        workspace: &WorkspaceRoot,
        invocation: &ToolInvocation,
        cancellation: &dyn CancellationPort,
    ) -> Result<String, ToolDenial> {
        Preflight::check(invocation, cancellation)?;
        let arguments: ApplyPatchArguments = parse_arguments(invocation)?;
        validate_hash(&arguments.expected_sha256)?;
        if arguments.edits.is_empty() {
            return Err(ToolDenial::rejected(ToolDenialCode::InvalidArguments));
        }
        let target = workspace.resolve_existing(&arguments.path)?;
        let (original, permissions) = read_editable_file(&target)?;
        if sha256_hex(original.as_bytes()) != arguments.expected_sha256 {
            return Err(ToolDenial::failed(ToolDenialCode::HashConflict));
        }
        let mut updated = original;
        for edit in &arguments.edits {
            if cancellation.is_cancelled() {
                return Err(ToolDenial::cancelled());
            }
            updated = apply_exact_edit(updated, edit)?;
            if updated.len() > MAX_FILE_BYTES {
                return Err(ToolDenial::rejected(ToolDenialCode::InputLimit));
            }
        }
        Preflight::ensure_safe_output(&updated)?;
        atomic_persist(
            workspace,
            &target,
            updated.as_bytes(),
            PersistMode::Replace,
            Some(permissions),
            Some(&arguments.expected_sha256),
            cancellation,
        )?;
        serialize_receipt(&target, updated.as_bytes())
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct WriteFileTool;

impl WriteFileTool {
    pub fn execute(
        workspace: &WorkspaceRoot,
        invocation: &ToolInvocation,
        cancellation: &dyn CancellationPort,
    ) -> ToolResult {
        match Self::try_execute(workspace, invocation, cancellation) {
            Ok(output) => success(invocation, output),
            Err(error) => error.into_result(invocation),
        }
    }

    fn try_execute(
        workspace: &WorkspaceRoot,
        invocation: &ToolInvocation,
        cancellation: &dyn CancellationPort,
    ) -> Result<String, ToolDenial> {
        Preflight::check(invocation, cancellation)?;
        let arguments: WriteFileArguments = parse_arguments(invocation)?;
        if arguments.content.len() > MAX_FILE_BYTES {
            return Err(ToolDenial::rejected(ToolDenialCode::InputLimit));
        }
        if arguments.content.contains('\0') {
            return Err(ToolDenial::rejected(ToolDenialCode::BinaryFile));
        }
        Preflight::ensure_safe_output(&arguments.content)?;
        let target = workspace.resolve_write(&arguments.path)?;
        let (mode, permissions, expected_hash) = match arguments.mode {
            WriteMode::Create => {
                if arguments.expected_sha256.is_some() {
                    return Err(ToolDenial::rejected(ToolDenialCode::InvalidArguments));
                }
                if target.absolute().exists() {
                    return Err(ToolDenial::failed(ToolDenialCode::AlreadyExists));
                }
                (PersistMode::Create, None, None)
            }
            WriteMode::Replace => {
                let expected = arguments
                    .expected_sha256
                    .as_deref()
                    .ok_or_else(|| ToolDenial::rejected(ToolDenialCode::InvalidArguments))?;
                validate_hash(expected)?;
                let existing = workspace.resolve_existing(&arguments.path)?;
                let (original, permissions) = read_editable_file(&existing)?;
                if sha256_hex(original.as_bytes()) != expected {
                    return Err(ToolDenial::failed(ToolDenialCode::HashConflict));
                }
                (PersistMode::Replace, Some(permissions), Some(expected))
            }
        };
        atomic_persist(
            workspace,
            &target,
            arguments.content.as_bytes(),
            mode,
            permissions,
            expected_hash,
            cancellation,
        )?;
        serialize_receipt(&target, arguments.content.as_bytes())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ApplyPatchArguments {
    path: String,
    expected_sha256: String,
    edits: Vec<ExactEdit>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExactEdit {
    old_text: String,
    new_text: String,
    expected_occurrences: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WriteFileArguments {
    path: String,
    mode: WriteMode,
    content: String,
    #[serde(default)]
    expected_sha256: Option<String>,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum WriteMode {
    Create,
    Replace,
}

#[derive(Clone, Copy)]
enum PersistMode {
    Create,
    Replace,
}

#[derive(Serialize)]
struct WriteReceipt {
    path: String,
    bytes: usize,
    sha256: String,
}

fn read_editable_file(
    target: &ResolvedToolPath,
) -> Result<(String, std::fs::Permissions), ToolDenial> {
    let mut file = File::open(target.absolute()).map_err(|error| io_denial(&error))?;
    let metadata = file.metadata().map_err(|error| io_denial(&error))?;
    if !metadata.is_file() {
        return Err(ToolDenial::rejected(ToolDenialCode::WrongFileType));
    }
    if metadata.len() > MAX_FILE_BYTES as u64 {
        return Err(ToolDenial::rejected(ToolDenialCode::InputLimit));
    }
    let mut bytes = Vec::new();
    std::io::Read::by_ref(&mut file)
        .take((MAX_FILE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| io_denial(&error))?;
    if bytes.len() > MAX_FILE_BYTES {
        return Err(ToolDenial::rejected(ToolDenialCode::InputLimit));
    }
    let text =
        String::from_utf8(bytes).map_err(|_| ToolDenial::rejected(ToolDenialCode::BinaryFile))?;
    if text.contains('\0') {
        return Err(ToolDenial::rejected(ToolDenialCode::BinaryFile));
    }
    Preflight::ensure_safe_output(&text)?;
    Ok((text, metadata.permissions()))
}

fn apply_exact_edit(mut text: String, edit: &ExactEdit) -> Result<String, ToolDenial> {
    if edit.old_text.is_empty() || edit.expected_occurrences == 0 {
        return Err(ToolDenial::rejected(ToolDenialCode::InvalidArguments));
    }
    let positions: Vec<usize> = text
        .char_indices()
        .filter_map(|(index, _)| text[index..].starts_with(&edit.old_text).then_some(index))
        .collect();
    if positions
        .windows(2)
        .any(|pair| pair[1] < pair[0] + edit.old_text.len())
    {
        return Err(ToolDenial::failed(ToolDenialCode::OverlappingMatches));
    }
    if positions.len() != edit.expected_occurrences {
        return Err(ToolDenial::failed(ToolDenialCode::OccurrenceConflict));
    }
    for position in positions.into_iter().rev() {
        text.replace_range(position..position + edit.old_text.len(), &edit.new_text);
    }
    Ok(text)
}

fn atomic_persist(
    workspace: &WorkspaceRoot,
    target: &ResolvedToolPath,
    bytes: &[u8],
    mode: PersistMode,
    permissions: Option<std::fs::Permissions>,
    expected_sha256: Option<&str>,
    cancellation: &dyn CancellationPort,
) -> Result<(), ToolDenial> {
    if cancellation.is_cancelled() {
        return Err(ToolDenial::cancelled());
    }
    let parent = target
        .absolute()
        .parent()
        .ok_or_else(|| ToolDenial::rejected(ToolDenialCode::InvalidPath))?;
    let canonical_parent = std::fs::canonicalize(parent).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            ToolDenial::rejected(ToolDenialCode::PathNotFound)
        } else {
            io_denial(&error)
        }
    })?;
    workspace.ensure_contained(&canonical_parent)?;
    let mut temp = Builder::new()
        .prefix(".minimax-write-")
        .tempfile_in(&canonical_parent)
        .map_err(|error| io_denial(&error))?;
    temp.write_all(bytes).map_err(|error| io_denial(&error))?;
    temp.flush().map_err(|error| io_denial(&error))?;
    if let Some(permissions) = permissions {
        temp.as_file()
            .set_permissions(permissions)
            .map_err(|error| io_denial(&error))?;
    }
    temp.as_file()
        .sync_all()
        .map_err(|error| io_denial(&error))?;
    if cancellation.is_cancelled() {
        return Err(ToolDenial::cancelled());
    }
    if let Some(expected_sha256) = expected_sha256 {
        revalidate_replace_target(workspace, target, expected_sha256)?;
    }
    match mode {
        PersistMode::Create => temp.persist_noclobber(target.absolute()),
        PersistMode::Replace => temp.persist(target.absolute()),
    }
    .map_err(|error| {
        if error.error.kind() == std::io::ErrorKind::AlreadyExists {
            ToolDenial::failed(ToolDenialCode::AlreadyExists)
        } else {
            io_denial(&error.error)
        }
    })?;
    sync_directory(&canonical_parent)?;
    Ok(())
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), ToolDenial> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| io_denial(&error))
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<(), ToolDenial> {
    Ok(())
}

fn revalidate_replace_target(
    workspace: &WorkspaceRoot,
    target: &ResolvedToolPath,
    expected_sha256: &str,
) -> Result<(), ToolDenial> {
    let canonical = std::fs::canonicalize(target.absolute()).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            ToolDenial::failed(ToolDenialCode::HashConflict)
        } else {
            io_denial(&error)
        }
    })?;
    workspace.ensure_contained(&canonical)?;
    let metadata = std::fs::metadata(&canonical).map_err(|error| io_denial(&error))?;
    if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES as u64 {
        return Err(ToolDenial::failed(ToolDenialCode::HashConflict));
    }
    let bytes = std::fs::read(&canonical).map_err(|error| io_denial(&error))?;
    if sha256_hex(&bytes) == expected_sha256 {
        Ok(())
    } else {
        Err(ToolDenial::failed(ToolDenialCode::HashConflict))
    }
}

fn serialize_receipt(target: &ResolvedToolPath, bytes: &[u8]) -> Result<String, ToolDenial> {
    let receipt = WriteReceipt {
        path: target.relative().to_string_lossy().replace('\\', "/"),
        bytes: bytes.len(),
        sha256: sha256_hex(bytes),
    };
    let output = serde_json::to_string(&receipt)
        .map_err(|_| ToolDenial::failed(ToolDenialCode::IoFailed))?;
    Preflight::ensure_safe_output(&output)?;
    Ok(output)
}

fn success(invocation: &ToolInvocation, output: String) -> ToolResult {
    ToolResult {
        schema_version: SchemaVersion,
        call_id: invocation.call.call_id.clone(),
        tool_name: invocation.call.name.clone(),
        status: ToolTerminalStatus::Succeeded,
        code: "ok".to_owned(),
        output: Some(output),
    }
}

fn parse_arguments<T: for<'de> Deserialize<'de>>(
    invocation: &ToolInvocation,
) -> Result<T, ToolDenial> {
    serde_json::from_str(&invocation.call.arguments_json)
        .map_err(|_| ToolDenial::rejected(ToolDenialCode::InvalidArguments))
}

fn validate_hash(hash: &str) -> Result<(), ToolDenial> {
    if hash.len() == 64
        && hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(ToolDenial::rejected(ToolDenialCode::InvalidArguments))
    }
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(64);
    for byte in digest {
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}
