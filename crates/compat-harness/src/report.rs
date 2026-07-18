use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::architecture::{
    load_cargo_architecture, validate_architecture, validate_cli_tui_markdown_boundary,
    validate_core_source_boundary, validate_migration_source_boundary,
    validate_retrieval_source_boundary, validate_vault_source_boundary,
};
use crate::baseline::{
    validate_cutover_candidate, validate_cutover_evidence, validate_cutover_strict_precondition,
    validate_product_entry, validate_rust_command_surface, validate_rust_provider_profiles,
    validate_rust_retrieval_evidence, validate_rust_tool_evidence, validate_rust_vault_evidence,
};
use crate::manifest::{CompatManifests, ManifestError, ParityStatus};
use crate::provider_eval::verify_provider_evaluation;
use crate::retrieval_eval::verify_retrieval_evaluation;

const REPORT_SCHEMA_VERSION: u16 = 1;
const DIFFERENCE_FIXTURE: &str = "fixtures/compat/command-differences.v1.json";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompatReport {
    pub schema_version: u16,
    pub contract_version: String,
    pub contract_fingerprint: String,
    pub entries: Vec<ReportEntry>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReportEntry {
    pub id: String,
    pub rust_status: ParityStatus,
    pub rust_evidence: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approved_difference: Option<ApprovedDifference>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApprovedDifference {
    pub id: String,
    pub command: String,
    pub locked_outcome: String,
    pub rust_behavior: String,
    pub reason: String,
    pub safety: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CommandDifferenceFixture {
    schema_version: u16,
    differences: Vec<ApprovedDifference>,
}

pub fn build_report(
    manifests: &CompatManifests,
    root: &Path,
) -> Result<CompatReport, ManifestError> {
    let differences = load_approved_differences(root, manifests)?;
    let mut entries = Vec::with_capacity(manifests.public_contract.items.len());
    for item in &manifests.public_contract.items {
        let mut rust_evidence = item.evidence.clone();
        rust_evidence.sort();
        let approved_difference = item
            .approved_difference
            .as_ref()
            .map(|id| {
                differences.get(id).cloned().ok_or_else(|| {
                    ManifestError::Validation(format!(
                        "public contract references unknown approved difference: {id}"
                    ))
                })
            })
            .transpose()?;
        entries.push(ReportEntry {
            id: item.id.clone(),
            rust_status: item.status.clone(),
            rust_evidence,
            approved_difference,
        });
    }
    entries.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(CompatReport {
        schema_version: REPORT_SCHEMA_VERSION,
        contract_version: manifests.public_contract.contract_version.clone(),
        contract_fingerprint: manifests.public_contract.content_fingerprint.clone(),
        entries,
    })
}

pub fn validate_report(
    report: &CompatReport,
    manifests: &CompatManifests,
    root: &Path,
) -> Result<(), ManifestError> {
    if report.schema_version != REPORT_SCHEMA_VERSION {
        return Err(ManifestError::Validation(
            "compatibility report schema version must be 1".to_owned(),
        ));
    }
    if report.contract_version != manifests.public_contract.contract_version {
        return Err(ManifestError::Validation(
            "compatibility report contract version mismatch".to_owned(),
        ));
    }
    if report.contract_fingerprint != manifests.public_contract.content_fingerprint {
        return Err(ManifestError::Validation(
            "compatibility report contract fingerprint mismatch".to_owned(),
        ));
    }

    let contract_items = manifests
        .public_contract
        .items
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect::<BTreeMap<_, _>>();
    let mut ids = BTreeSet::new();
    for entry in &report.entries {
        if !ids.insert(entry.id.as_str()) {
            return Err(ManifestError::Validation(format!(
                "duplicate compatibility report item: {}",
                entry.id
            )));
        }
        let item = contract_items.get(entry.id.as_str()).ok_or_else(|| {
            ManifestError::Validation(format!(
                "compatibility report contains a non-contract item: {}",
                entry.id
            ))
        })?;
        if entry.rust_status != item.status {
            return Err(ManifestError::Validation(format!(
                "compatibility report Rust status drift: {}",
                entry.id
            )));
        }
        if entry.rust_evidence.is_empty() {
            return Err(ManifestError::Validation(format!(
                "matched item requires evidence: {}",
                entry.id
            )));
        }
        for evidence in &entry.rust_evidence {
            if evidence.trim().is_empty() || !root.join(evidence).is_file() {
                return Err(ManifestError::Validation(format!(
                    "matched item references missing evidence: {} -> {}",
                    entry.id, evidence
                )));
            }
        }
        match entry.rust_status {
            ParityStatus::Matched if entry.approved_difference.is_some() => {
                return Err(ManifestError::Validation(format!(
                    "matched item cannot carry an approved difference: {}",
                    entry.id
                )));
            }
            ParityStatus::ApprovedDifference if entry.approved_difference.is_none() => {
                return Err(ManifestError::Validation(format!(
                    "approved difference is missing from report: {}",
                    entry.id
                )));
            }
            ParityStatus::Pending => {
                return Err(ManifestError::Validation(format!(
                    "public contract report cannot contain a pending item: {}",
                    entry.id
                )));
            }
            ParityStatus::Matched | ParityStatus::ApprovedDifference => {}
        }
    }
    if ids.len() != contract_items.len() {
        return Err(ManifestError::Validation(
            "compatibility report must contain every public contract item exactly once".to_owned(),
        ));
    }

    let expected = build_report(manifests, root)?;
    if report != &expected {
        return Err(ManifestError::Validation(
            "compatibility report differs from immutable contract and approved differences"
                .to_owned(),
        ));
    }
    Ok(())
}

pub fn report_json(report: &CompatReport) -> Result<String, ManifestError> {
    serde_json::to_string_pretty(report)
        .map(|json| format!("{json}\n"))
        .map_err(|_| ManifestError::Validation("cannot serialize compatibility report".to_owned()))
}

pub fn validate_compatibility_source_boundary(root: &Path) -> Result<(), ManifestError> {
    for (relative, source) in compatibility_module_sources(root)? {
        validate_executable_legacy_references(&relative, &source)?;
    }
    Ok(())
}

const COMPATIBILITY_SOURCE_ROOT: &str = "crates/compat-harness/src";
const COMPATIBILITY_ROOT_MODULES: [&str; 2] = ["lib.rs", "main.rs"];

#[derive(Clone, Debug, Eq, PartialEq)]
enum RustSourceToken {
    Identifier(String),
    StringLiteral(String),
    Punct(char),
}

fn compatibility_module_sources(root: &Path) -> Result<BTreeMap<String, String>, ManifestError> {
    let source_root = root.join(COMPATIBILITY_SOURCE_ROOT);
    let inventory = collect_rust_source_inventory(root, &source_root)?;
    let mut queue = VecDeque::new();
    for root_name in COMPATIBILITY_ROOT_MODULES {
        let relative = format!("{COMPATIBILITY_SOURCE_ROOT}/{root_name}");
        if !inventory.contains(&relative) {
            return validation(format!("compatibility root module is missing: {relative}"));
        }
        queue.push_back(root.join(&relative));
    }

    let mut sources = BTreeMap::new();
    while let Some(path) = queue.pop_front() {
        let relative = repository_relative_path(root, &path)?;
        if sources.contains_key(&relative) {
            continue;
        }
        require_regular_rust_source(root, &path, &relative)?;
        let source = fs::read_to_string(&path).map_err(|_| ManifestError::Read {
            file: relative.clone(),
        })?;
        let modules = external_module_declarations(&relative, &source)?;
        let module_directory = module_directory(&path)?;
        let mut declared = BTreeSet::new();
        for module in modules {
            if !declared.insert(module.clone()) {
                return validation(format!(
                    "duplicate compatibility module declaration: {relative} -> {module}"
                ));
            }
            queue.push_back(resolve_external_module(
                root,
                &module_directory,
                &relative,
                &module,
            )?);
        }
        sources.insert(relative, source);
    }

    let derived = sources.keys().cloned().collect::<BTreeSet<_>>();
    if let Some(path) = inventory.difference(&derived).next() {
        return validation(format!("orphan compatibility Rust source: {path}"));
    }
    if let Some(path) = derived.difference(&inventory).next() {
        return validation(format!(
            "derived compatibility module is absent from source inventory: {path}"
        ));
    }
    Ok(sources)
}

fn collect_rust_source_inventory(
    root: &Path,
    source_root: &Path,
) -> Result<BTreeSet<String>, ManifestError> {
    let relative = repository_relative_path(root, source_root)?;
    let metadata = fs::symlink_metadata(source_root).map_err(|_| ManifestError::Read {
        file: relative.clone(),
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return validation(format!(
            "compatibility source root must be a regular directory: {relative}"
        ));
    }
    let mut inventory = BTreeSet::new();
    walk_rust_source_inventory(root, source_root, &mut inventory)?;
    Ok(inventory)
}

fn walk_rust_source_inventory(
    root: &Path,
    directory: &Path,
    inventory: &mut BTreeSet<String>,
) -> Result<(), ManifestError> {
    let directory_relative = repository_relative_path(root, directory)?;
    let mut entries = fs::read_dir(directory)
        .map_err(|_| ManifestError::Read {
            file: directory_relative.clone(),
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| ManifestError::Read {
            file: directory_relative,
        })?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let relative = repository_relative_path(root, &path)?;
        let metadata = fs::symlink_metadata(&path).map_err(|_| ManifestError::Read {
            file: relative.clone(),
        })?;
        if metadata.file_type().is_symlink() {
            return validation(format!(
                "compatibility Rust source is symlinked: {relative}"
            ));
        }
        if metadata.is_dir() {
            walk_rust_source_inventory(root, &path, inventory)?;
        } else if metadata.is_file()
            && path.extension().and_then(|value| value.to_str()) == Some("rs")
        {
            inventory.insert(relative);
        } else if !metadata.is_file() {
            return validation(format!(
                "compatibility source inventory contains a non-regular path: {relative}"
            ));
        }
    }
    Ok(())
}

fn external_module_declarations(
    relative: &str,
    source: &str,
) -> Result<Vec<String>, ManifestError> {
    let tokens = tokenize_rust_source(relative, source)?;
    for window in tokens.windows(4) {
        if matches!(
            window,
            [
                RustSourceToken::Punct('#'),
                RustSourceToken::Punct('['),
                RustSourceToken::Identifier(attribute),
                RustSourceToken::Punct('=')
            ] if attribute == "path"
        ) {
            return validation(format!(
                "compatibility module path attributes are forbidden: {relative}"
            ));
        }
    }

    let mut declarations = Vec::new();
    let mut index = 0;
    while index + 2 < tokens.len() {
        if matches!(&tokens[index], RustSourceToken::Identifier(value) if value == "mod") {
            let Some(RustSourceToken::Identifier(module)) = tokens.get(index + 1) else {
                return validation(format!(
                    "invalid compatibility module declaration: {relative}"
                ));
            };
            match tokens.get(index + 2) {
                Some(RustSourceToken::Punct(';')) => declarations.push(module.clone()),
                Some(RustSourceToken::Punct('{')) => {}
                _ => {
                    return validation(format!(
                        "invalid compatibility module declaration: {relative} -> {module}"
                    ));
                }
            }
        }
        index += 1;
    }
    Ok(declarations)
}

fn module_directory(path: &Path) -> Result<PathBuf, ManifestError> {
    let parent = path.parent().ok_or_else(|| {
        ManifestError::Validation("compatibility module has no parent directory".to_owned())
    })?;
    let file_name = path.file_name().and_then(|value| value.to_str());
    if matches!(file_name, Some("lib.rs" | "main.rs" | "mod.rs")) {
        Ok(parent.to_path_buf())
    } else {
        let stem = path.file_stem().ok_or_else(|| {
            ManifestError::Validation("compatibility module has no file stem".to_owned())
        })?;
        Ok(parent.join(stem))
    }
}

fn resolve_external_module(
    root: &Path,
    module_directory: &Path,
    importer: &str,
    module: &str,
) -> Result<PathBuf, ManifestError> {
    if module.is_empty()
        || !module.bytes().enumerate().all(|(index, byte)| {
            byte == b'_' || byte.is_ascii_alphanumeric() && (index > 0 || !byte.is_ascii_digit())
        })
    {
        return validation(format!(
            "unsafe compatibility module name: {importer} -> {module}"
        ));
    }
    let candidates = [
        module_directory.join(format!("{module}.rs")),
        module_directory.join(module).join("mod.rs"),
    ];
    let mut matches = Vec::new();
    for candidate in candidates {
        let relative = repository_relative_path(root, &candidate)?;
        match fs::symlink_metadata(&candidate) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return validation(format!(
                    "compatibility Rust source is symlinked: {relative}"
                ));
            }
            Ok(metadata) if metadata.is_file() => matches.push(candidate),
            Ok(_) => {
                return validation(format!(
                    "compatibility module path is not a regular file: {relative}"
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(ManifestError::Read { file: relative }),
        }
    }
    match matches.as_slice() {
        [path] => Ok(path.clone()),
        [] => validation(format!(
            "unresolved compatibility module: {importer} -> {module}"
        )),
        _ => validation(format!(
            "ambiguous compatibility module: {importer} -> {module}"
        )),
    }
}

fn require_regular_rust_source(
    root: &Path,
    path: &Path,
    relative: &str,
) -> Result<(), ManifestError> {
    ensure_no_compatibility_symlink_components(root, path)?;
    let metadata = fs::symlink_metadata(path).map_err(|_| ManifestError::Read {
        file: relative.to_owned(),
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return validation(format!(
            "compatibility module must be a regular file: {relative}"
        ));
    }
    Ok(())
}

fn ensure_no_compatibility_symlink_components(
    root: &Path,
    path: &Path,
) -> Result<(), ManifestError> {
    let relative = path.strip_prefix(root).map_err(|_| {
        ManifestError::Validation(format!(
            "compatibility module escaped repository: {}",
            path.display()
        ))
    })?;
    let mut cursor = root.to_path_buf();
    for component in relative.components() {
        if !matches!(component, Component::Normal(_)) {
            return validation(format!(
                "unsafe compatibility module path: {}",
                path.display()
            ));
        }
        cursor.push(component.as_os_str());
        match fs::symlink_metadata(&cursor) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return validation(format!(
                    "compatibility Rust source is symlinked: {}",
                    repository_relative_path(root, &cursor)?
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(_) => {
                return Err(ManifestError::Read {
                    file: repository_relative_path(root, &cursor)?,
                });
            }
        }
    }
    Ok(())
}

fn repository_relative_path(root: &Path, path: &Path) -> Result<String, ManifestError> {
    let relative = path.strip_prefix(root).map_err(|_| {
        ManifestError::Validation(format!(
            "compatibility source escaped repository: {}",
            path.display()
        ))
    })?;
    let segments = relative
        .components()
        .map(|component| match component {
            Component::Normal(segment) => segment.to_str().map(str::to_owned).ok_or_else(|| {
                ManifestError::Validation("compatibility source path is not UTF-8".to_owned())
            }),
            _ => validation("compatibility source path is not normalized"),
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(segments.join("/"))
}

fn validate_executable_legacy_references(
    relative: &str,
    source: &str,
) -> Result<(), ManifestError> {
    let tokens = tokenize_rust_source(relative, source)?;
    let constants = collect_string_constants(&tokens);
    for index in 0..tokens.len() {
        if command_new_open(&tokens, index).is_some() {
            let open = command_new_open(&tokens, index).expect("checked above");
            let close = matching_delimiter(&tokens, open, '(', ')').ok_or_else(|| {
                ManifestError::Validation(format!(
                    "unterminated compatibility process construction: {relative}"
                ))
            })?;
            let command = constant_expression_value(&tokens[open + 1..close], &constants)
                .unwrap_or_default()
                .to_ascii_lowercase();
            if is_forbidden_process_name(&command) {
                return legacy_reference_error(relative, "process");
            }
            if is_shell_process_name(&command) {
                let statement_end = tokens[close + 1..]
                    .iter()
                    .position(|token| matches!(token, RustSourceToken::Punct(';')))
                    .map_or(tokens.len(), |offset| close + 1 + offset);
                let arguments =
                    constant_expression_value(&tokens[close + 1..statement_end], &constants)
                        .unwrap_or_default();
                if contains_forbidden_shell_edge(&arguments) {
                    return legacy_reference_error(relative, "process");
                }
            }
        }

        if let Some(open) = source_read_open(&tokens, index) {
            let close = matching_delimiter(&tokens, open, '(', ')').ok_or_else(|| {
                ManifestError::Validation(format!(
                    "unterminated compatibility source read: {relative}"
                ))
            })?;
            let source_path =
                constant_expression_value(&tokens[open + 1..close], &constants).unwrap_or_default();
            if is_forbidden_product_source(&source_path) {
                return legacy_reference_error(relative, "source");
            }
        }
    }
    Ok(())
}

fn collect_string_constants(tokens: &[RustSourceToken]) -> BTreeMap<String, String> {
    let mut constants = BTreeMap::new();
    let mut index = 0;
    while index < tokens.len() {
        if matches!(&tokens[index], RustSourceToken::Identifier(value) if value == "const")
            && let Some(RustSourceToken::Identifier(name)) = tokens.get(index + 1)
        {
            let Some(equal) = tokens[index + 2..]
                .iter()
                .position(|token| matches!(token, RustSourceToken::Punct('=')))
                .map(|offset| index + 2 + offset)
            else {
                index += 1;
                continue;
            };
            let Some(end) = tokens[equal + 1..]
                .iter()
                .position(|token| matches!(token, RustSourceToken::Punct(';')))
                .map(|offset| equal + 1 + offset)
            else {
                index += 1;
                continue;
            };
            if let Some(value) = constant_expression_value(&tokens[equal + 1..end], &constants) {
                constants.insert(name.clone(), value);
            }
            index = end;
        }
        index += 1;
    }
    constants
}

fn constant_expression_value(
    tokens: &[RustSourceToken],
    constants: &BTreeMap<String, String>,
) -> Option<String> {
    let mut value = String::new();
    let mut found = false;
    for token in tokens {
        match token {
            RustSourceToken::StringLiteral(fragment) => {
                value.push_str(fragment);
                found = true;
            }
            RustSourceToken::Identifier(name) if constants.contains_key(name) => {
                value.push_str(constants.get(name).expect("constant checked above"));
                found = true;
            }
            RustSourceToken::Identifier(name)
                if matches!(name.as_str(), "concat" | "format" | "env" | "option_env") => {}
            RustSourceToken::Identifier(_) | RustSourceToken::Punct(_) => {}
        }
    }
    found.then_some(value)
}

fn command_new_open(tokens: &[RustSourceToken], index: usize) -> Option<usize> {
    matches!(tokens.get(index), Some(RustSourceToken::Identifier(value)) if value == "Command")
        .then_some(())?;
    matches!(tokens.get(index + 1), Some(RustSourceToken::Punct(':'))).then_some(())?;
    matches!(tokens.get(index + 2), Some(RustSourceToken::Punct(':'))).then_some(())?;
    matches!(tokens.get(index + 3), Some(RustSourceToken::Identifier(value)) if value == "new")
        .then_some(())?;
    matches!(tokens.get(index + 4), Some(RustSourceToken::Punct('('))).then_some(index + 4)
}

fn source_read_open(tokens: &[RustSourceToken], index: usize) -> Option<usize> {
    let RustSourceToken::Identifier(name) = tokens.get(index)? else {
        return None;
    };
    if !matches!(
        name.as_str(),
        "include" | "include_str" | "include_bytes" | "read" | "read_to_string" | "open"
    ) {
        return None;
    }
    let mut open = index + 1;
    if matches!(tokens.get(open), Some(RustSourceToken::Punct('!'))) {
        open += 1;
    }
    matches!(tokens.get(open), Some(RustSourceToken::Punct('('))).then_some(open)
}

fn matching_delimiter(
    tokens: &[RustSourceToken],
    open: usize,
    opening: char,
    closing: char,
) -> Option<usize> {
    let mut depth = 0_usize;
    for (index, token) in tokens.iter().enumerate().skip(open) {
        match token {
            RustSourceToken::Punct(value) if *value == opening => depth += 1,
            RustSourceToken::Punct(value) if *value == closing => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

fn is_forbidden_process_name(value: &str) -> bool {
    let normalized = value.replace('\\', "/");
    let executable = normalized.rsplit('/').next().unwrap_or(&normalized);
    matches!(
        executable,
        "node" | "node.exe" | "npm" | "npm.cmd" | "npx" | "npx.cmd" | "tsc" | "tsc.cmd"
    )
}

fn is_shell_process_name(value: &str) -> bool {
    let normalized = value.replace('\\', "/");
    let executable = normalized.rsplit('/').next().unwrap_or(&normalized);
    matches!(
        executable,
        "sh" | "bash" | "cmd" | "cmd.exe" | "powershell" | "powershell.exe" | "pwsh" | "pwsh.exe"
    )
}

fn contains_forbidden_shell_edge(value: &str) -> bool {
    let normalized = value.replace('\\', "/").to_ascii_lowercase();
    is_forbidden_product_source(&normalized)
        || normalized.contains("npm run build")
        || normalized.contains("tsc -p")
        || normalized
            .split(|character: char| {
                !(character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_'))
            })
            .any(is_forbidden_process_name)
}

fn is_forbidden_product_source(value: &str) -> bool {
    let normalized = value.replace('\\', "/").to_ascii_lowercase();
    normalized.contains("dist/cli.js")
        || normalized
            .split('/')
            .collect::<Vec<_>>()
            .windows(2)
            .any(|window| {
                window[0] == "src" && (window[1].ends_with(".ts") || window[1].ends_with(".tsx"))
            })
}

fn legacy_reference_error<T>(relative: &str, class: &str) -> Result<T, ManifestError> {
    validation(format!(
        "compatibility source {class} reference denied: {relative}"
    ))
}

fn tokenize_rust_source(
    relative: &str,
    source: &str,
) -> Result<Vec<RustSourceToken>, ManifestError> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte.is_ascii_whitespace() {
            index += 1;
        } else if byte == b'/' && bytes.get(index + 1) == Some(&b'/') {
            index += 2;
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
        } else if byte == b'/' && bytes.get(index + 1) == Some(&b'*') {
            index = skip_block_comment(relative, bytes, index)?;
        } else if byte == b'r'
            && (bytes.get(index + 1) == Some(&b'"') || bytes.get(index + 1) == Some(&b'#'))
        {
            let (value, next) = read_raw_rust_string(relative, bytes, index)?;
            tokens.push(RustSourceToken::StringLiteral(value));
            index = next;
        } else if byte == b'"' {
            let (value, next) = read_rust_string(relative, bytes, index)?;
            tokens.push(RustSourceToken::StringLiteral(value));
            index = next;
        } else if byte == b'\'' {
            let lifetime = bytes
                .get(index + 1)
                .is_some_and(|next| next.is_ascii_alphabetic() || *next == b'_')
                && bytes.get(index + 2) != Some(&b'\'');
            if lifetime {
                tokens.push(RustSourceToken::Punct('\''));
                index += 1;
            } else {
                index = skip_rust_character(relative, bytes, index)?;
            }
        } else if byte.is_ascii_alphabetic() || byte == b'_' {
            let start = index;
            index += 1;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
            {
                index += 1;
            }
            tokens.push(RustSourceToken::Identifier(
                String::from_utf8_lossy(&bytes[start..index]).into_owned(),
            ));
        } else {
            tokens.push(RustSourceToken::Punct(char::from(byte)));
            index += 1;
        }
    }
    Ok(tokens)
}

fn skip_block_comment(relative: &str, bytes: &[u8], start: usize) -> Result<usize, ManifestError> {
    let mut depth = 1_usize;
    let mut index = start + 2;
    while index + 1 < bytes.len() {
        if bytes[index] == b'/' && bytes[index + 1] == b'*' {
            depth += 1;
            index += 2;
        } else if bytes[index] == b'*' && bytes[index + 1] == b'/' {
            depth -= 1;
            index += 2;
            if depth == 0 {
                return Ok(index);
            }
        } else {
            index += 1;
        }
    }
    validation(format!(
        "unterminated block comment in compatibility source: {relative}"
    ))
}

fn read_rust_string(
    relative: &str,
    bytes: &[u8],
    start: usize,
) -> Result<(String, usize), ManifestError> {
    let mut value = String::new();
    let mut index = start + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'"' => return Ok((value, index + 1)),
            b'\\' => {
                let Some(escaped) = bytes.get(index + 1).copied() else {
                    break;
                };
                match escaped {
                    b'\\' => value.push('\\'),
                    b'/' => value.push('/'),
                    b'\'' => value.push('\''),
                    b'"' => value.push('"'),
                    b'n' => value.push('\n'),
                    b'r' => value.push('\r'),
                    b't' => value.push('\t'),
                    _ => value.push(char::from(escaped)),
                }
                index += 2;
            }
            byte => {
                value.push(char::from(byte));
                index += 1;
            }
        }
    }
    validation(format!(
        "unterminated string in compatibility source: {relative}"
    ))
}

fn read_raw_rust_string(
    relative: &str,
    bytes: &[u8],
    start: usize,
) -> Result<(String, usize), ManifestError> {
    let mut quote = start + 1;
    while bytes.get(quote) == Some(&b'#') {
        quote += 1;
    }
    if bytes.get(quote) != Some(&b'"') {
        return validation(format!(
            "invalid raw string in compatibility source: {relative}"
        ));
    }
    let hashes = quote - start - 1;
    let content_start = quote + 1;
    let mut index = content_start;
    while index < bytes.len() {
        if bytes[index] == b'"'
            && bytes.get(index + 1..index + 1 + hashes) == Some(&vec![b'#'; hashes])
        {
            return Ok((
                String::from_utf8_lossy(&bytes[content_start..index]).into_owned(),
                index + 1 + hashes,
            ));
        }
        index += 1;
    }
    validation(format!(
        "unterminated raw string in compatibility source: {relative}"
    ))
}

fn skip_rust_character(relative: &str, bytes: &[u8], start: usize) -> Result<usize, ManifestError> {
    let mut index = start + 1;
    if bytes.get(index) == Some(&b'\\') {
        index += 2;
    } else {
        index += 1;
    }
    if bytes.get(index) == Some(&b'\'') {
        Ok(index + 1)
    } else {
        validation(format!(
            "invalid character literal in compatibility source: {relative}"
        ))
    }
}

fn validation<T>(message: impl Into<String>) -> Result<T, ManifestError> {
    Err(ManifestError::Validation(message.into()))
}

pub fn verify_fixture_compatibility(
    root: &Path,
    require_hosted_evidence: bool,
) -> Result<(), String> {
    let mode = if require_hosted_evidence {
        HostedEvidenceMode::Final
    } else {
        HostedEvidenceMode::None
    };
    verify_fixture_compatibility_mode(root, mode)
}

pub fn verify_fixture_compatibility_strict_precondition(root: &Path) -> Result<(), String> {
    verify_fixture_compatibility_mode(root, HostedEvidenceMode::CandidatePrecondition)
}

#[derive(Clone, Copy)]
enum HostedEvidenceMode {
    None,
    CandidatePrecondition,
    Final,
}

fn verify_fixture_compatibility_mode(
    root: &Path,
    hosted_evidence_mode: HostedEvidenceMode,
) -> Result<(), String> {
    validate_compatibility_source_boundary(root).map_err(|error| error.to_string())?;
    verify_provider_evaluation(root).map_err(|error| error.to_string())?;
    verify_retrieval_evaluation(root).map_err(|error| error.to_string())?;

    let manifests =
        crate::manifest::load_compat_manifests(root).map_err(|error| error.to_string())?;
    validate_rust_command_surface(&manifests.commands).map_err(|error| error.to_string())?;
    validate_rust_tool_evidence(root, &manifests.public_contract)
        .map_err(|error| error.to_string())?;
    validate_rust_vault_evidence(root).map_err(|error| error.to_string())?;
    validate_rust_retrieval_evidence(root).map_err(|error| error.to_string())?;
    validate_rust_provider_profiles(&manifests.providers).map_err(|error| error.to_string())?;
    validate_product_entry(root).map_err(|error| error.to_string())?;
    match hosted_evidence_mode {
        HostedEvidenceMode::None => {
            validate_cutover_candidate(root, &manifests.public_contract)
                .map_err(|error| error.to_string())?;
        }
        HostedEvidenceMode::CandidatePrecondition => {
            validate_cutover_strict_precondition(root, &manifests.public_contract)
                .map_err(|error| error.to_string())?;
        }
        HostedEvidenceMode::Final => {
            validate_cutover_evidence(root, &manifests.public_contract)
                .map_err(|error| error.to_string())?;
        }
    }

    let architecture = load_cargo_architecture(root).map_err(|error| error.to_string())?;
    validate_architecture(&architecture).map_err(|error| error.to_string())?;
    validate_core_source_boundary(root).map_err(|error| error.to_string())?;
    validate_vault_source_boundary(root).map_err(|error| error.to_string())?;
    validate_cli_tui_markdown_boundary(root).map_err(|error| error.to_string())?;
    validate_retrieval_source_boundary(root).map_err(|error| error.to_string())?;
    validate_migration_source_boundary(root).map_err(|error| error.to_string())?;

    let first = build_report(&manifests, root).map_err(|error| error.to_string())?;
    validate_report(&first, &manifests, root).map_err(|error| error.to_string())?;
    let first_json = report_json(&first).map_err(|error| error.to_string())?;
    let second_manifests =
        crate::manifest::load_compat_manifests(root).map_err(|error| error.to_string())?;
    let second = build_report(&second_manifests, root).map_err(|error| error.to_string())?;
    validate_report(&second, &second_manifests, root).map_err(|error| error.to_string())?;
    let second_json = report_json(&second).map_err(|error| error.to_string())?;
    if first_json != second_json {
        return Err("compatibility report is not deterministic".to_owned());
    }
    Ok(())
}

fn load_approved_differences(
    root: &Path,
    manifests: &CompatManifests,
) -> Result<BTreeMap<String, ApprovedDifference>, ManifestError> {
    let raw =
        fs::read_to_string(root.join(DIFFERENCE_FIXTURE)).map_err(|_| ManifestError::Read {
            file: DIFFERENCE_FIXTURE.to_owned(),
        })?;
    let fixture: CommandDifferenceFixture =
        serde_json::from_str(&raw).map_err(|_| ManifestError::Parse {
            file: DIFFERENCE_FIXTURE.to_owned(),
        })?;
    if fixture.schema_version != 1 {
        return Err(ManifestError::Validation(
            "unsupported approved-difference schema version".to_owned(),
        ));
    }

    let expected_links = manifests
        .public_contract
        .items
        .iter()
        .filter_map(|item| item.approved_difference.clone())
        .collect::<BTreeSet<_>>();
    let mut by_id = BTreeMap::new();
    let mut commands = BTreeSet::new();
    for difference in fixture.differences {
        if !difference.id.starts_with("difference.command.")
            || !commands.insert(difference.command.clone())
            || difference.locked_outcome.trim().is_empty()
            || difference.rust_behavior.len() < 24
            || difference.reason.len() < 24
            || difference.safety.len() < 24
            || by_id.insert(difference.id.clone(), difference).is_some()
        {
            return Err(ManifestError::Validation(
                "approved command differences must be unique, complete, and stable".to_owned(),
            ));
        }
    }
    if by_id.keys().cloned().collect::<BTreeSet<_>>() != expected_links {
        return Err(ManifestError::Validation(
            "approved command differences must exactly match public-contract links".to_owned(),
        ));
    }
    for (id, difference) in &by_id {
        let contract_id = format!("contract.command.{}", difference.command);
        let item = manifests
            .public_contract
            .items
            .iter()
            .find(|item| item.id == contract_id)
            .ok_or_else(|| {
                ManifestError::Validation(format!(
                    "approved difference has no public command: {id}"
                ))
            })?;
        if item.status != ParityStatus::ApprovedDifference
            || item.approved_difference.as_deref() != Some(id.as_str())
        {
            return Err(ManifestError::Validation(format!(
                "approved difference is not linked by its public command: {id}"
            )));
        }
        let command = manifests
            .commands
            .commands
            .iter()
            .find(|command| command.name == difference.command)
            .ok_or_else(|| {
                ManifestError::Validation(format!(
                    "approved difference command is not canonical: {id}"
                ))
            })?;
        if command.outcome != difference.locked_outcome {
            return Err(ManifestError::Validation(format!(
                "approved difference outcome drift: {id}"
            )));
        }
    }
    Ok(by_id)
}
