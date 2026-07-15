use std::path::Path;

use minimax_core::{CancellationPort, ToolFuture, ToolPort};
use minimax_protocol::{ToolDefinition, ToolInvocation, ToolResult, ToolValidationError};

use crate::{
    ApplyPatchTool, BoundedProcess, GitDiffTool, GitStatusTool, ListDirectoryTool,
    NpmDiagnosticTool, Preflight, ReadFileTool, RunDiagnosticTool, ToolDenial, ToolRegistry,
    WorkspaceRoot, WriteFileTool,
};

/// Concrete V1 tool boundary used by the CLI agent loop.
///
/// The registry, common preflight, workspace containment, and process limits are
/// composed here so callers cannot accidentally expose only part of the policy.
#[derive(Clone)]
pub struct BuiltinToolPort {
    workspace: WorkspaceRoot,
    process: BoundedProcess,
}

impl BuiltinToolPort {
    pub fn new(root: impl AsRef<Path>, process: BoundedProcess) -> Result<Self, ToolDenial> {
        Ok(Self {
            workspace: WorkspaceRoot::new(root)?,
            process,
        })
    }

    pub fn production(root: impl AsRef<Path>) -> Result<Self, ToolDenial> {
        Self::new(root, BoundedProcess::production())
    }

    pub fn definitions() -> Result<Vec<ToolDefinition>, ToolValidationError> {
        ToolRegistry::specs().map(|specs| specs.into_iter().map(|spec| spec.definition).collect())
    }

    async fn dispatch(
        &self,
        invocation: &ToolInvocation,
        cancellation: &dyn CancellationPort,
    ) -> ToolResult {
        match invocation.call.name.as_str() {
            "read_file" => ReadFileTool::execute(&self.workspace, invocation, cancellation),
            "list_directory" => {
                ListDirectoryTool::execute(&self.workspace, invocation, cancellation)
            }
            "apply_patch" => ApplyPatchTool::execute(&self.workspace, invocation, cancellation),
            "write_file" => WriteFileTool::execute(&self.workspace, invocation, cancellation),
            "run_diagnostic" => {
                RunDiagnosticTool::new(self.process.clone())
                    .execute(&self.workspace, invocation, cancellation)
                    .await
            }
            "git_status" => {
                GitStatusTool::new(self.process.clone())
                    .execute(&self.workspace, invocation, cancellation)
                    .await
            }
            "git_diff" => {
                GitDiffTool::new(self.process.clone())
                    .execute(&self.workspace, invocation, cancellation)
                    .await
            }
            "npm_diagnostic" => {
                NpmDiagnosticTool::new(self.process.clone())
                    .execute(&self.workspace, invocation, cancellation)
                    .await
            }
            _ => unreachable!("common preflight rejects tools outside the V1 registry"),
        }
    }
}

impl ToolPort for BuiltinToolPort {
    fn preflight(
        &self,
        invocation: &ToolInvocation,
        cancellation: &dyn CancellationPort,
    ) -> Result<(), ToolResult> {
        Preflight::check(invocation, cancellation)
            .map(|_| ())
            .map_err(|error| error.into_result(invocation))
    }

    fn execute<'a>(
        &'a self,
        invocation: &'a ToolInvocation,
        cancellation: &'a dyn CancellationPort,
    ) -> ToolFuture<'a> {
        Box::pin(self.dispatch(invocation, cancellation))
    }
}
