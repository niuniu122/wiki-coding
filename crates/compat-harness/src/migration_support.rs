use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs::File;
use std::io::Read as _;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;
use sha2::{Digest as _, Sha256};

const FIXTURE_RELATIVE: &str = "fixtures/compat/migration/typescript-v1";
const MANIFEST_NAME: &str = "manifest.v1.json";
const SUPPORT_WINDOW_NAME: &str = "support-window.v1.json";
const FIXTURE_VERSION: &str = "typescript-v1";
const CUTOVER_RELEASE: &str = "3.0.0";
const MAX_METADATA_BYTES: u64 = 1024 * 1024;
const METADATA_FILES: [&str; 2] = [MANIFEST_NAME, SUPPORT_WINDOW_NAME];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationSupportError(String);

impl fmt::Display for MigrationSupportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for MigrationSupportError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationSupportWindowStatus {
    pub cutover_release: String,
    pub minimum_subsequent_public_releases: usize,
    pub observed_subsequent_public_releases: usize,
    pub removal_eligible: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MigrationFixtureManifest {
    schema_version: u16,
    fixture_version: String,
    provenance: FixtureProvenance,
    metadata_files_excluded_from_fingerprint: Vec<String>,
    fixture_fingerprint: String,
    files: Vec<MigrationFixtureFile>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FixtureProvenance {
    source_product: String,
    source_format: String,
    migration_authority: String,
    source_preserving: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MigrationFixtureFile {
    relative_path: String,
    byte_length: u64,
    sha256: String,
    role: String,
    expected_disposition: String,
    #[serde(default)]
    exclusion_reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MigrationSupportWindow {
    schema_version: u16,
    fixture_version: String,
    fixture_fingerprint: String,
    cutover_release: String,
    minimum_subsequent_public_releases: usize,
    observed_public_releases: Vec<String>,
    removal_eligible: bool,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct PublicRelease {
    major: u64,
    minor: u64,
    patch: u64,
}

struct ExpectedPolicy {
    role: &'static str,
    disposition: &'static str,
    exclusion_reason: Option<&'static str>,
}

pub fn validate_migration_fixture_manifest(
    repository_root: &Path,
) -> Result<(), MigrationSupportError> {
    load_validated_manifest(repository_root).map(|_| ())
}

pub fn validate_migration_support_window(
    repository_root: &Path,
) -> Result<MigrationSupportWindowStatus, MigrationSupportError> {
    let manifest = load_validated_manifest(repository_root)?;
    let fixture_root = fixture_root(repository_root)?;
    let support: MigrationSupportWindow = read_json(
        &fixture_root.join(SUPPORT_WINDOW_NAME),
        "migration support window",
    )?;
    if support.schema_version != 1
        || support.fixture_version != FIXTURE_VERSION
        || support.fixture_fingerprint != manifest.fixture_fingerprint
        || support.cutover_release != CUTOVER_RELEASE
        || support.minimum_subsequent_public_releases != 2
    {
        return invalid("migration support window identity or policy mismatch");
    }

    let cutover = parse_public_release(&support.cutover_release)?;
    let mut previous = cutover;
    let mut seen = BTreeSet::new();
    for release in &support.observed_public_releases {
        let parsed = parse_public_release(release)?;
        if parsed <= cutover || parsed <= previous || !seen.insert(parsed) {
            return invalid(
                "observed public releases must be distinct, ordered, and after the cutover",
            );
        }
        previous = parsed;
    }
    let computed_eligibility =
        support.observed_public_releases.len() >= support.minimum_subsequent_public_releases;
    if support.removal_eligible != computed_eligibility {
        return invalid("migration removal eligibility does not match release evidence");
    }

    Ok(MigrationSupportWindowStatus {
        cutover_release: support.cutover_release,
        minimum_subsequent_public_releases: support.minimum_subsequent_public_releases,
        observed_subsequent_public_releases: support.observed_public_releases.len(),
        removal_eligible: computed_eligibility,
    })
}

fn load_validated_manifest(
    repository_root: &Path,
) -> Result<MigrationFixtureManifest, MigrationSupportError> {
    let fixture_root = fixture_root(repository_root)?;
    let manifest: MigrationFixtureManifest = read_json(
        &fixture_root.join(MANIFEST_NAME),
        "migration fixture manifest",
    )?;
    if manifest.schema_version != 1
        || manifest.fixture_version != FIXTURE_VERSION
        || manifest.provenance.source_product != "minimax-codex-typescript-v1"
        || manifest.provenance.source_format != "typescript-era-durable-state"
        || manifest.provenance.migration_authority != "rust"
        || !manifest.provenance.source_preserving
        || manifest.metadata_files_excluded_from_fingerprint != METADATA_FILES
    {
        return invalid("migration fixture manifest identity or provenance mismatch");
    }
    validate_lower_hex(&manifest.fixture_fingerprint, "fixture fingerprint")?;
    if manifest.files.is_empty() {
        return invalid("migration fixture manifest must contain evidence files");
    }

    let mut recorded = BTreeMap::new();
    let mut previous = None::<&str>;
    for entry in &manifest.files {
        validate_relative_path(&entry.relative_path)?;
        if METADATA_FILES.contains(&entry.relative_path.as_str()) {
            return invalid("metadata files cannot fingerprint themselves");
        }
        if previous.is_some_and(|path| path >= entry.relative_path.as_str()) {
            return invalid("migration fixture entries must be unique and sorted");
        }
        previous = Some(&entry.relative_path);
        validate_lower_hex(&entry.sha256, "fixture file sha256")?;
        let policy = expected_policy(&entry.relative_path);
        if entry.role != policy.role
            || entry.expected_disposition != policy.disposition
            || entry.exclusion_reason.as_deref() != policy.exclusion_reason
        {
            return invalid(format!(
                "migration fixture policy mismatch: {}",
                entry.relative_path
            ));
        }
        recorded.insert(entry.relative_path.clone(), entry);
    }

    let discovered = collect_fixture_files(&fixture_root)?;
    let recorded_paths = recorded.keys().cloned().collect::<BTreeSet<_>>();
    let discovered_paths = discovered.keys().cloned().collect::<BTreeSet<_>>();
    if recorded_paths != discovered_paths {
        return invalid("migration fixture path inventory mismatch");
    }
    for (path, disk) in discovered {
        let entry = recorded
            .get(&path)
            .ok_or_else(|| MigrationSupportError(format!("missing fixture entry: {path}")))?;
        if entry.byte_length != disk.byte_length || entry.sha256 != disk.sha256 {
            return invalid(format!("migration fixture content drift: {path}"));
        }
    }

    let computed = fixture_fingerprint(&manifest);
    if manifest.fixture_fingerprint != computed {
        return invalid(format!(
            "migration fixture fingerprint mismatch: expected {computed}"
        ));
    }
    Ok(manifest)
}

struct DiscoveredFile {
    byte_length: u64,
    sha256: String,
}

fn collect_fixture_files(
    fixture_root: &Path,
) -> Result<BTreeMap<String, DiscoveredFile>, MigrationSupportError> {
    let mut files = BTreeMap::new();
    walk_fixture(fixture_root, fixture_root, &mut files)?;
    Ok(files)
}

fn walk_fixture(
    fixture_root: &Path,
    directory: &Path,
    files: &mut BTreeMap<String, DiscoveredFile>,
) -> Result<(), MigrationSupportError> {
    let mut entries = std::fs::read_dir(directory)
        .map_err(|_| MigrationSupportError("cannot read migration fixture directory".to_owned()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| MigrationSupportError("cannot enumerate migration fixtures".to_owned()))?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let metadata = std::fs::symlink_metadata(&path)
            .map_err(|_| MigrationSupportError("cannot inspect migration fixture".to_owned()))?;
        if metadata.file_type().is_symlink() {
            return invalid("symlinked migration fixture evidence is forbidden");
        }
        if metadata.is_dir() {
            walk_fixture(fixture_root, &path, files)?;
            continue;
        }
        if !metadata.is_file() {
            return invalid("non-file migration fixture evidence is forbidden");
        }
        let relative = relative_string(fixture_root, &path)?;
        if METADATA_FILES.contains(&relative.as_str()) {
            continue;
        }
        let sha256 = hash_file(&path)?;
        files.insert(
            relative,
            DiscoveredFile {
                byte_length: metadata.len(),
                sha256,
            },
        );
    }
    Ok(())
}

fn expected_policy(path: &str) -> ExpectedPolicy {
    let lower = path.to_ascii_lowercase();
    if lower == "config.json" {
        included("provider_configuration")
    } else if lower == "indexes/threads.json" {
        included("thread_index")
    } else if lower.starts_with("sessions/") && lower.ends_with(".jsonl") {
        included("session_journal")
    } else if lower.starts_with("turns/") && lower.ends_with(".jsonl") {
        included("turn_journal")
    } else if lower == "capability-snapshot.json"
        || (lower.starts_with("capabilities/") && lower.ends_with(".json"))
    {
        included("capability_metadata")
    } else if lower.contains("secret") || lower.contains("credential") || lower.contains("key") {
        excluded("secret_store", "secret_path")
    } else if lower.starts_with("traces/") {
        excluded("private_reasoning_trace", "private_trace")
    } else if lower.starts_with("summaries/") {
        excluded("derived_summary", "derived_summary")
    } else if lower.starts_with("indexes/") || lower.contains("cache") || lower.starts_with("db/") {
        excluded("derived_cache", "derived_data")
    } else if lower.starts_with("locks/") || lower.ends_with(".lock") {
        excluded("lock_state", "lock_state")
    } else {
        excluded("unsupported_evidence", "unsupported_path")
    }
}

const fn included(role: &'static str) -> ExpectedPolicy {
    ExpectedPolicy {
        role,
        disposition: "include",
        exclusion_reason: None,
    }
}

const fn excluded(role: &'static str, reason: &'static str) -> ExpectedPolicy {
    ExpectedPolicy {
        role,
        disposition: "exclude",
        exclusion_reason: Some(reason),
    }
}

fn fixture_fingerprint(manifest: &MigrationFixtureManifest) -> String {
    let mut canonical = format!(
        "schemaVersion={}\nfixtureVersion={}\nsourceProduct={}\nsourceFormat={}\nmigrationAuthority={}\nsourcePreserving={}\n",
        manifest.schema_version,
        manifest.fixture_version,
        manifest.provenance.source_product,
        manifest.provenance.source_format,
        manifest.provenance.migration_authority,
        manifest.provenance.source_preserving
    );
    for metadata in &manifest.metadata_files_excluded_from_fingerprint {
        canonical.push_str("metadata=");
        canonical.push_str(metadata);
        canonical.push('\n');
    }
    for entry in &manifest.files {
        canonical.push_str("file=");
        canonical.push_str(&entry.relative_path);
        canonical.push('\0');
        canonical.push_str(&entry.byte_length.to_string());
        canonical.push('\0');
        canonical.push_str(&entry.sha256);
        canonical.push('\0');
        canonical.push_str(&entry.role);
        canonical.push('\0');
        canonical.push_str(&entry.expected_disposition);
        canonical.push('\0');
        canonical.push_str(entry.exclusion_reason.as_deref().unwrap_or(""));
        canonical.push('\n');
    }
    sha256(canonical.as_bytes())
}

fn fixture_root(repository_root: &Path) -> Result<PathBuf, MigrationSupportError> {
    let root = repository_root.join(FIXTURE_RELATIVE);
    let metadata = std::fs::symlink_metadata(&root)
        .map_err(|_| MigrationSupportError("migration fixture root is missing".to_owned()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return invalid("migration fixture root must be a real directory");
    }
    Ok(root)
}

fn read_json<T: for<'de> Deserialize<'de>>(
    path: &Path,
    label: &str,
) -> Result<T, MigrationSupportError> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| MigrationSupportError(format!("{label} is missing")))?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() > MAX_METADATA_BYTES
    {
        return invalid(format!("{label} must be a bounded regular file"));
    }
    let bytes =
        std::fs::read(path).map_err(|_| MigrationSupportError(format!("cannot read {label}")))?;
    serde_json::from_slice(&bytes)
        .map_err(|_| MigrationSupportError(format!("invalid {label} JSON")))
}

fn hash_file(path: &Path) -> Result<String, MigrationSupportError> {
    let mut file = File::open(path)
        .map_err(|_| MigrationSupportError("cannot read migration fixture file".to_owned()))?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| MigrationSupportError("cannot hash migration fixture file".to_owned()))?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(hex_digest(digest.finalize().as_slice()))
}

fn sha256(bytes: &[u8]) -> String {
    hex_digest(Sha256::digest(bytes).as_slice())
}

fn hex_digest(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn validate_lower_hex(value: &str, label: &str) -> Result<(), MigrationSupportError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return invalid(format!("{label} must be lowercase SHA-256"));
    }
    Ok(())
}

fn validate_relative_path(path: &str) -> Result<(), MigrationSupportError> {
    let parsed = Path::new(path);
    if path.is_empty()
        || path.contains('\\')
        || parsed.is_absolute()
        || parsed
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return invalid("migration fixture path must be normalized and relative");
    }
    Ok(())
}

fn relative_string(root: &Path, path: &Path) -> Result<String, MigrationSupportError> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| MigrationSupportError("fixture path escaped fixture root".to_owned()))?;
    let segments = relative
        .components()
        .map(|component| match component {
            Component::Normal(segment) => segment
                .to_str()
                .map(str::to_owned)
                .ok_or_else(|| MigrationSupportError("fixture path is not UTF-8".to_owned())),
            _ => invalid("fixture path is not normalized"),
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(segments.join("/"))
}

fn parse_public_release(value: &str) -> Result<PublicRelease, MigrationSupportError> {
    let parts = value.split('.').collect::<Vec<_>>();
    if parts.len() != 3
        || parts
            .iter()
            .any(|part| part.is_empty() || (part.len() > 1 && part.starts_with('0')))
    {
        return invalid("public release must be a strict major.minor.patch version");
    }
    let numbers = parts
        .into_iter()
        .map(|part| {
            part.parse::<u64>()
                .map_err(|_| MigrationSupportError("invalid public release version".to_owned()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(PublicRelease {
        major: numbers[0],
        minor: numbers[1],
        patch: numbers[2],
    })
}

fn invalid<T>(message: impl Into<String>) -> Result<T, MigrationSupportError> {
    Err(MigrationSupportError(message.into()))
}
