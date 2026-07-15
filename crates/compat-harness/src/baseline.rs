use std::fmt;
use std::path::Path;

use minimax_tui::{CommandIntent, ParsedInput, parse_input};

use crate::CommandManifest;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BaselineError {
    Command(String),
    PermissionModes,
    PackageRead,
    PackageParse,
    ProductEntry,
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
        }
    }
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
