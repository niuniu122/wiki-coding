use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::Path;
use std::process::Command;

use minimax_protocol::ProviderProtocolKind;
use minimax_provider::{ConfigLayer, resolve_config};
use minimax_tui::{CommandAvailability, CommandIntent, ParsedInput, parse_input};
use serde::Deserialize;
use sha2::{Digest as _, Sha256};

use crate::source_authority::validate_package_product_scripts;
use crate::{CommandManifest, ParityStatus, ProviderManifest, PublicContractManifest};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BaselineError {
    Command(String),
    PermissionModes,
    PackageRead,
    PackageParse,
    ProductEntry,
    ToolEvidence(String),
    VaultEvidence,
    RetrievalEvidence,
    ProviderEvidence,
    CutoverEvidence,
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
            Self::ProductEntry => formatter.write_str(
                "the npm product entry must be the sole fixed Rust launcher with no legacy route",
            ),
            Self::ToolEvidence(requirement) => {
                write!(formatter, "Rust tool evidence is incomplete: {requirement}")
            }
            Self::VaultEvidence => formatter.write_str("Rust Vault evidence is incomplete"),
            Self::RetrievalEvidence => formatter.write_str("Rust retrieval evidence is incomplete"),
            Self::ProviderEvidence => {
                formatter.write_str("Rust Provider profile evidence is incomplete")
            }
            Self::CutoverEvidence => formatter.write_str("Rust cutover evidence is incomplete"),
        }
    }
}

pub fn validate_rust_retrieval_evidence(root: &Path) -> Result<(), BaselineError> {
    let raw = std::fs::read_to_string(root.join("fixtures/compat/retrieval/explanations.v1.json"))
        .map_err(|_| BaselineError::RetrievalEvidence)?;
    let fixture: serde_json::Value =
        serde_json::from_str(&raw).map_err(|_| BaselineError::RetrievalEvidence)?;
    let domains = fixture
        .get("domains")
        .and_then(serde_json::Value::as_array)
        .ok_or(BaselineError::RetrievalEvidence)?;
    let project_facts = fixture
        .get("projectFacts")
        .and_then(serde_json::Value::as_array)
        .ok_or(BaselineError::RetrievalEvidence)?;
    if fixture
        .get("schemaVersion")
        .and_then(serde_json::Value::as_u64)
        != Some(1)
        || domains.len() != 3
        || project_facts.len() != 8
        || fixture
            .get("discoveredProjectExecution")
            .and_then(serde_json::Value::as_bool)
            != Some(false)
        || fixture
            .get("liveNetwork")
            .and_then(serde_json::Value::as_bool)
            != Some(false)
        || fixture
            .get("modelDownload")
            .and_then(serde_json::Value::as_bool)
            != Some(false)
        || fixture
            .get("productEntry")
            .and_then(serde_json::Value::as_str)
            != Some("bin/minimax-codex.cjs")
    {
        return Err(BaselineError::RetrievalEvidence);
    }
    for path in [
        "fixtures/compat/retrieval/lexical.v1.json",
        "fixtures/compat/retrieval/projects.v1.json",
        "fixtures/compat/retrieval/embedding-resource.v1.json",
        "crates/retrieval/tests/lexical.rs",
        "crates/retrieval/tests/project_discovery.rs",
        "crates/retrieval/tests/embedding_resource.rs",
        "crates/retrieval/tests/benchmark.rs",
        "crates/cli/tests/index_commands.rs",
        "crates/cli/tests/discovery_commands.rs",
    ] {
        if !root.join(path).is_file() {
            return Err(BaselineError::RetrievalEvidence);
        }
    }
    validate_product_entry(root).map_err(|_| BaselineError::RetrievalEvidence)
}

pub fn validate_rust_vault_evidence(root: &Path) -> Result<(), BaselineError> {
    let raw = std::fs::read_to_string(root.join("fixtures/compat/vault/maintenance.v1.json"))
        .map_err(|_| BaselineError::VaultEvidence)?;
    let fixture: serde_json::Value =
        serde_json::from_str(&raw).map_err(|_| BaselineError::VaultEvidence)?;
    let cases = fixture
        .get("cases")
        .and_then(serde_json::Value::as_array)
        .ok_or(BaselineError::VaultEvidence)?;
    if fixture
        .get("schemaVersion")
        .and_then(serde_json::Value::as_u64)
        != Some(1)
        || fixture.get("database").and_then(serde_json::Value::as_bool) != Some(false)
        || fixture
            .get("liveProvider")
            .and_then(serde_json::Value::as_bool)
            != Some(false)
        || fixture
            .get("productEntry")
            .and_then(serde_json::Value::as_str)
            != Some("bin/minimax-codex.cjs")
        || cases.len() != 8
        || cases
            .iter()
            .any(|case| case.get("id").and_then(serde_json::Value::as_str).is_none())
    {
        return Err(BaselineError::VaultEvidence);
    }
    for path in [
        "crates/vault/tests/maintenance.rs",
        "crates/vault/tests/retention.rs",
        "crates/cli/tests/vault_commands.rs",
        "fixtures/compat/wiki/main-model-workflow.v1.json",
    ] {
        if !root.join(path).is_file() {
            return Err(BaselineError::VaultEvidence);
        }
    }
    Ok(())
}

pub fn validate_rust_tool_evidence(
    root: &Path,
    public_contract: &PublicContractManifest,
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
        let id = format!("contract.requirement.{requirement}");
        let item = public_contract
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

pub fn validate_rust_provider_profiles(manifest: &ProviderManifest) -> Result<(), BaselineError> {
    let expected = [
        (
            "minimax_official",
            "builtin",
            "provider:minimax/official",
            &["responses"][..],
            &["MINIMAX_API_KEY"][..],
        ),
        (
            "minimax_hashsight",
            "builtin",
            "provider:minimax/hashsight",
            &["chat_completions"][..],
            &["HASHSIGHT_API_KEY"][..],
        ),
        (
            "custom_openai_compatible",
            "user",
            "provider:user/{profile}",
            &["responses", "chat_completions"][..],
            &[][..],
        ),
    ];
    for (id, source, provider_id, protocols, credentials) in expected {
        let profile = manifest
            .profile_classes
            .iter()
            .find(|profile| profile.id == id)
            .ok_or(BaselineError::ProviderEvidence)?;
        if profile.source != source
            || profile.provider_profile_id != provider_id
            || profile
                .protocols
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                != protocols
            || profile
                .credential_bindings
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                != credentials
        {
            return Err(BaselineError::ProviderEvidence);
        }
    }

    let official = resolve_config(None, None, &BTreeMap::new(), ConfigLayer::default())
        .map_err(|_| BaselineError::ProviderEvidence)?;
    if official.provider_id.as_str() != "minimax-official"
        || official.endpoint != "https://api.minimax.io/v1"
        || official.protocol != ProviderProtocolKind::Responses
        || official.environment_key != "MINIMAX_API_KEY"
    {
        return Err(BaselineError::ProviderEvidence);
    }
    let hashsight = resolve_config(
        None,
        None,
        &BTreeMap::new(),
        ConfigLayer {
            provider_id: Some("minimax-hashsight".to_owned()),
            endpoint: Some("https://www.hashsight.cn/v1".to_owned()),
            protocol: Some(ProviderProtocolKind::ChatCompletions),
            environment_key: Some("HASHSIGHT_API_KEY".to_owned()),
            ..ConfigLayer::default()
        },
    )
    .map_err(|_| BaselineError::ProviderEvidence)?;
    if hashsight.provider_id.as_str() != "minimax-hashsight"
        || hashsight.endpoint != "https://www.hashsight.cn/v1"
        || hashsight.protocol != ProviderProtocolKind::ChatCompletions
        || hashsight.environment_key != "HASHSIGHT_API_KEY"
    {
        return Err(BaselineError::ProviderEvidence);
    }
    let custom = resolve_config(
        None,
        None,
        &BTreeMap::new(),
        ConfigLayer {
            provider_id: Some("custom-fixture".to_owned()),
            endpoint: Some("https://example.test/v1".to_owned()),
            protocol: Some(ProviderProtocolKind::Responses),
            environment_key: Some("CUSTOM_API_KEY".to_owned()),
            ..ConfigLayer::default()
        },
    )
    .map_err(|_| BaselineError::ProviderEvidence)?;
    if custom.provider_id.as_str() != "custom-fixture"
        || custom.endpoint != "https://example.test/v1"
        || custom.protocol != ProviderProtocolKind::Responses
        || custom.environment_key != "CUSTOM_API_KEY"
    {
        return Err(BaselineError::ProviderEvidence);
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HostedReleaseGate {
    schema_version: u16,
    evidence_class: String,
    workflow: String,
    run_id: u64,
    run_url: String,
    branch: String,
    head_sha: String,
    tree_sha: String,
    product_fingerprint: String,
    product_file_count: u64,
    conclusion: String,
    jobs: Vec<HostedJob>,
    licenses: HostedLicenses,
    security: HostedSecurity,
    offline: bool,
    provider_calls: u64,
    credentials_read: u64,
    model_downloads: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HostedJob {
    job_id: u64,
    platform: String,
    conclusion: String,
    environment: HostedEnvironment,
    package: HostedPackage,
    performance: HostedPerformance,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HostedEnvironment {
    os: String,
    os_release: String,
    architecture: String,
    cpu_model: String,
    logical_cpu_count: u64,
    node: String,
    rustc_release: String,
    rustc_host: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HostedPackage {
    archive_sha256: String,
    binary_sha256: String,
    compressed_bytes: u64,
    embedding_included: bool,
    support_tier: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HostedPerformance {
    cold_start_samples_ms: Vec<f64>,
    cold_start_p95_ms: f64,
    idle_rss_samples_bytes: Vec<u64>,
    idle_rss_maximum_bytes: u64,
    wiki_bm25_p95_ms: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HostedLicenses {
    packages_checked: u64,
    invalid: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HostedSecurity {
    unsafe_files: u64,
    unsafe_workspace_lint: String,
    database_packages: u64,
    migration_network_or_credential_paths: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReleaseThresholds {
    schema_version: u16,
    cold_start_ms: f64,
    idle_rss_bytes: u64,
    base_compressed_bytes: u64,
    wiki_bm25_p95_ms: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CommandDifferenceFixture {
    schema_version: u16,
    differences: Vec<CommandDifference>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CommandDifference {
    id: String,
    command: String,
    locked_outcome: String,
    rust_behavior: String,
    reason: String,
    safety: String,
}

pub fn validate_cutover_evidence(
    root: &Path,
    public_contract: &PublicContractManifest,
) -> Result<(), BaselineError> {
    validate_hosted_release_gate(root)?;
    validate_cutover_candidate(root, public_contract)
}

pub fn validate_cutover_candidate(
    root: &Path,
    public_contract: &PublicContractManifest,
) -> Result<(), BaselineError> {
    validate_command_behavior_evidence(root, public_contract)?;
    if public_contract
        .items
        .iter()
        .any(|item| item.status == ParityStatus::Pending)
    {
        return Err(BaselineError::CutoverEvidence);
    }
    for required in [
        "contract.provider_profile.minimax_official",
        "contract.provider_profile.minimax_hashsight",
        "contract.provider_profile.custom_openai_compatible",
        "contract.migration",
        "contract.release_gate",
        "contract.product_entry",
    ] {
        let item = public_contract
            .items
            .iter()
            .find(|item| item.id == required)
            .ok_or(BaselineError::CutoverEvidence)?;
        if item.status != ParityStatus::Matched
            || item.evidence.is_empty()
            || item.evidence.iter().any(|path| !root.join(path).is_file())
        {
            return Err(BaselineError::CutoverEvidence);
        }
    }
    Ok(())
}

fn validate_command_behavior_evidence(
    root: &Path,
    public_contract: &PublicContractManifest,
) -> Result<(), BaselineError> {
    let raw = std::fs::read_to_string(root.join("fixtures/compat/command-differences.v1.json"))
        .map_err(|_| BaselineError::CutoverEvidence)?;
    let fixture: CommandDifferenceFixture =
        serde_json::from_str(&raw).map_err(|_| BaselineError::CutoverEvidence)?;
    let expected = ["/api", "/interrupt", "/models", "/provider", "/retry"];
    let expected_ids = BTreeSet::from([
        "difference.command.api",
        "difference.command.interrupt",
        "difference.command.models",
        "difference.command.provider",
        "difference.command.retry",
    ]);
    let mut actual = fixture
        .differences
        .iter()
        .map(|difference| difference.command.as_str())
        .collect::<Vec<_>>();
    actual.sort_unstable();
    let actual_ids = fixture
        .differences
        .iter()
        .map(|difference| difference.id.as_str())
        .collect::<BTreeSet<_>>();
    let contract_links = public_contract
        .items
        .iter()
        .filter_map(|item| item.approved_difference.as_deref())
        .collect::<BTreeSet<_>>();
    if fixture.schema_version != 1
        || actual != expected
        || actual_ids != expected_ids
        || contract_links != expected_ids
        || fixture.differences.iter().any(|difference| {
            difference.id.trim().is_empty()
                || difference.locked_outcome.trim().is_empty()
                || difference.rust_behavior.len() < 24
                || difference.reason.len() < 24
                || difference.safety.len() < 24
        })
        || !root
            .join("crates/cli/tests/discovery_commands.rs")
            .is_file()
        || !root.join("crates/cli/tests/restart.rs").is_file()
    {
        return Err(BaselineError::CutoverEvidence);
    }
    Ok(())
}

fn validate_hosted_release_gate(root: &Path) -> Result<(), BaselineError> {
    let gate: HostedReleaseGate = serde_json::from_str(
        &std::fs::read_to_string(root.join("fixtures/compat/release/hosted-gates.v1.json"))
            .map_err(|_| BaselineError::CutoverEvidence)?,
    )
    .map_err(|_| BaselineError::CutoverEvidence)?;
    let thresholds: ReleaseThresholds = serde_json::from_str(
        &std::fs::read_to_string(root.join("fixtures/compat/release/thresholds.v1.json"))
            .map_err(|_| BaselineError::CutoverEvidence)?,
    )
    .map_err(|_| BaselineError::CutoverEvidence)?;
    let (product_fingerprint, product_file_count) = compute_product_fingerprint(root)?;
    if gate.schema_version != 1
        || gate.evidence_class != "hosted_release_gate"
        || gate.workflow != "CI"
        || gate.run_id == 0
        || gate.run_url
            != format!(
                "https://github.com/niuniu122/wiki-coding/actions/runs/{}",
                gate.run_id
            )
        || gate.branch != "main"
        || !valid_hex(&gate.head_sha, 40)
        || !valid_hex(&gate.tree_sha, 40)
        || !valid_hex(&gate.product_fingerprint, 64)
        || gate.product_fingerprint != product_fingerprint
        || gate.product_file_count != product_file_count
        || gate.conclusion != "success"
        || gate.jobs.len() != 2
        || gate.licenses.packages_checked == 0
        || gate.licenses.invalid != 0
        || gate.security.unsafe_files != 0
        || gate.security.unsafe_workspace_lint != "forbid"
        || gate.security.database_packages != 0
        || gate.security.migration_network_or_credential_paths != 0
        || !gate.offline
        || gate.provider_calls != 0
        || gate.credentials_read != 0
        || gate.model_downloads != 0
        || thresholds.schema_version != 1
    {
        return Err(BaselineError::CutoverEvidence);
    }
    let mut platforms = BTreeMap::new();
    for job in &gate.jobs {
        if platforms
            .insert(job.platform.as_str(), job.job_id)
            .is_some()
            || job.job_id == 0
            || job.conclusion != "success"
            || job.environment.architecture != "x64"
            || job.environment.os_release.trim().is_empty()
            || job.environment.cpu_model.trim().is_empty()
            || job.environment.logical_cpu_count == 0
            || !job.environment.node.starts_with('v')
            || job.environment.rustc_release != "1.97.0"
            || !valid_hex(&job.package.archive_sha256, 64)
            || !valid_hex(&job.package.binary_sha256, 64)
            || job.package.compressed_bytes > thresholds.base_compressed_bytes
            || job.package.embedding_included
            || job.package.support_tier != "hosted_release"
            || job.performance.cold_start_samples_ms.len() != 9
            || job.performance.idle_rss_samples_bytes.len() != 5
            || !job
                .performance
                .cold_start_samples_ms
                .iter()
                .all(|sample| sample.is_finite() && *sample > 0.0)
            || job.performance.cold_start_p95_ms > thresholds.cold_start_ms
            || job.performance.idle_rss_maximum_bytes > thresholds.idle_rss_bytes
            || job.performance.wiki_bm25_p95_ms > thresholds.wiki_bm25_p95_ms
            || job.performance.idle_rss_samples_bytes.iter().max().copied()
                != Some(job.performance.idle_rss_maximum_bytes)
            || job
                .performance
                .cold_start_samples_ms
                .iter()
                .copied()
                .reduce(f64::max)
                != Some(job.performance.cold_start_p95_ms)
        {
            return Err(BaselineError::CutoverEvidence);
        }
        let valid_environment = match job.platform.as_str() {
            "windows-x86_64-msvc" => {
                job.environment.os == "win32"
                    && job.environment.rustc_host == "x86_64-pc-windows-msvc"
            }
            "linux-x86_64-gnu" => {
                job.environment.os == "linux"
                    && job.environment.rustc_host == "x86_64-unknown-linux-gnu"
            }
            _ => false,
        };
        if !valid_environment {
            return Err(BaselineError::CutoverEvidence);
        }
    }
    if !platforms.contains_key("windows-x86_64-msvc") || !platforms.contains_key("linux-x86_64-gnu")
    {
        return Err(BaselineError::CutoverEvidence);
    }
    Ok(())
}

fn compute_product_fingerprint(root: &Path) -> Result<(String, u64), BaselineError> {
    let tracked = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "-s", "-z", "--cached"])
        .output()
        .map_err(|_| BaselineError::CutoverEvidence)?;
    let untracked = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "-z", "--others", "--exclude-standard"])
        .output()
        .map_err(|_| BaselineError::CutoverEvidence)?;
    if !tracked.status.success() || !untracked.status.success() {
        return Err(BaselineError::CutoverEvidence);
    }
    let mut inputs = BTreeMap::new();
    for record in tracked
        .stdout
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
    {
        let record = std::str::from_utf8(record).map_err(|_| BaselineError::CutoverEvidence)?;
        let (metadata, path) = record
            .split_once('\t')
            .ok_or(BaselineError::CutoverEvidence)?;
        let fields = metadata.split_whitespace().collect::<Vec<_>>();
        if fields.len() != 3
            || !matches!(fields[0], "100644" | "100755")
            || !valid_hex(fields[1], 40)
            || fields[2] != "0"
        {
            return Err(BaselineError::CutoverEvidence);
        }
        if !excluded_product_path(path) {
            inputs.insert(path.to_owned(), format!("{}:{}", fields[0], fields[1]));
        }
    }
    for path in untracked
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
    {
        let path = std::str::from_utf8(path).map_err(|_| BaselineError::CutoverEvidence)?;
        if excluded_product_path(path) {
            continue;
        }
        let absolute = root.join(path);
        let metadata =
            std::fs::symlink_metadata(&absolute).map_err(|_| BaselineError::CutoverEvidence)?;
        if !metadata.is_file() || metadata.file_type().is_symlink() || inputs.contains_key(path) {
            return Err(BaselineError::CutoverEvidence);
        }
        let bytes = std::fs::read(absolute).map_err(|_| BaselineError::CutoverEvidence)?;
        let content = Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        inputs.insert(path.to_owned(), format!("untracked:{content}"));
    }

    let mut fingerprint = Sha256::new();
    fingerprint.update(b"minimax-codex-product-v2\0");
    for (path, content_identity) in &inputs {
        fingerprint.update(path.as_bytes());
        fingerprint.update([0]);
        fingerprint.update(content_identity.as_bytes());
    }
    let encoded = fingerprint
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok((encoded, inputs.len() as u64))
}

fn excluded_product_path(path: &str) -> bool {
    path == "fixtures/compat/release/hosted-gates.v1.json" || path.starts_with(".planning/")
}

fn valid_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
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
    let bins = package
        .get("bin")
        .and_then(serde_json::Value::as_object)
        .ok_or(BaselineError::ProductEntry)?;
    validate_package_product_scripts(&package).map_err(|_| BaselineError::ProductEntry)?;
    if bins.len() != 1
        || bins
            .get("minimax-codex")
            .and_then(serde_json::Value::as_str)
            != Some("bin/minimax-codex.cjs")
    {
        return Err(BaselineError::ProductEntry);
    }
    let lock_raw = std::fs::read_to_string(root.join("package-lock.json"))
        .map_err(|_| BaselineError::PackageRead)?;
    let lock: serde_json::Value =
        serde_json::from_str(&lock_raw).map_err(|_| BaselineError::PackageParse)?;
    let lock_packages = lock
        .get("packages")
        .and_then(serde_json::Value::as_object)
        .ok_or(BaselineError::ProductEntry)?;
    let lock_root = lock_packages
        .get("")
        .and_then(serde_json::Value::as_object)
        .ok_or(BaselineError::ProductEntry)?;
    if lock
        .get("lockfileVersion")
        .and_then(serde_json::Value::as_u64)
        != Some(3)
        || lock_packages.len() != 1
        || lock_root.get("name") != package.get("name")
        || lock_root.get("version") != package.get("version")
        || lock_root.get("bin") != package.get("bin")
        || lock_root.get("engines") != package.get("engines")
        || ["dependencies", "devDependencies", "optionalDependencies"]
            .iter()
            .any(|key| lock_root.contains_key(*key))
    {
        return Err(BaselineError::ProductEntry);
    }
    let launcher = std::fs::read_to_string(root.join("bin/minimax-codex.cjs"))
        .map_err(|_| BaselineError::ProductEntry)?;
    for required in [
        "\"win32:x64\": \"minimax-codex.exe\"",
        "\"linux:x64\": \"minimax-codex\"",
        "spawnSync(binaryPath, process.argv.slice(2)",
        "shell: false",
        "lstatSync",
        "isSymbolicLink",
        "Reinstall minimax-codex for a supported Windows x64 or Linux x64 release.",
        "could not start",
        "ended by signal",
    ] {
        if !launcher.contains(required) {
            return Err(BaselineError::ProductEntry);
        }
    }
    for forbidden in [
        "http://",
        "https://",
        "fetch(",
        "execSync(",
        "process.env",
        "dist/cli",
        "minimax-codex-legacy",
        "src/cli.tsx",
    ] {
        if launcher.contains(forbidden) {
            return Err(BaselineError::ProductEntry);
        }
    }
    Ok(())
}
