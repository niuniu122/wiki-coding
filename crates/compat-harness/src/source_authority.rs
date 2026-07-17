use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::path::{Component, Path};

use serde::Deserialize;
use sha2::{Digest, Sha256};

pub const SOURCE_AUTHORITY_MANIFEST: &str = "fixtures/compat/source-authority.v1.json";
pub const LEGACY_FIXTURE_PHASE_11_DISPOSITION: &str =
    "record-each-fixture-responsibility-in-typescript-responsibilities.v1.json";
pub const LEGACY_FIXTURE_PHASE_14_ZERO_CONTRACT: &str =
    "delete-all-entries-and-set-the-class-count-to-zero";

const FORBIDDEN_JAVASCRIPT_CAPABILITIES: [&str; 9] = [
    "fallback",
    "migration",
    "provider",
    "retrieval",
    "runtimeDownload",
    "session",
    "tool",
    "vault",
    "wiki",
];
const EXPECTED_RUST_ROOTS: [(&str, RustRootKind); 9] = [
    ("crates/cli", RustRootKind::Product),
    ("crates/compat-harness", RustRootKind::Verification),
    ("crates/core", RustRootKind::Product),
    ("crates/protocol", RustRootKind::Product),
    ("crates/provider", RustRootKind::Product),
    ("crates/retrieval", RustRootKind::Product),
    ("crates/tools", RustRootKind::Product),
    ("crates/tui", RustRootKind::Product),
    ("crates/vault", RustRootKind::Product),
];
const EXPECTED_JAVASCRIPT: [(&str, &str, JavascriptPurpose); 5] = [
    (
        "npm-launcher",
        "bin/minimax-codex.cjs",
        JavascriptPurpose::RustBinaryLauncher,
    ),
    (
        "release-package",
        "scripts/release/package-rust.mjs",
        JavascriptPurpose::RustReleasePackaging,
    ),
    (
        "product-fingerprint",
        "scripts/release/product-fingerprint.mjs",
        JavascriptPurpose::ProductFingerprinting,
    ),
    (
        "milestone-verification",
        "scripts/release/verify-milestone-flow.mjs",
        JavascriptPurpose::MilestoneVerification,
    ),
    (
        "release-verification",
        "scripts/release/verify-rust-release.mjs",
        JavascriptPurpose::RustReleaseVerification,
    ),
];
const EXPECTED_LEGACY_FIXTURES: [&str; 3] = [
    "test/fixtures/executors/diag-large.js",
    "test/fixtures/executors/diag-ok.js",
    "test/fixtures/executors/diag-slow.js",
];
const EXPECTED_IMMUTABLE_FIXTURE_ROOTS: [&str; 7] = [
    "fixtures/compat/migration",
    "fixtures/compat/provider-streams",
    "fixtures/compat/release",
    "fixtures/compat/retrieval",
    "fixtures/compat/tools",
    "fixtures/compat/vault",
    "fixtures/compat/wiki",
];
const EXPECTED_TARGETS: [(&str, &str, &str); 2] = [
    ("linux-x86_64-gnu", "linux", "x86_64-unknown-linux-gnu"),
    ("windows-x86_64-msvc", "windows", "x86_64-pc-windows-msvc"),
];

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceAuthorityManifest {
    pub schema_version: u16,
    pub rust_product_roots: Vec<RustProductRoot>,
    pub executable_entries: Vec<ExecutableEntry>,
    pub javascript_allowlist: Vec<JavascriptAuthority>,
    pub transitional_type_script: TransitionalTypeScript,
    pub transitional_legacy_test_fixtures: TransitionalLegacyTestFixtures,
    pub immutable_fixture_roots: Vec<ImmutableFixtureRoot>,
    pub supported_targets: Vec<SupportedTarget>,
    pub state_authority: StateAuthority,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RustProductRoot {
    pub path: String,
    pub kind: RustRootKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RustRootKind {
    Product,
    Verification,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutableEntry {
    pub command: String,
    pub package_manifest: String,
    pub javascript_entry_id: String,
    pub rust_binary: String,
    pub purpose: ExecutablePurpose,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExecutablePurpose {
    SupportedRustProductCommand,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JavascriptAuthority {
    pub id: String,
    pub path: String,
    pub sha256: String,
    pub purpose: JavascriptPurpose,
    pub forbidden_capabilities: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum JavascriptPurpose {
    RustBinaryLauncher,
    RustReleasePackaging,
    ProductFingerprinting,
    MilestoneVerification,
    RustReleaseVerification,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransitionalTypeScript {
    pub phase_14_zero_contract: String,
    pub entries: Vec<TransitionalTypeScriptEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransitionalTypeScriptEntry {
    pub path: String,
    pub sha256: String,
    pub purpose: TransitionalTypeScriptPurpose,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TransitionalTypeScriptPurpose {
    InertShrinkingEvidence,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransitionalLegacyTestFixtures {
    pub phase_11_disposition: String,
    pub phase_14_zero_contract: String,
    pub entries: Vec<TransitionalLegacyTestFixture>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransitionalLegacyTestFixture {
    pub path: String,
    pub sha256: String,
    pub purpose: LegacyFixturePurpose,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LegacyFixturePurpose {
    ExecutorDiagnosticFixture,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImmutableFixtureRoot {
    pub path: String,
    pub purpose: ImmutableFixturePurpose,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImmutableFixturePurpose {
    RustCompatibilityEvidence,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SupportedTarget {
    pub id: String,
    pub platform: String,
    pub rust_target: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StateAuthority {
    pub writable_roots: Vec<StateRoot>,
    pub migration_input_roots: Vec<StateRoot>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StateRoot {
    pub path: String,
    pub owner: StateOwner,
    pub access: StateAccess,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StateOwner {
    Rust,
    TypeScriptEra,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StateAccess {
    ReadWrite,
    ReadOnlyMigrationInput,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SourceAuthorityError {
    ManifestRead,
    ManifestParse(String),
    InvalidManifest(String),
    PathRead(String),
    HashDrift(String),
    Violation(String),
}

impl fmt::Display for SourceAuthorityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ManifestRead => formatter.write_str("cannot read source authority manifest"),
            Self::ManifestParse(message) => {
                write!(
                    formatter,
                    "invalid source authority manifest JSON: {message}"
                )
            }
            Self::InvalidManifest(message) => formatter.write_str(message),
            Self::PathRead(path) => write!(formatter, "cannot read authority path: {path}"),
            Self::HashDrift(path) => write!(formatter, "source authority hash drift: {path}"),
            Self::Violation(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for SourceAuthorityError {}

pub fn load_source_authority(root: &Path) -> Result<SourceAuthorityManifest, SourceAuthorityError> {
    let contents = fs::read_to_string(root.join(SOURCE_AUTHORITY_MANIFEST))
        .map_err(|_| SourceAuthorityError::ManifestRead)?;
    parse_source_authority(root, &contents)
}

pub fn validate_source_authority(
    root: &Path,
    manifest: &SourceAuthorityManifest,
) -> Result<(), SourceAuthorityError> {
    validate_manifest(root, manifest)?;
    validate_executable_links(root, manifest)?;

    let mut present_typescript = BTreeSet::new();
    let mut present_javascript = BTreeSet::new();
    collect_present_sources(root, root, &mut present_typescript, &mut present_javascript)?;

    let expected_typescript = manifest
        .transitional_type_script
        .entries
        .iter()
        .map(|entry| entry.path.clone())
        .collect::<BTreeSet<_>>();
    validate_inventory("TypeScript", &expected_typescript, &present_typescript)?;

    let allowlisted_javascript = manifest
        .javascript_allowlist
        .iter()
        .map(|entry| entry.path.clone())
        .collect::<BTreeSet<_>>();
    let legacy_javascript = manifest
        .transitional_legacy_test_fixtures
        .entries
        .iter()
        .map(|entry| entry.path.clone())
        .collect::<BTreeSet<_>>();
    let expected_javascript = allowlisted_javascript
        .union(&legacy_javascript)
        .cloned()
        .collect::<BTreeSet<_>>();
    validate_inventory("JavaScript", &expected_javascript, &present_javascript)?;

    if allowlisted_javascript
        .intersection(&legacy_javascript)
        .next()
        .is_some()
    {
        return violation("legacy JavaScript fixture entered executable JavaScript authority");
    }
    for entry in &manifest.javascript_allowlist {
        let contents = fs::read_to_string(root.join(&entry.path))
            .map_err(|_| SourceAuthorityError::PathRead(entry.path.clone()))?;
        validate_javascript_source_text(&entry.path, &contents)?;
    }
    Ok(())
}

fn validate_executable_links(
    root: &Path,
    manifest: &SourceAuthorityManifest,
) -> Result<(), SourceAuthorityError> {
    let package_contents = fs::read_to_string(root.join("package.json"))
        .map_err(|_| SourceAuthorityError::PathRead("package.json".to_owned()))?;
    let package: serde_json::Value = serde_json::from_str(&package_contents)
        .map_err(|_| SourceAuthorityError::Violation("package.json is invalid JSON".to_owned()))?;
    for executable in &manifest.executable_entries {
        let javascript = manifest
            .javascript_allowlist
            .iter()
            .find(|entry| entry.id == executable.javascript_entry_id)
            .ok_or_else(|| {
                SourceAuthorityError::Violation(format!(
                    "unknown executable JavaScript entry: {}",
                    executable.javascript_entry_id
                ))
            })?;
        let extension = Path::new(&javascript.path)
            .extension()
            .and_then(|value| value.to_str());
        if !matches!(extension, Some("cjs" | "mjs" | "js")) {
            return violation(format!(
                "unsupported executable extension: {}",
                javascript.path
            ));
        }
        let package_entry = package
            .get("bin")
            .and_then(|bins| bins.get(&executable.command))
            .and_then(serde_json::Value::as_str);
        if package_entry != Some(javascript.path.as_str()) {
            return violation(format!(
                "package executable entry does not match source authority: {}",
                executable.command
            ));
        }
    }
    Ok(())
}

fn collect_present_sources(
    root: &Path,
    directory: &Path,
    typescript: &mut BTreeSet<String>,
    javascript: &mut BTreeSet<String>,
) -> Result<(), SourceAuthorityError> {
    let mut entries = fs::read_dir(directory)
        .map_err(|_| SourceAuthorityError::PathRead(repository_path(root, directory)))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| SourceAuthorityError::PathRead(repository_path(root, directory)))?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let relative = repository_path(root, &path);
        let metadata = fs::symlink_metadata(&path)
            .map_err(|_| SourceAuthorityError::PathRead(relative.clone()))?;
        if metadata.is_dir() {
            if should_skip_directory(&relative) {
                continue;
            }
            if metadata.file_type().is_symlink() {
                return violation(format!(
                    "source inventory directory is a symlink: {relative}"
                ));
            }
            collect_present_sources(root, &path, typescript, javascript)?;
            continue;
        }
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .map(str::to_ascii_lowercase);
        if matches!(
            extension.as_deref(),
            Some("ts" | "tsx" | "js" | "cjs" | "mjs")
        ) {
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return violation(format!(
                    "source inventory path is not a regular file: {relative}"
                ));
            }
            if matches!(extension.as_deref(), Some("ts" | "tsx")) {
                typescript.insert(relative);
            } else {
                javascript.insert(relative);
            }
        }
    }
    Ok(())
}

fn should_skip_directory(relative: &str) -> bool {
    !relative.contains('/')
        && matches!(
            relative,
            ".git" | "dist" | "node_modules" | "target" | "coverage"
        )
}

fn repository_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn validate_inventory(
    class: &str,
    expected: &BTreeSet<String>,
    present: &BTreeSet<String>,
) -> Result<(), SourceAuthorityError> {
    if let Some(path) = present.difference(expected).next() {
        return violation(format!("unclassified {class} path: {path}"));
    }
    if let Some(path) = expected.difference(present).next() {
        return violation(format!("missing classified {class} path: {path}"));
    }
    Ok(())
}

pub fn validate_javascript_source_text(
    path: &str,
    source: &str,
) -> Result<(), SourceAuthorityError> {
    let normalized = source.replace('\\', "/");
    let lowercase = normalized.to_ascii_lowercase();

    for line in lowercase.lines() {
        let import_like = line.trim_start().starts_with("import ") || line.contains("require(");
        if import_like
            && ["../src/", "/src/", "src/", "../crates/", "/crates/"]
                .iter()
                .any(|pattern| line.contains(pattern))
        {
            return violation(format!("JavaScript product source import denied: {path}"));
        }
    }

    if contains_fallback_invocation(&lowercase) {
        return violation(format!("JavaScript fallback execution denied: {path}"));
    }

    if [
        "node:http",
        "node:https",
        "fetch(",
        "http.get(",
        "https.get(",
        "downloadruntime",
        "download_runtime",
        "runtimeurl",
        "runtime_url",
    ]
    .iter()
    .any(|pattern| lowercase.contains(pattern))
    {
        return violation(format!("JavaScript runtime download denied: {path}"));
    }

    for domain in [
        "provider",
        "retrieval",
        "session",
        "vault",
        "wiki",
        "tool",
        "migration",
    ] {
        if [
            format!("function {domain}"),
            format!("class {domain}"),
            format!("const {domain} ="),
            format!("let {domain} ="),
            format!("var {domain} ="),
        ]
        .iter()
        .any(|pattern| lowercase.contains(pattern))
        {
            return violation(format!(
                "JavaScript product-domain implementation denied: {path}"
            ));
        }
    }
    Ok(())
}

fn contains_fallback_invocation(source: &str) -> bool {
    const PROCESS_CALLS: [&str; 5] = [
        "spawnsync(",
        "spawn(",
        "execfile(",
        "execfilesync(",
        "execsync(",
    ];
    const FALLBACKS: [&str; 3] = ["dist/cli.js", "minimax-codex-legacy", "src/cli.tsx"];
    PROCESS_CALLS.iter().any(|call| {
        source.match_indices(call).any(|(start, _)| {
            let tail = &source[start..];
            let end = tail.find(");").map_or(tail.len(), |index| index + 2);
            FALLBACKS
                .iter()
                .any(|fallback| tail[..end].contains(fallback))
        })
    })
}

pub(crate) fn parse_source_authority(
    root: &Path,
    contents: &str,
) -> Result<SourceAuthorityManifest, SourceAuthorityError> {
    let manifest: SourceAuthorityManifest = serde_json::from_str(contents)
        .map_err(|error| SourceAuthorityError::ManifestParse(error.to_string()))?;
    validate_manifest(root, &manifest)?;
    Ok(manifest)
}

fn validate_manifest(
    root: &Path,
    manifest: &SourceAuthorityManifest,
) -> Result<(), SourceAuthorityError> {
    if manifest.schema_version != 1 {
        return invalid("source authority schemaVersion must be 1");
    }

    validate_exact_rust_roots(&manifest.rust_product_roots)?;
    validate_executable_entries(&manifest.executable_entries)?;
    validate_exact_javascript(&manifest.javascript_allowlist)?;
    validate_transitional_typescript(&manifest.transitional_type_script)?;
    validate_legacy_fixtures(&manifest.transitional_legacy_test_fixtures)?;
    validate_immutable_fixture_roots(&manifest.immutable_fixture_roots)?;
    validate_supported_targets(&manifest.supported_targets)?;
    validate_state_authority(&manifest.state_authority)?;
    validate_unique_authority_paths(manifest)?;

    for root_entry in &manifest.rust_product_roots {
        require_regular_directory(root, &root_entry.path)?;
    }
    require_regular_file(root, "package.json")?;
    for entry in &manifest.javascript_allowlist {
        validate_hash(root, &entry.path, &entry.sha256)?;
    }
    for entry in &manifest.transitional_type_script.entries {
        validate_hash(root, &entry.path, &entry.sha256)?;
    }
    for entry in &manifest.transitional_legacy_test_fixtures.entries {
        validate_hash(root, &entry.path, &entry.sha256)?;
    }
    for entry in &manifest.immutable_fixture_roots {
        require_regular_directory(root, &entry.path)?;
    }
    Ok(())
}

fn validate_exact_rust_roots(entries: &[RustProductRoot]) -> Result<(), SourceAuthorityError> {
    let actual = entries
        .iter()
        .map(|entry| (entry.path.as_str(), entry.kind))
        .collect::<Vec<_>>();
    if actual != EXPECTED_RUST_ROOTS {
        return invalid("rustProductRoots must enumerate the exact Cargo workspace roots");
    }
    Ok(())
}

fn validate_executable_entries(entries: &[ExecutableEntry]) -> Result<(), SourceAuthorityError> {
    let [entry] = entries else {
        return invalid("executableEntries must contain only the supported minimax-codex command");
    };
    if entry.command != "minimax-codex"
        || entry.package_manifest != "package.json"
        || entry.javascript_entry_id != "npm-launcher"
        || entry.rust_binary != "minimax-codex"
        || entry.purpose != ExecutablePurpose::SupportedRustProductCommand
    {
        return invalid("executableEntries contains an unknown executable entry");
    }
    validate_relative_path(&entry.package_manifest)?;
    Ok(())
}

fn validate_exact_javascript(entries: &[JavascriptAuthority]) -> Result<(), SourceAuthorityError> {
    if entries.len() != EXPECTED_JAVASCRIPT.len() {
        return invalid("javascriptAllowlist must contain the exact distribution allowlist");
    }
    for (entry, (id, path, purpose)) in entries.iter().zip(EXPECTED_JAVASCRIPT) {
        validate_relative_path(&entry.path)?;
        validate_hash_text(&entry.path, &entry.sha256)?;
        if entry.id != id || entry.path != path || entry.purpose != purpose {
            return invalid("javascriptAllowlist contains an unknown JavaScript authority path");
        }
        if entry.forbidden_capabilities != FORBIDDEN_JAVASCRIPT_CAPABILITIES.map(str::to_owned) {
            return invalid("JavaScript authority must forbid every product-domain capability");
        }
    }
    Ok(())
}

fn validate_transitional_typescript(
    transitional: &TransitionalTypeScript,
) -> Result<(), SourceAuthorityError> {
    if transitional.phase_14_zero_contract != "delete-all-entries-and-set-count-to-zero" {
        return invalid("transitionalTypeScript must declare the Phase 14 zero contract");
    }
    if transitional.entries.is_empty() {
        return invalid("transitionalTypeScript cannot be empty before Phase 14");
    }
    let mut previous = None;
    for entry in &transitional.entries {
        validate_relative_path(&entry.path)?;
        validate_hash_text(&entry.path, &entry.sha256)?;
        if !matches!(
            Path::new(&entry.path)
                .extension()
                .and_then(|value| value.to_str()),
            Some("ts" | "tsx")
        ) {
            return invalid("transitionalTypeScript contains a non-TypeScript path");
        }
        if entry.purpose != TransitionalTypeScriptPurpose::InertShrinkingEvidence {
            return invalid("transitional TypeScript must be inert shrinking evidence");
        }
        ensure_sorted(previous, &entry.path, "transitionalTypeScript")?;
        previous = Some(entry.path.as_str());
    }
    Ok(())
}

fn validate_legacy_fixtures(
    fixtures: &TransitionalLegacyTestFixtures,
) -> Result<(), SourceAuthorityError> {
    if fixtures.phase_11_disposition != LEGACY_FIXTURE_PHASE_11_DISPOSITION {
        return invalid("transitionalLegacyTestFixtures lacks the Phase 11 disposition");
    }
    if fixtures.phase_14_zero_contract != LEGACY_FIXTURE_PHASE_14_ZERO_CONTRACT {
        return invalid("transitionalLegacyTestFixtures lacks the Phase 14 zero contract");
    }
    let actual = fixtures
        .entries
        .iter()
        .map(|entry| entry.path.as_str())
        .collect::<Vec<_>>();
    if actual != EXPECTED_LEGACY_FIXTURES {
        return invalid("transitionalLegacyTestFixtures must contain only the three diag fixtures");
    }
    for entry in &fixtures.entries {
        validate_relative_path(&entry.path)?;
        validate_hash_text(&entry.path, &entry.sha256)?;
        if entry.purpose != LegacyFixturePurpose::ExecutorDiagnosticFixture {
            return invalid("legacy fixture purpose must be executorDiagnosticFixture");
        }
    }
    Ok(())
}

fn validate_immutable_fixture_roots(
    entries: &[ImmutableFixtureRoot],
) -> Result<(), SourceAuthorityError> {
    let actual = entries
        .iter()
        .map(|entry| entry.path.as_str())
        .collect::<Vec<_>>();
    if actual != EXPECTED_IMMUTABLE_FIXTURE_ROOTS {
        return invalid("immutableFixtureRoots must enumerate every compatibility fixture root");
    }
    for entry in entries {
        validate_relative_path(&entry.path)?;
        if entry.purpose != ImmutableFixturePurpose::RustCompatibilityEvidence {
            return invalid("immutable fixture root has an unsupported purpose");
        }
    }
    Ok(())
}

fn validate_supported_targets(entries: &[SupportedTarget]) -> Result<(), SourceAuthorityError> {
    let actual = entries
        .iter()
        .map(|entry| {
            (
                entry.id.as_str(),
                entry.platform.as_str(),
                entry.rust_target.as_str(),
            )
        })
        .collect::<Vec<_>>();
    if actual != EXPECTED_TARGETS {
        return invalid("supportedTargets must remain Windows x64 MSVC and Linux x64 GNU");
    }
    Ok(())
}

fn validate_state_authority(authority: &StateAuthority) -> Result<(), SourceAuthorityError> {
    let [writable] = authority.writable_roots.as_slice() else {
        return invalid("stateAuthority must declare exactly one writable root");
    };
    if writable.path != ".minimax"
        || writable.owner != StateOwner::Rust
        || writable.access != StateAccess::ReadWrite
    {
        return invalid("only the Rust-owned .minimax root may be writable");
    }
    let [migration_input] = authority.migration_input_roots.as_slice() else {
        return invalid("stateAuthority must declare exactly one migration input root");
    };
    if migration_input.path != ".mini-codex"
        || migration_input.owner != StateOwner::TypeScriptEra
        || migration_input.access != StateAccess::ReadOnlyMigrationInput
    {
        return invalid(".mini-codex must be a read-only TypeScript-era migration input");
    }
    validate_relative_path(&writable.path)?;
    validate_relative_path(&migration_input.path)?;
    Ok(())
}

fn validate_unique_authority_paths(
    manifest: &SourceAuthorityManifest,
) -> Result<(), SourceAuthorityError> {
    let mut paths = BTreeSet::new();
    let mut insert = |path: &str| {
        if paths.insert(path.to_owned()) {
            Ok(())
        } else {
            invalid(format!("duplicate authority path across classes: {path}"))
        }
    };
    for entry in &manifest.rust_product_roots {
        insert(&entry.path)?;
    }
    for entry in &manifest.executable_entries {
        insert(&entry.package_manifest)?;
    }
    for entry in &manifest.javascript_allowlist {
        insert(&entry.path)?;
    }
    for entry in &manifest.transitional_type_script.entries {
        insert(&entry.path)?;
    }
    for entry in &manifest.transitional_legacy_test_fixtures.entries {
        insert(&entry.path)?;
    }
    for entry in &manifest.immutable_fixture_roots {
        insert(&entry.path)?;
    }
    for entry in manifest
        .state_authority
        .writable_roots
        .iter()
        .chain(&manifest.state_authority.migration_input_roots)
    {
        insert(&entry.path)?;
    }
    Ok(())
}

fn validate_relative_path(path: &str) -> Result<(), SourceAuthorityError> {
    let parsed = Path::new(path);
    let unsafe_component = parsed.components().any(|component| {
        matches!(
            component,
            Component::Prefix(_) | Component::RootDir | Component::ParentDir | Component::CurDir
        )
    });
    if path.is_empty()
        || path.contains('\\')
        || path.ends_with('/')
        || parsed.is_absolute()
        || unsafe_component
    {
        return invalid(format!("unsafe repository-relative path: {path}"));
    }
    Ok(())
}

fn validate_hash_text(path: &str, hash: &str) -> Result<(), SourceAuthorityError> {
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return invalid(format!(
            "invalid lowercase SHA-256 for authority path: {path}"
        ));
    }
    Ok(())
}

fn ensure_sorted(
    previous: Option<&str>,
    current: &str,
    class: &str,
) -> Result<(), SourceAuthorityError> {
    if previous.is_some_and(|value| value >= current) {
        return invalid(format!("{class} paths must be sorted and duplicate-free"));
    }
    Ok(())
}

fn require_regular_file(root: &Path, path: &str) -> Result<(), SourceAuthorityError> {
    validate_relative_path(path)?;
    let metadata = fs::symlink_metadata(root.join(path))
        .map_err(|_| SourceAuthorityError::PathRead(path.to_owned()))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return invalid(format!("authority path must be a regular file: {path}"));
    }
    Ok(())
}

fn require_regular_directory(root: &Path, path: &str) -> Result<(), SourceAuthorityError> {
    validate_relative_path(path)?;
    let metadata = fs::symlink_metadata(root.join(path))
        .map_err(|_| SourceAuthorityError::PathRead(path.to_owned()))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return invalid(format!(
            "authority path must be a regular directory: {path}"
        ));
    }
    Ok(())
}

fn validate_hash(root: &Path, path: &str, expected: &str) -> Result<(), SourceAuthorityError> {
    require_regular_file(root, path)?;
    let bytes =
        fs::read(root.join(path)).map_err(|_| SourceAuthorityError::PathRead(path.to_owned()))?;
    let actual = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if actual != expected {
        return Err(SourceAuthorityError::HashDrift(path.to_owned()));
    }
    Ok(())
}

fn invalid<T>(message: impl Into<String>) -> Result<T, SourceAuthorityError> {
    Err(SourceAuthorityError::InvalidManifest(message.into()))
}

fn violation<T>(message: impl Into<String>) -> Result<T, SourceAuthorityError> {
    Err(SourceAuthorityError::Violation(message.into()))
}
