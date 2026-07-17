use std::path::Path;

use minimax_core::PermissionMode;
use minimax_protocol::RuntimeErrorCode;
use minimax_provider::{ConfigSource, CredentialError, CredentialSource, ResolvedConfig};
use minimax_tools::{SandboxCapability, SandboxCapabilityState};
use minimax_vault::{RuntimeStore, RuntimeStoreError};
use serde::Serialize;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Pass,
    Warn,
    Fail,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DoctorCheck {
    pub name: &'static str,
    pub status: CheckStatus,
    pub detail: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DoctorReport {
    pub schema_version: u16,
    pub healthy: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_source: Option<ConfigSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_source: Option<CredentialSource>,
    pub checks: Vec<DoctorCheck>,
}

#[must_use]
pub fn permission_status(mode: PermissionMode, capability: SandboxCapability) -> String {
    match mode {
        PermissionMode::Confirm => format!(
            "permission mode: confirm | approval: required | subprocess sandbox: {} ({}) | {}",
            capability.state().as_str(),
            capability.backend(),
            capability.detail()
        ),
        PermissionMode::FullAccess => format!(
            "permission mode: full-access | approval: skipped | subprocess sandbox: {} | trusted projects only; workspace, secret, command, size, timeout, and cancellation gates remain enforced",
            SandboxCapabilityState::DisabledByFullAccess.as_str()
        ),
    }
}

#[must_use]
pub fn inspect(
    project_root: &Path,
    config: Result<&ResolvedConfig, RuntimeErrorCode>,
    credential: Result<CredentialSource, CredentialError>,
    terminal_capable: bool,
) -> DoctorReport {
    let config_source = config.as_ref().ok().map(|config| config.source);
    let credential_source = credential.as_ref().ok().copied();
    let mut checks = Vec::new();
    checks.push(match config {
        Ok(_) => pass("provider_config", "public provider configuration is valid"),
        Err(_) => fail(
            "provider_config",
            "provider configuration is invalid; inspect the layered config files",
        ),
    });
    checks.push(match credential {
        Ok(CredentialSource::Environment) => pass(
            "credential_source",
            "a credential is available from the configured environment key",
        ),
        Ok(CredentialSource::OsKeyring) => pass(
            "credential_source",
            "a credential is available from the OS credential store",
        ),
        Err(CredentialError::Missing) => fail(
            "credential_source",
            "no credential is available for the selected mode",
        ),
        Err(CredentialError::Unavailable) => fail(
            "credential_source",
            "the OS credential store is unavailable",
        ),
        Err(CredentialError::Locked) => {
            fail("credential_source", "the OS credential store is locked")
        }
        Err(CredentialError::Denied) => {
            fail("credential_source", "OS credential access was denied")
        }
        Err(CredentialError::Unknown) => fail(
            "credential_source",
            "OS credential access failed without a public detail",
        ),
    });

    if project_root.is_dir() {
        checks.push(pass("project_root", "the project root is accessible"));
        match RuntimeStore::open(project_root) {
            Ok(store) => {
                checks.push(pass(
                    "runtime_lease",
                    "the runtime writer lease is available",
                ));
                checks.push(if store.journal_path().is_file() {
                    pass("runtime_journal", "the runtime journal is recoverable")
                } else {
                    fail("runtime_journal", "the runtime journal is unavailable")
                });
                checks.push(if store.current_index_path().is_file() {
                    pass("runtime_index", "the derived runtime index is consistent")
                } else {
                    fail("runtime_index", "the derived runtime index is unavailable")
                });
            }
            Err(RuntimeStoreError::Busy) => checks.push(fail(
                "runtime_lease",
                "another process currently owns the runtime writer lease",
            )),
            Err(_) => checks.push(fail(
                "runtime_recovery",
                "the runtime journal or derived index requires repair",
            )),
        }
    } else {
        checks.push(fail(
            "project_root",
            "the project root does not exist or is not a directory",
        ));
    }

    let sandbox = SandboxCapability::detect(project_root);
    checks.push(match sandbox.state() {
        SandboxCapabilityState::Enforced => pass("subprocess_sandbox", sandbox.detail()),
        SandboxCapabilityState::Unavailable | SandboxCapabilityState::Unsupported => {
            warn("subprocess_sandbox", sandbox.detail())
        }
        SandboxCapabilityState::DisabledByFullAccess => warn(
            "subprocess_sandbox",
            "full-access disables subprocess isolation for this process",
        ),
    });

    checks.push(if terminal_capable {
        pass(
            "terminal",
            "interactive terminal input and output are available",
        )
    } else {
        warn(
            "terminal",
            "interactive raw mode is unavailable; use line or JSONL mode",
        )
    });
    DoctorReport {
        schema_version: 1,
        healthy: checks.iter().all(|check| check.status != CheckStatus::Fail),
        config_source,
        credential_source,
        checks,
    }
}

const fn pass(name: &'static str, detail: &'static str) -> DoctorCheck {
    DoctorCheck {
        name,
        status: CheckStatus::Pass,
        detail,
    }
}

const fn warn(name: &'static str, detail: &'static str) -> DoctorCheck {
    DoctorCheck {
        name,
        status: CheckStatus::Warn,
        detail,
    }
}

const fn fail(name: &'static str, detail: &'static str) -> DoctorCheck {
    DoctorCheck {
        name,
        status: CheckStatus::Fail,
        detail,
    }
}
