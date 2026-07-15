use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const SUPPORTED_SCHEMA_VERSION: u16 = 1;
const BASELINE_COMMIT: &str = "84784f5";
const PRODUCT_ENTRY: &str = "dist/cli.js";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommandManifest {
    pub schema_version: u16,
    pub commands: Vec<CommandItem>,
    pub target_permission_modes: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CommandItem {
    pub name: String,
    pub aliases: Vec<String>,
    pub argument: String,
    pub outcome: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderManifest {
    pub schema_version: u16,
    pub protocols: Vec<String>,
    pub profile_classes: Vec<ProviderProfile>,
    pub feature_matrix: ProviderFeatureMatrix,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderProfile {
    pub id: String,
    pub source: String,
    pub provider_profile_id: String,
    pub protocols: Vec<String>,
    pub credential_bindings: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProviderFeatureMatrix {
    pub streaming: bool,
    pub native_tool_calls: bool,
    pub parallel_tool_calls: bool,
    pub structured_output: bool,
    pub reasoning_metadata: bool,
    pub usage: bool,
    pub prompt_caching: bool,
    pub image_input: bool,
    pub audio_input: bool,
    pub provider_hosted_tools: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BaselineStatus {
    pub schema_version: u16,
    pub baseline_commit: String,
    pub product_entry: String,
    pub items: Vec<StatusItem>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ParityStatus {
    Matched,
    Pending,
    ApprovedDifference,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StatusItem {
    pub id: String,
    pub status: ParityStatus,
    #[serde(default)]
    pub evidence: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub difference: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompatManifests {
    pub commands: CommandManifest,
    pub providers: ProviderManifest,
    pub baseline: BaselineStatus,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ManifestError {
    Read { file: String },
    Parse { file: String },
    Validation(String),
}

impl fmt::Display for ManifestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { file } => write!(formatter, "cannot read compatibility file: {file}"),
            Self::Parse { file } => write!(formatter, "invalid compatibility JSON: {file}"),
            Self::Validation(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for ManifestError {}

#[must_use]
pub fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("compat-harness must be nested under crates")
        .to_path_buf()
}

pub fn load_compat_manifests(root: &Path) -> Result<CompatManifests, ManifestError> {
    let commands = read_json(root, "fixtures/compat/commands.v1.json")?;
    let providers = read_json(root, "fixtures/compat/providers.v1.json")?;
    let baseline = read_json(root, "fixtures/compat/baseline-status.v1.json")?;
    let manifests = CompatManifests {
        commands,
        providers,
        baseline,
    };
    validate_manifests(&manifests)?;
    Ok(manifests)
}

fn read_json<T: for<'de> Deserialize<'de>>(
    root: &Path,
    relative: &str,
) -> Result<T, ManifestError> {
    let raw = fs::read_to_string(root.join(relative)).map_err(|_| ManifestError::Read {
        file: relative.to_owned(),
    })?;
    serde_json::from_str(&raw).map_err(|_| ManifestError::Parse {
        file: relative.to_owned(),
    })
}

fn validate_manifests(manifests: &CompatManifests) -> Result<(), ManifestError> {
    for (name, version) in [
        ("commands", manifests.commands.schema_version),
        ("providers", manifests.providers.schema_version),
        ("baseline", manifests.baseline.schema_version),
    ] {
        if version != SUPPORTED_SCHEMA_VERSION {
            return Err(ManifestError::Validation(format!(
                "unsupported {name} manifest schema version: {version}"
            )));
        }
    }
    if manifests.baseline.baseline_commit != BASELINE_COMMIT {
        return Err(ManifestError::Validation(
            "compatibility baseline commit must remain 84784f5".to_owned(),
        ));
    }
    if manifests.baseline.product_entry != PRODUCT_ENTRY {
        return Err(ManifestError::Validation(
            "compatibility product entry must remain dist/cli.js".to_owned(),
        ));
    }
    ensure_unique(
        manifests.commands.commands.iter().flat_map(|command| {
            std::iter::once(command.name.as_str()).chain(command.aliases.iter().map(String::as_str))
        }),
        "command name or alias",
    )?;
    ensure_unique(
        manifests
            .providers
            .profile_classes
            .iter()
            .map(|profile| profile.id.as_str()),
        "provider profile",
    )?;
    ensure_unique(
        manifests.providers.protocols.iter().map(String::as_str),
        "provider protocol",
    )?;
    ensure_unique(
        manifests.baseline.items.iter().map(|item| item.id.as_str()),
        "compatibility item",
    )?;
    Ok(())
}

fn ensure_unique<'a>(
    values: impl IntoIterator<Item = &'a str>,
    label: &str,
) -> Result<(), ManifestError> {
    let mut seen = BTreeSet::new();
    for value in values {
        if value.trim().is_empty() {
            return Err(ManifestError::Validation(format!(
                "{label} must not be empty"
            )));
        }
        if !seen.insert(value) {
            return Err(ManifestError::Validation(format!(
                "duplicate {label}: {value}"
            )));
        }
    }
    Ok(())
}
