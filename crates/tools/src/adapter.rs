use std::path::Path;
use std::sync::Arc;

use minimax_core::{
    CancellationPort, PermissionMode, ToolExecutionContext, ToolFuture, ToolLifecycleError,
    ToolLifecycleFuture, ToolPort,
};
use minimax_protocol::{
    SHELL_TOOL_NAMES, ToolDefinition, ToolInvocation, ToolResult, ToolValidationError,
};

use crate::{
    ApplyPatchTool, BoundedProcess, GitDiffTool, GitStatusTool, ListDirectoryTool,
    NativePtyBackend, NpmDiagnosticTool, Preflight, ProcessShellSessionIds, ReadFileTool,
    RunDiagnosticTool, ShellCommandTool, ShellSessionManager, ShellSessionTool, SystemShellClock,
    ToolDenial, ToolDenialCode, ToolRegistry, WorkspaceRoot, WriteFileTool,
};

/// Concrete V1 tool boundary used by the CLI agent loop.
///
/// The registry, common preflight, workspace containment, and process limits are
/// composed here so callers cannot accidentally expose only part of the policy.
#[derive(Clone)]
pub struct BuiltinToolPort {
    workspace: WorkspaceRoot,
    process: BoundedProcess,
    shell_manager: ShellSessionManager,
}

impl BuiltinToolPort {
    pub fn new(root: impl AsRef<Path>, process: BoundedProcess) -> Result<Self, ToolDenial> {
        let ids = ProcessShellSessionIds::new()
            .map_err(|_| ToolDenial::failed(ToolDenialCode::ShellLaunchFailed))?;
        let shell_manager = ShellSessionManager::new(
            Arc::new(NativePtyBackend),
            Arc::new(ids),
            Arc::new(SystemShellClock),
        );
        Self::with_shell_manager(root, process, shell_manager)
    }

    pub fn with_shell_manager(
        root: impl AsRef<Path>,
        process: BoundedProcess,
        shell_manager: ShellSessionManager,
    ) -> Result<Self, ToolDenial> {
        Ok(Self {
            workspace: WorkspaceRoot::new(root)?,
            process,
            shell_manager,
        })
    }

    pub fn production(root: impl AsRef<Path>) -> Result<Self, ToolDenial> {
        Self::new(root, BoundedProcess::production())
    }

    pub fn definitions() -> Result<Vec<ToolDefinition>, ToolValidationError> {
        Self::definitions_for(PermissionMode::Confirm)
    }

    pub fn definitions_for(
        mode: PermissionMode,
    ) -> Result<Vec<ToolDefinition>, ToolValidationError> {
        ToolRegistry::specs_for(mode)
            .map(|specs| specs.into_iter().map(|spec| spec.definition).collect())
    }

    async fn dispatch(
        &self,
        invocation: &ToolInvocation,
        context: ToolExecutionContext,
        cancellation: &dyn CancellationPort,
    ) -> ToolResult {
        let sandbox_policy = context.sandbox_policy();
        if SHELL_TOOL_NAMES.contains(&invocation.call.name.as_str())
            && let Err(error) = Preflight::check_with_context(invocation, context, cancellation)
        {
            return error.into_result(invocation);
        }
        match invocation.call.name.as_str() {
            "read_file" => ReadFileTool::execute(&self.workspace, invocation, cancellation),
            "list_directory" => {
                ListDirectoryTool::execute(&self.workspace, invocation, cancellation)
            }
            "apply_patch" => ApplyPatchTool::execute(&self.workspace, invocation, cancellation),
            "write_file" => WriteFileTool::execute(&self.workspace, invocation, cancellation),
            "run_diagnostic" => {
                RunDiagnosticTool::new(self.process.clone())
                    .execute_with_policy(&self.workspace, invocation, sandbox_policy, cancellation)
                    .await
            }
            "git_status" => {
                GitStatusTool::new(self.process.clone())
                    .execute_with_policy(&self.workspace, invocation, sandbox_policy, cancellation)
                    .await
            }
            "git_diff" => {
                GitDiffTool::new(self.process.clone())
                    .execute_with_policy(&self.workspace, invocation, sandbox_policy, cancellation)
                    .await
            }
            "npm_diagnostic" => {
                NpmDiagnosticTool::new(self.process.clone())
                    .execute_with_policy(&self.workspace, invocation, sandbox_policy, cancellation)
                    .await
            }
            "shell_command" => {
                ShellCommandTool::new(self.shell_manager.clone())
                    .execute(&self.workspace, invocation, cancellation)
                    .await
            }
            "shell_session" => {
                ShellSessionTool::new(self.shell_manager.clone())
                    .execute(invocation, cancellation)
                    .await
            }
            _ => unreachable!("common preflight rejects tools outside the registered lists"),
        }
    }
}

impl ToolPort for BuiltinToolPort {
    fn preflight(
        &self,
        invocation: &ToolInvocation,
        context: ToolExecutionContext,
        cancellation: &dyn CancellationPort,
    ) -> Result<(), ToolResult> {
        Preflight::check_with_context(invocation, context, cancellation)
            .map(|_| ())
            .map_err(|error| error.into_result(invocation))
    }

    fn execute<'a>(
        &'a self,
        invocation: &'a ToolInvocation,
        context: ToolExecutionContext,
        cancellation: &'a dyn CancellationPort,
    ) -> ToolFuture<'a> {
        Box::pin(self.dispatch(invocation, context, cancellation))
    }

    fn transition_permission<'a>(&'a self, mode: PermissionMode) -> ToolLifecycleFuture<'a> {
        Box::pin(async move {
            match mode {
                PermissionMode::FullAccess => {
                    self.shell_manager.enable().await;
                    Ok(())
                }
                PermissionMode::Confirm => self
                    .shell_manager
                    .disable_and_stop_all()
                    .await
                    .map_err(shell_lifecycle_error),
            }
        })
    }

    fn shutdown<'a>(&'a self) -> ToolLifecycleFuture<'a> {
        Box::pin(async move {
            self.shell_manager
                .shutdown()
                .await
                .map_err(shell_lifecycle_error)
        })
    }
}

fn shell_lifecycle_error(error: crate::ShellCleanupError) -> ToolLifecycleError {
    ToolLifecycleError {
        code: "shell_stop_indeterminate",
        session_ids: error
            .session_ids
            .into_iter()
            .map(|session_id| session_id.as_str().to_owned())
            .collect(),
    }
}
