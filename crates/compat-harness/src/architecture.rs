use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::path::Path;
use std::process::Command;

use serde::Deserialize;

const COMPAT_HARNESS: &str = "minimax-compat-harness";
const CORE: &str = "minimax-core";
const PROTOCOL: &str = "minimax-protocol";
const CLI: &str = "minimax-cli";
const VAULT: &str = "minimax-vault";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchitecturePackage {
    pub name: String,
    pub local: bool,
    pub dependencies: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchitectureGraph {
    pub packages: Vec<ArchitecturePackage>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArchitectureError {
    MetadataCommand,
    MetadataParse,
    CoreSourceRead,
    Violation(String),
}

impl fmt::Display for ArchitectureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MetadataCommand => formatter.write_str("cargo metadata command failed"),
            Self::MetadataParse => formatter.write_str("cargo metadata returned invalid JSON"),
            Self::CoreSourceRead => formatter.write_str("cannot read minimax-core source boundary"),
            Self::Violation(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for ArchitectureError {}

#[derive(Deserialize)]
struct CargoMetadata {
    packages: Vec<CargoPackage>,
    workspace_members: Vec<String>,
    resolve: CargoResolve,
}

#[derive(Deserialize)]
struct CargoPackage {
    id: String,
    name: String,
}

#[derive(Deserialize)]
struct CargoResolve {
    nodes: Vec<CargoNode>,
}

#[derive(Deserialize)]
struct CargoNode {
    id: String,
    dependencies: Vec<String>,
}

pub fn load_cargo_architecture(root: &Path) -> Result<ArchitectureGraph, ArchitectureError> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let output = Command::new(cargo)
        .args([
            "metadata",
            "--locked",
            "--format-version",
            "1",
            "--manifest-path",
        ])
        .arg(root.join("Cargo.toml"))
        .current_dir(root)
        .output()
        .map_err(|_| ArchitectureError::MetadataCommand)?;
    if !output.status.success() {
        return Err(ArchitectureError::MetadataCommand);
    }
    let metadata: CargoMetadata =
        serde_json::from_slice(&output.stdout).map_err(|_| ArchitectureError::MetadataParse)?;
    Ok(graph_from_metadata(metadata))
}

pub fn validate_architecture(graph: &ArchitectureGraph) -> Result<(), ArchitectureError> {
    validate_database_denylist(graph)?;
    validate_core_dependencies(graph)?;
    validate_vault_dependencies(graph)?;

    let local_names = graph
        .packages
        .iter()
        .filter(|package| package.local)
        .map(|package| package.name.as_str())
        .collect::<BTreeSet<_>>();
    let local_dependencies = graph
        .packages
        .iter()
        .filter(|package| package.local)
        .map(|package| {
            let dependencies = package
                .dependencies
                .iter()
                .filter(|dependency| local_names.contains(dependency.as_str()))
                .cloned()
                .collect::<BTreeSet<_>>();
            (package.name.clone(), dependencies)
        })
        .collect::<BTreeMap<_, _>>();

    validate_no_cycle(&local_dependencies)?;

    for (package, dependencies) in &local_dependencies {
        for dependency in dependencies {
            if package != COMPAT_HARNESS && dependency == COMPAT_HARNESS {
                return Err(ArchitectureError::Violation(format!(
                    "production package must not depend on compat harness: {package} -> {dependency}"
                )));
            }
            if !dependency_allowed(package, dependency) {
                return Err(ArchitectureError::Violation(format!(
                    "forbidden local dependency: {package} -> {dependency}"
                )));
            }
        }
    }
    Ok(())
}

pub fn validate_core_source_boundary(root: &Path) -> Result<(), ArchitectureError> {
    validate_core_source_directory(&root.join("crates/core/src"))
}

pub fn validate_vault_source_boundary(root: &Path) -> Result<(), ArchitectureError> {
    validate_source_directory(&root.join("crates/vault/src"), validate_vault_source_text)
}

pub fn validate_cli_tui_markdown_boundary(root: &Path) -> Result<(), ArchitectureError> {
    for directory in [root.join("crates/cli/src"), root.join("crates/tui/src")] {
        validate_source_directory(&directory, validate_ui_source_text)?;
    }
    Ok(())
}

fn validate_source_directory(
    source_root: &Path,
    validator: fn(&str, &str) -> Result<(), ArchitectureError>,
) -> Result<(), ArchitectureError> {
    validate_source_directory_inner(source_root, source_root, validator)
}

fn validate_source_directory_inner(
    source_root: &Path,
    directory: &Path,
    validator: fn(&str, &str) -> Result<(), ArchitectureError>,
) -> Result<(), ArchitectureError> {
    for entry in fs::read_dir(directory).map_err(|_| ArchitectureError::CoreSourceRead)? {
        let entry = entry.map_err(|_| ArchitectureError::CoreSourceRead)?;
        let path = entry.path();
        if path.is_dir() {
            validate_source_directory_inner(source_root, &path, validator)?;
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
            let source =
                fs::read_to_string(&path).map_err(|_| ArchitectureError::CoreSourceRead)?;
            let relative = path.strip_prefix(source_root).unwrap_or(&path);
            validator(&relative.to_string_lossy(), &source)?;
        }
    }
    Ok(())
}

pub fn validate_core_source_directory(source_root: &Path) -> Result<(), ArchitectureError> {
    validate_core_source_directory_inner(source_root, source_root)
}

fn validate_core_source_directory_inner(
    source_root: &Path,
    directory: &Path,
) -> Result<(), ArchitectureError> {
    for entry in fs::read_dir(directory).map_err(|_| ArchitectureError::CoreSourceRead)? {
        let entry = entry.map_err(|_| ArchitectureError::CoreSourceRead)?;
        let path = entry.path();
        if path.is_dir() {
            validate_core_source_directory_inner(source_root, &path)?;
            continue;
        }
        if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
            let source =
                fs::read_to_string(&path).map_err(|_| ArchitectureError::CoreSourceRead)?;
            let relative = path.strip_prefix(source_root).unwrap_or(&path);
            validate_core_source_text(&relative.to_string_lossy(), &source)?;
        }
    }
    Ok(())
}

pub fn validate_core_source_text(file: &str, source: &str) -> Result<(), ArchitectureError> {
    const DENIED: [&str; 19] = [
        "std::path",
        "std::fs",
        "PathBuf",
        "Path::",
        ".md",
        "minimax_vault",
        "minimax_tools",
        "tokio::",
        "tokio_util",
        "reqwest",
        "hyper::",
        "http::",
        "crossterm",
        "keyring",
        "rusqlite",
        "sqlx",
        "diesel",
        "sea_orm",
        "seaorm",
    ];
    if let Some(pattern) = DENIED.iter().find(|pattern| source.contains(*pattern)) {
        return Err(ArchitectureError::Violation(format!(
            "core source boundary denied: {file} contains {pattern}"
        )));
    }
    Ok(())
}

pub fn validate_vault_source_text(file: &str, source: &str) -> Result<(), ArchitectureError> {
    const DENIED: [&str; 12] = [
        "minimax_provider",
        "ProviderPort",
        "reqwest",
        "hyper::",
        "http::",
        "rusqlite",
        "sqlx",
        "diesel",
        "sea_orm",
        "seaorm",
        "Authorization",
        "Bearer ",
    ];
    if let Some(pattern) = DENIED.iter().find(|pattern| source.contains(*pattern)) {
        return Err(ArchitectureError::Violation(format!(
            "vault source boundary denied: {file} contains {pattern}"
        )));
    }
    Ok(())
}

pub fn validate_ui_source_text(file: &str, source: &str) -> Result<(), ArchitectureError> {
    const DENIED: [&str; 5] = [
        "parse_wiki_page",
        "pulldown_cmark",
        "markdown::",
        "split_once(\"\\n---\\n\")",
        "FRONTMATTER_KEYS",
    ];
    if let Some(pattern) = DENIED.iter().find(|pattern| source.contains(*pattern)) {
        return Err(ArchitectureError::Violation(format!(
            "CLI/TUI Markdown boundary denied: {file} contains {pattern}"
        )));
    }
    Ok(())
}

fn graph_from_metadata(metadata: CargoMetadata) -> ArchitectureGraph {
    let names_by_id = metadata
        .packages
        .iter()
        .map(|package| (package.id.clone(), package.name.clone()))
        .collect::<BTreeMap<_, _>>();
    let workspace_members = metadata
        .workspace_members
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let dependencies_by_id = metadata
        .resolve
        .nodes
        .into_iter()
        .map(|node| (node.id, node.dependencies))
        .collect::<BTreeMap<_, _>>();
    let mut packages = metadata
        .packages
        .into_iter()
        .map(|package| {
            let dependencies = dependencies_by_id
                .get(&package.id)
                .into_iter()
                .flatten()
                .filter_map(|dependency| names_by_id.get(dependency))
                .cloned()
                .collect();
            ArchitecturePackage {
                local: workspace_members.contains(&package.id),
                name: package.name,
                dependencies,
            }
        })
        .collect::<Vec<_>>();
    packages.sort_by(|left, right| left.name.cmp(&right.name));
    ArchitectureGraph { packages }
}

fn dependency_allowed(package: &str, dependency: &str) -> bool {
    match package {
        PROTOCOL => false,
        CORE => dependency == PROTOCOL,
        "minimax-provider" | "minimax-tools" | "minimax-retrieval" | "minimax-vault"
        | "minimax-tui" => matches!(dependency, CORE | PROTOCOL),
        CLI => dependency != COMPAT_HARNESS,
        COMPAT_HARNESS => dependency != CLI && dependency != COMPAT_HARNESS,
        _ => false,
    }
}

fn validate_core_dependencies(graph: &ArchitectureGraph) -> Result<(), ArchitectureError> {
    const ALLOWED: [&str; 3] = [PROTOCOL, "serde", "serde_json"];
    if let Some(core) = graph
        .packages
        .iter()
        .find(|package| package.local && package.name == CORE)
    {
        for dependency in &core.dependencies {
            if !ALLOWED.contains(&dependency.as_str()) {
                return Err(ArchitectureError::Violation(format!(
                    "core dependency denied: {CORE} -> {dependency}"
                )));
            }
        }
    }
    Ok(())
}

fn validate_vault_dependencies(graph: &ArchitectureGraph) -> Result<(), ArchitectureError> {
    const ALLOWED: [&str; 8] = [
        CORE,
        PROTOCOL,
        "fs4",
        "serde",
        "serde_json",
        "sha2",
        "tempfile",
        "windows-sys",
    ];
    if let Some(vault) = graph
        .packages
        .iter()
        .find(|package| package.local && package.name == VAULT)
    {
        for dependency in &vault.dependencies {
            if !ALLOWED.contains(&dependency.as_str()) {
                return Err(ArchitectureError::Violation(format!(
                    "vault dependency denied: {VAULT} -> {dependency}"
                )));
            }
        }
    }
    Ok(())
}

fn validate_database_denylist(graph: &ArchitectureGraph) -> Result<(), ArchitectureError> {
    for package in &graph.packages {
        let normalized = package.name.to_ascii_lowercase().replace(['-', '_'], "");
        if normalized.contains("sqlite")
            || normalized.starts_with("sqlx")
            || normalized.starts_with("diesel")
            || normalized.contains("seaorm")
        {
            return Err(ArchitectureError::Violation(format!(
                "database dependency denied: {}",
                package.name
            )));
        }
    }
    Ok(())
}

fn validate_no_cycle(
    dependencies: &BTreeMap<String, BTreeSet<String>>,
) -> Result<(), ArchitectureError> {
    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    for package in dependencies.keys() {
        if let Some(cycle) = find_cycle(package, dependencies, &mut visiting, &mut visited) {
            return Err(ArchitectureError::Violation(format!(
                "local dependency cycle involving: {}",
                cycle.join(", ")
            )));
        }
    }
    Ok(())
}

fn find_cycle(
    package: &str,
    dependencies: &BTreeMap<String, BTreeSet<String>>,
    visiting: &mut BTreeSet<String>,
    visited: &mut BTreeSet<String>,
) -> Option<Vec<String>> {
    if visited.contains(package) {
        return None;
    }
    if !visiting.insert(package.to_owned()) {
        return Some(vec![package.to_owned()]);
    }
    for dependency in dependencies.get(package).into_iter().flatten() {
        if visiting.contains(dependency) {
            let mut cycle = visiting.iter().cloned().collect::<Vec<_>>();
            cycle.retain(|entry| entry == package || entry == dependency);
            cycle.sort();
            return Some(cycle);
        }
        if let Some(cycle) = find_cycle(dependency, dependencies, visiting, visited) {
            return Some(cycle);
        }
    }
    visiting.remove(package);
    visited.insert(package.to_owned());
    None
}
