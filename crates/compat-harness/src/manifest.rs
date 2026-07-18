use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const SUPPORTED_SCHEMA_VERSION: u16 = 1;
const CONTRACT_VERSION: &str = "v1";
const PROVENANCE_COMMIT: &str = "84784f5";
const PRODUCT_ENTRY: &str = "bin/minimax-codex.cjs";

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
pub struct PublicContractManifest {
    pub schema_version: u16,
    pub contract_version: String,
    pub provenance_commit: String,
    pub content_fingerprint: String,
    pub product_entry: String,
    pub required_item_ids: Vec<String>,
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
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StatusItem {
    pub id: String,
    pub status: ParityStatus,
    #[serde(default)]
    pub evidence: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approved_difference: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompatManifests {
    pub commands: CommandManifest,
    pub providers: ProviderManifest,
    pub public_contract: PublicContractManifest,
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
    let public_contract = read_json(root, "fixtures/compat/public-contract.v1.json")?;
    let manifests = CompatManifests {
        commands,
        providers,
        public_contract,
    };
    validate_manifests(root, &manifests)?;
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

fn validate_manifests(root: &Path, manifests: &CompatManifests) -> Result<(), ManifestError> {
    for (name, version) in [
        ("commands", manifests.commands.schema_version),
        ("providers", manifests.providers.schema_version),
        ("public contract", manifests.public_contract.schema_version),
    ] {
        if version != SUPPORTED_SCHEMA_VERSION {
            return Err(ManifestError::Validation(format!(
                "unsupported {name} manifest schema version: {version}"
            )));
        }
    }
    if manifests.public_contract.contract_version != CONTRACT_VERSION {
        return Err(ManifestError::Validation(
            "public contract version must remain v1".to_owned(),
        ));
    }
    if manifests.public_contract.provenance_commit != PROVENANCE_COMMIT {
        return Err(ManifestError::Validation(
            "public contract provenance commit must remain 84784f5".to_owned(),
        ));
    }
    if manifests.public_contract.product_entry != PRODUCT_ENTRY {
        return Err(ManifestError::Validation(
            "public contract product entry must be the evidence-gated Rust launcher".to_owned(),
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
        manifests
            .public_contract
            .required_item_ids
            .iter()
            .map(String::as_str),
        "required public contract item",
    )?;
    ensure_unique(
        manifests
            .public_contract
            .items
            .iter()
            .map(|item| item.id.as_str()),
        "public contract item",
    )?;

    let expected_ids = expected_contract_ids(manifests);
    let required_ids = manifests
        .public_contract
        .required_item_ids
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let item_ids = manifests
        .public_contract
        .items
        .iter()
        .map(|item| item.id.clone())
        .collect::<BTreeSet<_>>();
    if manifests
        .public_contract
        .required_item_ids
        .windows(2)
        .any(|pair| pair[0] >= pair[1])
        || required_ids != expected_ids
        || item_ids != required_ids
        || manifests.public_contract.items.len() != required_ids.len()
    {
        return Err(ManifestError::Validation(
            "public contract items must exactly cover the sorted required contract IDs".to_owned(),
        ));
    }

    for item in &manifests.public_contract.items {
        if !item.id.starts_with("contract.")
            || item.id.contains("typescript")
            || item.id.starts_with("rust.")
        {
            return Err(ManifestError::Validation(format!(
                "public contract item has an invalid stable ID: {}",
                item.id
            )));
        }
        if item.status == ParityStatus::Pending || item.evidence.is_empty() {
            return Err(ManifestError::Validation(format!(
                "public contract item requires current Rust evidence: {}",
                item.id
            )));
        }
        for evidence in &item.evidence {
            if evidence.trim().is_empty() || !root.join(evidence).is_file() {
                return Err(ManifestError::Validation(format!(
                    "public contract item references missing evidence: {} -> {}",
                    item.id, evidence
                )));
            }
        }
        match item.status {
            ParityStatus::Matched if item.approved_difference.is_some() => {
                return Err(ManifestError::Validation(format!(
                    "matched public contract item cannot approve a difference: {}",
                    item.id
                )));
            }
            ParityStatus::ApprovedDifference
                if item
                    .approved_difference
                    .as_deref()
                    .is_none_or(|id| !id.starts_with("difference.command.")) =>
            {
                return Err(ManifestError::Validation(format!(
                    "approved public contract difference requires a stable ID: {}",
                    item.id
                )));
            }
            ParityStatus::Pending | ParityStatus::Matched | ParityStatus::ApprovedDifference => {}
        }
    }

    let actual_fingerprint = contract_fingerprint(&manifests.public_contract)?;
    if manifests.public_contract.content_fingerprint != actual_fingerprint {
        return Err(ManifestError::Validation(
            "public contract content fingerprint mismatch".to_owned(),
        ));
    }
    Ok(())
}

fn expected_contract_ids(manifests: &CompatManifests) -> BTreeSet<String> {
    let mut ids = BTreeSet::from([
        "contract.migration".to_owned(),
        "contract.permission_modes".to_owned(),
        "contract.product_entry".to_owned(),
        "contract.release_gate".to_owned(),
        "contract.requirement.TOOL-01".to_owned(),
        "contract.requirement.TOOL-02".to_owned(),
        "contract.requirement.TOOL-03".to_owned(),
        "contract.requirement.TOOL-04".to_owned(),
        "contract.requirement.TOOL-05".to_owned(),
        "contract.retrieval".to_owned(),
        "contract.vault".to_owned(),
    ]);
    for command in &manifests.commands.commands {
        ids.insert(format!("contract.command.{}", command.name));
        ids.extend(
            command
                .aliases
                .iter()
                .map(|alias| format!("contract.command.{alias}")),
        );
    }
    ids.extend(
        manifests
            .providers
            .profile_classes
            .iter()
            .map(|profile| format!("contract.provider_profile.{}", profile.id)),
    );
    ids.extend(
        manifests
            .providers
            .protocols
            .iter()
            .map(|protocol| format!("contract.provider_protocol.{protocol}")),
    );
    ids
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ContractFingerprintInput<'a> {
    schema_version: u16,
    contract_version: &'a str,
    provenance_commit: &'a str,
    product_entry: &'a str,
    required_item_ids: &'a [String],
    items: &'a [StatusItem],
}

fn contract_fingerprint(manifest: &PublicContractManifest) -> Result<String, ManifestError> {
    let input = ContractFingerprintInput {
        schema_version: manifest.schema_version,
        contract_version: &manifest.contract_version,
        provenance_commit: &manifest.provenance_commit,
        product_entry: &manifest.product_entry,
        required_item_ids: &manifest.required_item_ids,
        items: &manifest.items,
    };
    let bytes = serde_json::to_vec(&input)
        .map_err(|_| ManifestError::Validation("cannot fingerprint public contract".to_owned()))?;
    let digest = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(format!("sha256:{digest}"))
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
