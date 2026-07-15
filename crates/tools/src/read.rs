use std::fs::File;
use std::io::Read as _;

use minimax_protocol::{
    MAX_TOOL_RESULT_BYTES, SchemaVersion, ToolInvocation, ToolResult, ToolTerminalStatus,
};
use serde::{Deserialize, Serialize};

use crate::WorkspaceRoot;
use crate::error::{ToolDenial, ToolDenialCode, io_denial};
use crate::policy::{CancellationSignal, Preflight, ensure_public_path};
use crate::write::sha256_hex;

const MAX_READ_BYTES: usize = 64 * 1_024;
const MAX_DIRECTORY_ENTRIES: usize = 500;

#[derive(Clone, Copy, Debug, Default)]
pub struct ReadFileTool;

impl ReadFileTool {
    pub fn execute(
        workspace: &WorkspaceRoot,
        invocation: &ToolInvocation,
        cancellation: &dyn CancellationSignal,
    ) -> ToolResult {
        match Self::try_execute(workspace, invocation, cancellation) {
            Ok(output) => success(invocation, output),
            Err(error) => error.into_result(invocation),
        }
    }

    fn try_execute(
        workspace: &WorkspaceRoot,
        invocation: &ToolInvocation,
        cancellation: &dyn CancellationSignal,
    ) -> Result<String, ToolDenial> {
        Preflight::check(invocation, cancellation)?;
        let arguments: PathArguments = parse_arguments(invocation)?;
        let target = workspace.resolve_existing(&arguments.path)?;
        if cancellation.is_cancelled() {
            return Err(ToolDenial::cancelled());
        }
        let mut file = File::open(target.absolute()).map_err(|error| io_denial(&error))?;
        let metadata = file.metadata().map_err(|error| io_denial(&error))?;
        if !metadata.is_file() {
            return Err(ToolDenial::rejected(ToolDenialCode::WrongFileType));
        }
        if metadata.len() > MAX_READ_BYTES as u64 {
            return Err(ToolDenial::rejected(ToolDenialCode::InputLimit));
        }
        let mut bytes = Vec::new();
        file.by_ref()
            .take((MAX_READ_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|error| io_denial(&error))?;
        if bytes.len() > MAX_READ_BYTES {
            return Err(ToolDenial::rejected(ToolDenialCode::InputLimit));
        }
        if cancellation.is_cancelled() {
            return Err(ToolDenial::cancelled());
        }
        let content = String::from_utf8(bytes)
            .map_err(|_| ToolDenial::rejected(ToolDenialCode::BinaryFile))?;
        if content.contains('\0') {
            return Err(ToolDenial::rejected(ToolDenialCode::BinaryFile));
        }
        Preflight::ensure_safe_output(&content)?;
        let receipt = ReadReceipt {
            path: path_for_output(target.relative()),
            bytes: content.len(),
            sha256: sha256_hex(content.as_bytes()),
            content,
        };
        let output = serde_json::to_string(&receipt)
            .map_err(|_| ToolDenial::failed(ToolDenialCode::IoFailed))?;
        Preflight::ensure_safe_output(&output)?;
        Ok(output)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ListDirectoryTool;

impl ListDirectoryTool {
    pub fn execute(
        workspace: &WorkspaceRoot,
        invocation: &ToolInvocation,
        cancellation: &dyn CancellationSignal,
    ) -> ToolResult {
        match Self::try_execute(workspace, invocation, cancellation) {
            Ok(output) => success(invocation, output),
            Err(error) => error.into_result(invocation),
        }
    }

    fn try_execute(
        workspace: &WorkspaceRoot,
        invocation: &ToolInvocation,
        cancellation: &dyn CancellationSignal,
    ) -> Result<String, ToolDenial> {
        Preflight::check(invocation, cancellation)?;
        let arguments: PathArguments = parse_arguments(invocation)?;
        let target = workspace.resolve_existing(&arguments.path)?;
        if !target.absolute().is_dir() {
            return Err(ToolDenial::rejected(ToolDenialCode::WrongFileType));
        }
        let reader = std::fs::read_dir(target.absolute()).map_err(|error| io_denial(&error))?;
        let mut entries = Vec::new();
        for entry in reader {
            if cancellation.is_cancelled() {
                return Err(ToolDenial::cancelled());
            }
            if entries.len() == MAX_DIRECTORY_ENTRIES {
                return Err(ToolDenial::rejected(ToolDenialCode::EntryLimit));
            }
            let entry = entry.map_err(|error| io_denial(&error))?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| ToolDenial::rejected(ToolDenialCode::BinaryFile))?;
            ensure_public_path(&name)?;
            let file_type = entry.file_type().map_err(|error| io_denial(&error))?;
            let kind = if file_type.is_file() {
                EntryKind::File
            } else if file_type.is_dir() {
                EntryKind::Directory
            } else if file_type.is_symlink() {
                EntryKind::Symlink
            } else {
                EntryKind::Other
            };
            entries.push(DirectoryEntry { name, kind });
        }
        entries.sort_by_key(|entry| normalized_name(&entry.name));
        let receipt = DirectoryReceipt {
            path: path_for_output(target.relative()),
            entries,
        };
        let output = serde_json::to_string(&receipt)
            .map_err(|_| ToolDenial::failed(ToolDenialCode::IoFailed))?;
        if output.len() > MAX_TOOL_RESULT_BYTES {
            return Err(ToolDenial::rejected(ToolDenialCode::OutputLimit));
        }
        Preflight::ensure_safe_output(&output)?;
        Ok(output)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PathArguments {
    path: String,
}

#[derive(Serialize)]
struct ReadReceipt {
    path: String,
    bytes: usize,
    sha256: String,
    content: String,
}

#[derive(Serialize)]
struct DirectoryReceipt {
    path: String,
    entries: Vec<DirectoryEntry>,
}

#[derive(Serialize)]
struct DirectoryEntry {
    name: String,
    kind: EntryKind,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum EntryKind {
    File,
    Directory,
    Symlink,
    Other,
}

fn parse_arguments<T: for<'de> Deserialize<'de>>(
    invocation: &ToolInvocation,
) -> Result<T, ToolDenial> {
    serde_json::from_str(&invocation.call.arguments_json)
        .map_err(|_| ToolDenial::rejected(ToolDenialCode::InvalidArguments))
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

fn path_for_output(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(windows)]
fn normalized_name(name: &str) -> String {
    name.to_lowercase()
}

#[cfg(not(windows))]
fn normalized_name(name: &str) -> String {
    name.to_owned()
}
