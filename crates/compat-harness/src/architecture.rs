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
    let source_root = root.join("crates/core/src");
    for entry in fs::read_dir(source_root).map_err(|_| ArchitectureError::CoreSourceRead)? {
        let entry = entry.map_err(|_| ArchitectureError::CoreSourceRead)?;
        if entry
            .path()
            .extension()
            .and_then(|extension| extension.to_str())
            != Some("rs")
        {
            continue;
        }
        let source =
            fs::read_to_string(entry.path()).map_err(|_| ArchitectureError::CoreSourceRead)?;
        validate_core_source_text(&entry.file_name().to_string_lossy(), &source)?;
    }
    Ok(())
}

pub fn validate_core_source_text(file: &str, source: &str) -> Result<(), ArchitectureError> {
    const DENIED: [&str; 9] = [
        "std::path",
        "PathBuf",
        "Path::",
        ".md",
        "minimax_vault",
        "minimax_tools",
        "reqwest",
        "hyper::",
        "http::",
    ];
    if let Some(pattern) = DENIED.iter().find(|pattern| source.contains(*pattern)) {
        return Err(ArchitectureError::Violation(format!(
            "core source boundary denied: {file} contains {pattern}"
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

fn validate_database_denylist(graph: &ArchitectureGraph) -> Result<(), ArchitectureError> {
    const DENIED: [&str; 6] = ["sqlite", "sqlx", "diesel", "rusqlite", "orm", "libsqlite"];
    for package in &graph.packages {
        let normalized = package.name.to_ascii_lowercase().replace(['-', '_'], "");
        if DENIED.iter().any(|denied| normalized.contains(denied)) {
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
