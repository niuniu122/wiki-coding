use std::fmt;

use minimax_protocol::{SchemaVersion, ToolInvocation, ToolResult, ToolTerminalStatus};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolDenialCode {
    Cancelled,
    UnknownTool,
    InvalidArguments,
    EffectMismatch,
    InvalidPath,
    OutsideWorkspace,
    ProtectedPath,
    SecretPath,
    SecretContent,
    PathNotFound,
    WrongFileType,
    BinaryFile,
    InputLimit,
    OutputLimit,
    EntryLimit,
    HashConflict,
    OccurrenceConflict,
    OverlappingMatches,
    AlreadyExists,
    IoDenied,
    IoFailed,
    SpawnFailed,
    ProcessIo,
    NonzeroExit,
    TimedOut,
    UnsafeScript,
    CleanupUnknown,
}

impl ToolDenialCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cancelled => "cancelled",
            Self::UnknownTool => "unknown_tool",
            Self::InvalidArguments => "invalid_arguments",
            Self::EffectMismatch => "effect_mismatch",
            Self::InvalidPath => "invalid_path",
            Self::OutsideWorkspace => "outside_workspace",
            Self::ProtectedPath => "protected_path",
            Self::SecretPath => "secret_path",
            Self::SecretContent => "secret_content",
            Self::PathNotFound => "path_not_found",
            Self::WrongFileType => "wrong_file_type",
            Self::BinaryFile => "binary_file",
            Self::InputLimit => "input_limit",
            Self::OutputLimit => "output_limit",
            Self::EntryLimit => "entry_limit",
            Self::HashConflict => "hash_conflict",
            Self::OccurrenceConflict => "occurrence_conflict",
            Self::OverlappingMatches => "overlapping_matches",
            Self::AlreadyExists => "already_exists",
            Self::IoDenied => "io_denied",
            Self::IoFailed => "io_failed",
            Self::SpawnFailed => "spawn_failed",
            Self::ProcessIo => "process_io",
            Self::NonzeroExit => "nonzero_exit",
            Self::TimedOut => "timed_out",
            Self::UnsafeScript => "unsafe_script",
            Self::CleanupUnknown => "cleanup_unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ToolDenial {
    code: ToolDenialCode,
    status: ToolTerminalStatus,
}

impl ToolDenial {
    #[must_use]
    pub const fn rejected(code: ToolDenialCode) -> Self {
        Self {
            code,
            status: ToolTerminalStatus::Rejected,
        }
    }

    #[must_use]
    pub const fn failed(code: ToolDenialCode) -> Self {
        Self {
            code,
            status: ToolTerminalStatus::Failed,
        }
    }

    #[must_use]
    pub const fn cancelled() -> Self {
        Self {
            code: ToolDenialCode::Cancelled,
            status: ToolTerminalStatus::Cancelled,
        }
    }

    #[must_use]
    pub const fn code(self) -> ToolDenialCode {
        self.code
    }

    #[must_use]
    pub const fn status(self) -> ToolTerminalStatus {
        self.status
    }

    #[must_use]
    pub fn into_result(self, invocation: &ToolInvocation) -> ToolResult {
        ToolResult {
            schema_version: SchemaVersion,
            call_id: invocation.call.call_id.clone(),
            tool_name: invocation.call.name.clone(),
            status: self.status,
            code: self.code.as_str().to_owned(),
            output: None,
        }
    }
}

impl fmt::Display for ToolDenial {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code.as_str())
    }
}

impl std::error::Error for ToolDenial {}

pub(crate) fn io_denial(error: &std::io::Error) -> ToolDenial {
    if error.kind() == std::io::ErrorKind::PermissionDenied {
        ToolDenial::failed(ToolDenialCode::IoDenied)
    } else {
        ToolDenial::failed(ToolDenialCode::IoFailed)
    }
}
