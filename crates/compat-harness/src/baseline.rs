use std::fmt;
use std::path::Path;

use minimax_tui::{CommandAvailability, CommandIntent, ParsedInput, parse_input};

use crate::{BaselineStatus, CommandManifest, ParityStatus};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BaselineError {
    Command(String),
    PermissionModes,
    PackageRead,
    PackageParse,
    ProductEntry,
    ToolEvidence(String),
}

impl fmt::Display for BaselineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Command(command) => write!(formatter, "Rust command route is missing: {command}"),
            Self::PermissionModes => {
                formatter.write_str("Rust permission names must remain confirm and full-access")
            }
            Self::PackageRead => formatter.write_str("cannot read package.json"),
            Self::PackageParse => formatter.write_str("package.json is invalid"),
            Self::ProductEntry => {
                formatter.write_str("the npm product entry must remain dist/cli.js")
            }
            Self::ToolEvidence(requirement) => {
                write!(formatter, "Rust tool evidence is incomplete: {requirement}")
            }
        }
    }
}

pub fn validate_rust_tool_evidence(
    root: &Path,
    baseline: &BaselineStatus,
) -> Result<(), BaselineError> {
    let e2e = std::fs::read_to_string(root.join("fixtures/compat/tools/e2e.v1.json"))
        .map_err(|_| BaselineError::ToolEvidence("e2e fixture".to_owned()))?;
    let e2e: serde_json::Value = serde_json::from_str(&e2e)
        .map_err(|_| BaselineError::ToolEvidence("e2e fixture".to_owned()))?;
    let cases = e2e
        .get("cases")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| BaselineError::ToolEvidence("e2e fixture".to_owned()))?;
    if e2e.get("schemaVersion").and_then(serde_json::Value::as_u64) != Some(1)
        || cases.len() != 2
        || cases.iter().any(|case| {
            case.get("calls")
                .and_then(serde_json::Value::as_array)
                .is_none_or(|calls| calls.len() != 2)
        })
    {
        return Err(BaselineError::ToolEvidence("e2e fixture".to_owned()));
    }

    for requirement in ["TOOL-01", "TOOL-02", "TOOL-03", "TOOL-04", "TOOL-05"] {
        let id = format!("rust.requirement.{requirement}");
        let item = baseline
            .items
            .iter()
            .find(|item| item.id == id)
            .ok_or_else(|| BaselineError::ToolEvidence(requirement.to_owned()))?;
        if item.status != ParityStatus::Matched
            || item.evidence.is_empty()
            || item.evidence.iter().any(|path| !root.join(path).is_file())
        {
            return Err(BaselineError::ToolEvidence(requirement.to_owned()));
        }
    }
    Ok(())
}

impl std::error::Error for BaselineError {}

pub fn validate_rust_command_surface(manifest: &CommandManifest) -> Result<(), BaselineError> {
    for command in &manifest.commands {
        for name in std::iter::once(&command.name).chain(&command.aliases) {
            let input = match command.argument.as_str() {
                "required" => format!("{name} fixture"),
                "none" | "optional" => name.clone(),
                _ => return Err(BaselineError::Command(name.clone())),
            };
            let parsed = parse_input(&input).map_err(|_| BaselineError::Command(name.clone()))?;
            let ParsedInput::Command(intent) = parsed else {
                return Err(BaselineError::Command(name.clone()));
            };
            if name == "/quit" && intent != CommandIntent::Exit {
                return Err(BaselineError::Command(name.clone()));
            }
            if matches!(name.as_str(), "/agent" | "/continue" | "/permissions")
                && intent.availability() != CommandAvailability::Available
            {
                return Err(BaselineError::Command(name.clone()));
            }
        }
    }
    if manifest.target_permission_modes != ["confirm", "full-access"]
        || parse_input("/permissions confirm").is_err()
        || parse_input("/permissions full-access").is_err()
        || parse_input("/permissions workspace-read").is_ok()
    {
        return Err(BaselineError::PermissionModes);
    }
    Ok(())
}

pub fn validate_product_entry(root: &Path) -> Result<(), BaselineError> {
    let raw = std::fs::read_to_string(root.join("package.json"))
        .map_err(|_| BaselineError::PackageRead)?;
    let package: serde_json::Value =
        serde_json::from_str(&raw).map_err(|_| BaselineError::PackageParse)?;
    if package
        .get("bin")
        .and_then(|bin| bin.get("minimax-codex"))
        .and_then(serde_json::Value::as_str)
        != Some("dist/cli.js")
    {
        return Err(BaselineError::ProductEntry);
    }
    Ok(())
}
