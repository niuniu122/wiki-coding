//! Bounded tool adapters and effect boundaries for the Rust runtime.
//!
//! Existing bounded tools cross the same policy gates in both permission modes.
//! Arbitrary Shell tools additionally require a full-access execution context.

mod adapter;
mod error;
mod git;
mod npm;
mod path;
mod policy;
mod process;
mod read;
mod sandbox;
mod shell;
#[path = "shell/command.rs"]
mod shell_command;
#[path = "shell/session.rs"]
mod shell_session;
mod write;

/// Unstable process-private protocol shared by the CLI shell host and native backend.
#[doc(hidden)]
pub mod internal_shell_host {
    pub use crate::shell::host::{
        HostBootstrap, HostChannel, HostControl, HostEvent, HostListener, HostProtocolError,
        HostSupervisor, INTERNAL_HOST_ARGUMENT, ParentChannel, RootExit, run_host_lifecycle,
        run_internal_shell_host,
    };
}

pub use error::{ToolDenial, ToolDenialCode};
pub use git::{GitDiffTool, GitStatusTool};
pub use minimax_core::CancellationPort as CancellationSignal;
pub use npm::NpmDiagnosticTool;
pub use path::{ResolvedToolPath, WorkspaceRoot};
pub use policy::{NeverCancelled, Preflight, ToolRegistry, ToolSpec};
pub use process::{
    BoundedProcess, ChildEvent, ChildEventFuture, ChildStopFuture, DirectChild, ProcessCompletion,
    ProcessLaunchError, ProcessLauncher, ProcessLimits, ProcessRequest, RunDiagnosticTool,
    SandboxCapability, SandboxCapabilityState, SandboxLaunchReceipt, TokioProcessLauncher,
};
pub use read::{ListDirectoryTool, ReadFileTool};
pub use shell::{
    DEFAULT_COMMAND_YIELD, DEFAULT_POLL_YIELD, DEFAULT_WRITE_YIELD, MAX_RUNNING_SHELL_SESSIONS,
    MAX_TERMINAL_SHELL_RECEIPTS, NativePtyBackend, ProcessShellSessionIds, PtyBackend, PtyChild,
    PtyGuard, PtyTerminateFuture, ReaderSpawner, ReaderTask, ShellCleanupError,
    ShellCommandRequest, ShellManagerError, ShellOutputBudget, ShellOutputBuffer, ShellOutputChunk,
    ShellPollRequest, ShellSessionIdSource, ShellSessionManager, ShellSpawnRequest,
    ShellWriteRequest, SpawnedPty, SystemReaderSpawner, SystemShellClock, TERMINAL_RECEIPT_TTL,
};
pub use shell_command::{
    SHELL_EXITED, SHELL_NONZERO_EXIT, SHELL_RUNNING, SHELL_STOPPED, ShellCommandTool,
};
pub use shell_session::ShellSessionTool;
pub use write::{ApplyPatchTool, WriteFileTool};

/// Human-readable boundary used by architecture checks and documentation.
pub const CRATE_ROLE: &str = "permission-aware tool adapters and external effects";
pub use adapter::BuiltinToolPort;
